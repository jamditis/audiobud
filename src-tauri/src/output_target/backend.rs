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
use tauri_specta::Event;

use super::{
    CaptureError, LockToggle, OutputTargetLockEvent, PinnedTarget, WindowHandle, WindowIdentity,
};

/// Emitted when a pinned paste was suppressed because the locked window is gone
/// (#120). The frontend turns this into a brief notice; the lock is already
/// dropped by the time it fires, so the next dictation is a normal foreground
/// paste.
///
/// This is separate from [`OutputTargetLockEvent`] (#255): that one carries the
/// full `{kind, app, title}` state for the overlay/tray/settings indicator,
/// while this bare event exists only to trigger the toast in `App.tsx`.
pub const TARGET_LOCK_LOST_EVENT: &str = "target-lock-lost";

/// Toggle the target lock from the tray item or the shortcut.
///
/// Locking captures the window focused at that moment; pressing again unlocks.
/// The tray menu is rebuilt either way so its checkmark follows the real state,
/// and [`OutputTargetLockEvent`] is emitted so the indicator surfaces (#255)
/// follow along too.
pub fn toggle_target_lock(app: &AppHandle) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized");
        return;
    };

    match pinned.toggle(capture_foreground_window) {
        LockToggle::Locked(window) => {
            info!(
                "Output locked to window {:#x} (process {})",
                window.handle.0, window.process_id
            );
            let (app_name, title) = window_label(window);
            let _ = OutputTargetLockEvent::Locked {
                app: app_name,
                title,
            }
            .emit(app);
        }
        LockToggle::Unlocked => {
            info!("Output lock released; delivery follows the foreground");
            let _ = OutputTargetLockEvent::Unlocked.emit(app);
        }
        LockToggle::NotLocked(error) => warn!("Could not lock the output target: {}", error),
    }

    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Unlock the output target unconditionally, for the indicator's quick-unlock
/// affordance (#121). Unlike [`toggle_target_lock`], this never re-locks: the
/// indicator only offers it while a lock is shown (live or stale), and a
/// stale notice with no backend lock left to toggle would otherwise capture a
/// fresh, unwanted lock on the current foreground window.
pub fn unlock_output_target(app: &AppHandle) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return;
    };
    if !pinned.is_locked() {
        // Nothing to release -- most often the frontend dismissing a stale
        // ("lost") latch, which the backend already unlocked when it happened.
        return;
    }
    pinned.unlock();
    info!("Output lock released from the indicator");
    let _ = OutputTargetLockEvent::Unlocked.emit(app);
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Read the current lock state for the indicator surfaces (#255).
///
/// Never reports [`OutputTargetLockEvent::Lost`]: that kind is event-only,
/// emitted once when [`resolve_paste_target`] drops a stale lock. A snapshot
/// query after that loss reads `Unlocked`, matching the frontend contract in
/// `output-target-indicator.ts`.
#[tauri::command]
#[specta::specta]
pub fn get_output_target_lock(app: AppHandle) -> OutputTargetLockEvent {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return OutputTargetLockEvent::Unlocked;
    };
    match pinned.locked() {
        Some(window) => {
            let (app_name, title) = window_label(window);
            OutputTargetLockEvent::Locked {
                app: app_name,
                title,
            }
        }
        None => OutputTargetLockEvent::Unlocked,
    }
}

/// Release the output target lock from the indicator's quick-unlock button
/// (#121). A thin command wrapper around [`unlock_output_target`] so the
/// frontend has an explicit "unlock", distinct from the tray's lock/unlock
/// toggle.
#[tauri::command]
#[specta::specta]
pub fn release_output_target_lock(app: AppHandle) {
    unlock_output_target(&app);
}

/// Resolve where the paste about to fire is delivered, or `None` when it must be
/// suppressed because the locked window is gone (#120). Suppression emits
/// [`TARGET_LOCK_LOST_EVENT`] (for the toast) and [`OutputTargetLockEvent::Lost`]
/// (for the indicator, #255) once, and leaves the app unlocked.
pub fn resolve_paste_target(app: &AppHandle) -> Option<super::OutputTarget> {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return Some(super::OutputTarget::Foreground);
    };

    // Read before resolve() so a lost lock's label can still be reported --
    // resolve() clears the lock in the same step that reports LockLost.
    let locked_before = pinned.locked();

    match pinned.resolve(window_is_alive) {
        super::Resolved::Deliver(target) => Some(target),
        super::Resolved::LockLost => {
            warn!("Locked window is gone; the transcript was not pasted");
            let (app_name, title) = locked_before.map(window_label).unwrap_or((None, None));
            let _ = app.emit(TARGET_LOCK_LOST_EVENT, ());
            let _ = OutputTargetLockEvent::Lost {
                app: app_name,
                title,
            }
            .emit(app);
            // resolve() already released the lock, so the tray checkmark would
            // otherwise keep claiming a lock that no longer exists.
            crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
            None
        }
    }
}

/// Resolve a human-readable label for a locked window (#255): the app
/// (process) name and the window title. Best-effort -- either half can come
/// back `None` (a window with no title, a process query the OS refuses), and
/// the caller sends both to the frontend, which owns name precedence and
/// truncation (`output-target-indicator.ts`'s `resolveTargetName`).
#[cfg(windows)]
pub use imp::window_label;

#[cfg(not(windows))]
pub use fallback::window_label;

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
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    // AttachThreadInput lives in System::Threading in this windows-crate
    // version, not in UI::Input::KeyboardAndMouse where the docs file it.
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, SetForegroundWindow,
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

    /// The window title via `GetWindowTextW`, or `None` for a title-less or
    /// closed window. Trimmed; an all-whitespace title also reads as `None`.
    fn window_title(hwnd: HWND) -> Option<String> {
        let mut buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        let trimmed = title.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// The owning process's executable name (no directory, no extension), or
    /// `None` if the process cannot be opened or queried -- e.g. it exited, or
    /// it runs at a privilege level `PROCESS_QUERY_LIMITED_INFORMATION` cannot
    /// see into. `PROCESS_QUERY_LIMITED_INFORMATION` is used rather than a
    /// broader access right because it is the least the query needs and is
    /// available even for processes AudioBud does not own.
    fn process_name(process_id: u32) -> Option<String> {
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let queried = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        };
        unsafe {
            let _ = CloseHandle(handle);
        }
        queried.ok()?;
        if len == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
    }

    /// Resolve a locked window's app name and title (#255). Best-effort: a
    /// window that closed between capture and lookup, or a process query that
    /// fails, contributes `None` for that half rather than failing the whole
    /// lookup.
    pub fn window_label(identity: WindowIdentity) -> (Option<String>, Option<String>) {
        (
            process_name(identity.process_id),
            window_title(to_hwnd(identity.handle)),
        )
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

    /// No label backend on this platform yet (#119, #255): nothing can be
    /// locked, so this is unreachable, but it reports "unknown" rather than
    /// fabricate a name.
    pub fn window_label(_identity: WindowIdentity) -> (Option<String>, Option<String>) {
        (None, None)
    }
}
