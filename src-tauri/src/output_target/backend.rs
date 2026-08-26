//! Platform half of the output target: capturing a window and delivering to it.
//!
//! Three operations sit behind one interface so the paste path never spells out
//! a platform:
//!   - [`capture_foreground_window`] -- what the lock toggle pins to,
//!   - [`window_is_alive`] -- the identity re-check [`PinnedTarget::resolve`]
//!     runs before every pinned paste (#254),
//!   - [`borrow_focus`] -- run the normal paste against a pinned window, then
//!     give focus back.
//!
//! Windows is the only backend for now (#119, #120). Elsewhere capture reports
//! [`CaptureError::Unsupported`], so no lock can ever be taken and the other two
//! are unreachable; they still fail closed rather than paste somewhere unasked.

use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager};

use super::{CaptureError, LockToggle, PinnedTarget, WindowHandle, WindowIdentity};

/// Emitted when a pinned paste was suppressed because the locked window is gone
/// (#120). The frontend turns this into a brief notice; the lock is already
/// dropped by the time it fires, so the next dictation is a normal foreground
/// paste.
pub const TARGET_LOCK_LOST_EVENT: &str = "target-lock-lost";

/// Toggle the target lock from the tray item or the shortcut.
///
/// Locking captures the window focused at that moment; pressing again unlocks.
/// The tray menu is rebuilt either way so its checkmark follows the real state.
pub fn toggle_target_lock(app: &AppHandle) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized");
        return;
    };

    match pinned.toggle(capture_foreground_window) {
        LockToggle::Locked(window) => info!(
            "Output locked to window {:#x} (process {})",
            window.handle.0, window.process_id
        ),
        LockToggle::Unlocked => info!("Output lock released; delivery follows the foreground"),
        LockToggle::NotLocked(error) => warn!("Could not lock the output target: {}", error),
    }

    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Resolve where the paste about to fire is delivered, or `None` when it must be
/// suppressed because the locked window is gone (#120). Suppression emits
/// [`TARGET_LOCK_LOST_EVENT`] once and leaves the app unlocked.
pub fn resolve_paste_target(app: &AppHandle) -> Option<super::OutputTarget> {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return Some(super::OutputTarget::Foreground);
    };

    match pinned.resolve(window_is_alive) {
        super::Resolved::Deliver(target) => Some(target),
        super::Resolved::LockLost => {
            warn!("Locked window is gone; the transcript was not pasted");
            let _ = app.emit(TARGET_LOCK_LOST_EVENT, ());
            // resolve() already released the lock, so the tray checkmark would
            // otherwise keep claiming a lock that no longer exists.
            crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
            None
        }
    }
}

/// Whether the locked window is still the window that was locked. Wraps the
/// shared identity check with this platform's probe (#254).
pub fn window_is_alive(locked: WindowIdentity) -> bool {
    super::identity_is_alive(locked, probe_identity)
}

#[cfg(windows)]
pub use imp::{borrow_focus, capture_foreground_window, probe_identity};

#[cfg(not(windows))]
pub use fallback::{borrow_focus, capture_foreground_window, probe_identity};

#[cfg(windows)]
mod imp {
    use super::{CaptureError, WindowHandle, WindowIdentity};
    use log::warn;
    use std::ffi::c_void;
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    // AttachThreadInput lives in System::Threading in this windows-crate
    // version, not in UI::Input::KeyboardAndMouse where the docs file it.
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    /// How long the activated window gets to take focus before keystrokes are
    /// sent. Activation is asynchronous, so a paste sent immediately can reach
    /// the old window instead.
    const FOCUS_SETTLE: Duration = Duration::from_millis(30);

    fn to_hwnd(handle: WindowHandle) -> HWND {
        HWND(handle.0 as *mut c_void)
    }

    /// Capture the window that currently holds the foreground, refusing
    /// AudioBud's own windows (#164).
    pub fn capture_foreground_window() -> Result<WindowIdentity, CaptureError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return Err(CaptureError::NoForegroundWindow);
        }
        let identity = identity_of(hwnd).ok_or(CaptureError::NoForegroundWindow)?;
        super::super::accept_capture(identity, std::process::id())
    }

    /// The process and thread that own `handle` right now, or `None` if no
    /// window has that handle any more.
    pub fn probe_identity(handle: WindowHandle) -> Option<(u32, u32)> {
        identity_of(to_hwnd(handle)).map(|w| (w.process_id, w.thread_id))
    }

    /// Give `target` the foreground, run `action`, then hand focus back to the
    /// window that had it. The error path is deliberately closed: if the target
    /// cannot be activated, `action` never runs, so a transcript is not typed
    /// into whatever holds focus instead (#120).
    pub fn borrow_focus<T>(target: WindowHandle, action: impl FnOnce() -> T) -> Result<T, String> {
        let hwnd = to_hwnd(target);
        let previous = unsafe { GetForegroundWindow() };

        activate(hwnd)?;
        let outcome = action();

        if !previous.0.is_null() && previous.0 != hwnd.0 {
            if let Err(e) = activate(previous) {
                // The transcript is already delivered, so a failed hand-back is
                // reported, not propagated.
                warn!("Failed to restore the previous foreground window: {}", e);
            }
        }

        Ok(outcome)
    }

    fn identity_of(hwnd: HWND) -> Option<WindowIdentity> {
        let mut process_id = 0u32;
        // Returns 0 for a handle that is no longer a window, which covers the
        // IsWindow check as well as reading the owner.
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if thread_id == 0 {
            return None;
        }
        Some(WindowIdentity {
            handle: WindowHandle(hwnd.0 as isize),
            process_id,
            thread_id,
        })
    }

    /// Bring `hwnd` to the foreground.
    ///
    /// Windows refuses `SetForegroundWindow` unless the caller meets one of its
    /// foreground-change conditions, which AudioBud does not: the global hotkey
    /// is handled on a keyboard manager thread, not by foreground input.
    /// Attaching this thread's input queue to the target's makes the two count
    /// as one input context for the call, which is the documented workaround
    /// (#163). The attachment is always undone, including when activation fails.
    fn activate(hwnd: HWND) -> Result<(), String> {
        let this_thread = unsafe { GetCurrentThreadId() };
        let owner_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
        if owner_thread == 0 {
            return Err("target window no longer exists".to_string());
        }

        let attached = owner_thread != this_thread
            && unsafe { AttachThreadInput(this_thread, owner_thread, true) }.as_bool();

        let activated = unsafe { SetForegroundWindow(hwnd) }.as_bool();

        if attached {
            unsafe {
                let _ = AttachThreadInput(this_thread, owner_thread, false);
            }
        }

        if !activated {
            return Err("the system refused to activate the target window".to_string());
        }

        std::thread::sleep(FOCUS_SETTLE);
        Ok(())
    }
}

#[cfg(not(windows))]
mod fallback {
    use super::{CaptureError, WindowHandle, WindowIdentity};

    /// No window-targeting backend on this platform yet (#119), so nothing can
    /// be locked and the paste path always sees `Foreground`.
    pub fn capture_foreground_window() -> Result<WindowIdentity, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    /// Unreachable while capture is unsupported. `None` reads as "not alive",
    /// which drops any lock that somehow exists rather than pasting into it.
    pub fn probe_identity(_handle: WindowHandle) -> Option<(u32, u32)> {
        None
    }

    /// Unreachable while capture is unsupported, and fails closed: `action` is
    /// not run, so nothing is typed into an unintended window.
    pub fn borrow_focus<T>(
        _target: WindowHandle,
        _action: impl FnOnce() -> T,
    ) -> Result<T, String> {
        Err("window targeting is not supported on this platform".to_string())
    }
}
