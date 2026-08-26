//! Platform half of the output target: capturing a window and delivering to it.
//!
//! Four operations sit behind one interface so the paste path never spells out
//! a platform:
//!   - [`capture_foreground_window`] -- what the lock toggle pins to,
//!   - [`window_is_alive`] -- the identity re-check run before every pinned
//!     paste (#254),
//!   - [`borrow_focus`] -- run the normal paste against a pinned window, then
//!     give focus back,
//!   - [`FocusHold::ensure`] -- re-check, at every keystroke boundary inside
//!     that borrow, that the target still holds focus.
//!
//! Windows is the only backend for now (#119, #120). Elsewhere capture reports
//! [`CaptureError::Unsupported`], so no lock can ever be taken and the rest is
//! unreachable; it still fails closed rather than type somewhere unasked.

use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager};

use super::{CaptureError, LockToggle, PinnedTarget, WindowIdentity};

/// Emitted when a pinned paste was suppressed because the locked window is gone
/// (#120). The frontend turns this into a brief notice; the lock is already
/// dropped by the time it fires, so the next dictation is a normal foreground
/// paste.
pub const TARGET_LOCK_LOST_EVENT: &str = "target-lock-lost";

/// Where one paste is delivered. Carries the whole [`WindowIdentity`], not just
/// the handle, because every step of the delivery re-checks it (#254).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Whatever window holds focus when the paste fires.
    Foreground,
    /// The locked window.
    Pinned(WindowIdentity),
}

/// The result of a focus-borrowed delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Borrowed<T> {
    /// `action` ran against the target.
    Delivered(T),
    /// The target died between resolving and activating it, so `action` never
    /// ran and nothing was typed. The lock is already dropped and the notice
    /// already sent.
    Suppressed,
}

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
pub fn resolve_paste_target(app: &AppHandle) -> Option<Delivery> {
    // A one-shot pick (#124) routes THIS transcript and is then spent, so it is
    // consulted before the lock and overrides it for this paste only. A pick
    // whose window has gone suppresses the paste the same way a lost lock does.
    match crate::window_picker::backend::take_pick_target(app) {
        Some(crate::window_picker::PickDelivery::Deliver(window)) => {
            return Some(Delivery::Pinned(window))
        }
        Some(crate::window_picker::PickDelivery::PickLost) => return None,
        None => {}
    }

    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return Some(Delivery::Foreground);
    };

    match pinned.resolve(window_is_alive) {
        super::Resolved::Deliver(super::OutputTarget::Foreground) => Some(Delivery::Foreground),
        super::Resolved::Deliver(super::OutputTarget::Pinned(_)) => {
            // resolve kept the lock, so the identity it just validated is still
            // the one held. Falling back to Foreground if it somehow went is
            // wrong -- that is the wrong-app paste this feature prevents.
            match pinned.locked() {
                Some(identity) => Some(Delivery::Pinned(identity)),
                None => {
                    announce_lock_lost(app);
                    None
                }
            }
        }
        super::Resolved::LockLost => {
            announce_lock_lost(app);
            None
        }
    }
}

/// Tell the user the lock is gone, once, and put the tray back in step.
fn announce_lock_lost(app: &AppHandle) {
    warn!("Locked window is gone; the transcript was not delivered to it");
    let _ = app.emit(TARGET_LOCK_LOST_EVENT, ());
    // The lock is already released, so the tray checkmark would otherwise keep
    // claiming a lock that no longer exists.
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Whether the locked window is still the window that was locked. Wraps the
/// shared identity check with this platform's probe (#254).
pub fn window_is_alive(locked: WindowIdentity) -> bool {
    super::identity_is_alive(locked, probe_identity)
}

/// Keeps the target in focus for the length of one delivery.
///
/// Activation is a moment, not a lease: the user can click away, or a window can
/// steal focus, between the clipboard write and the paste keystroke, and the
/// delivery also sleeps (`paste_delay_ms`, the auto-submit gap). So the paste
/// path calls [`FocusHold::ensure`] immediately before each keystroke it sends.
/// A hold with no target is the foreground path, where every check passes.
pub struct FocusHold<'a> {
    app: &'a AppHandle,
    target: Option<WindowIdentity>,
}

impl<'a> FocusHold<'a> {
    /// A hold for `target`, or for the plain foreground path when `None`.
    pub fn new(app: &'a AppHandle, target: Option<WindowIdentity>) -> Self {
        Self { app, target }
    }

    /// Confirm the next keystroke will reach the intended window.
    ///
    /// Fails closed. If the target has closed, the lock is dropped, the notice
    /// is sent, and this errors so the caller sends nothing more. If the target
    /// merely lost focus, it is re-activated once; a refused activation is also
    /// an error rather than typing into the window that took focus.
    pub fn ensure(&self) -> Result<(), String> {
        let Some(target) = self.target else {
            return Ok(());
        };

        if !window_is_alive(target) {
            if let Some(pinned) = self.app.try_state::<PinnedTarget>() {
                pinned.unlock();
            }
            announce_lock_lost(self.app);
            return Err("the locked window closed during delivery".to_string());
        }

        if foreground_is(target) {
            return Ok(());
        }

        warn!("Target window lost focus mid-delivery; re-activating it");
        activate_target(target)
    }
}

/// Give `target` the foreground, run `action`, then hand focus back to the
/// window that had it.
///
/// The identity is re-validated here, not just when the target was resolved:
/// the paste path waits on the Enigo mutex in between, and Windows recycles
/// handle values, so a window that died in that gap could otherwise be
/// activated by a handle that now belongs to something else (#254).
pub fn borrow_focus<T>(
    app: &AppHandle,
    target: WindowIdentity,
    action: impl FnOnce() -> T,
) -> Result<Borrowed<T>, String> {
    if !window_is_alive(target) {
        if let Some(pinned) = app.try_state::<PinnedTarget>() {
            pinned.unlock();
        }
        announce_lock_lost(app);
        return Ok(Borrowed::Suppressed);
    }

    let previous = foreground_window();
    activate_target(target)?;
    let outcome = action();
    restore_foreground(previous, target);

    Ok(Borrowed::Delivered(outcome))
}

#[cfg(windows)]
pub use imp::{
    activate_target, capture_foreground_window, foreground_is, foreground_window, probe_identity,
    restore_foreground,
};

#[cfg(not(windows))]
pub use fallback::{
    activate_target, capture_foreground_window, foreground_is, foreground_window, probe_identity,
    restore_foreground,
};

#[cfg(windows)]
mod imp {
    use super::{CaptureError, WindowIdentity};
    use crate::output_target::{is_eligible_target, WindowFacts, WindowHandle};
    use log::warn;
    use std::ffi::c_void;
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    // AttachThreadInput lives in System::Threading in this windows-crate
    // version, not in UI::Input::KeyboardAndMouse where the docs file it.
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetTopWindow, GetWindow, GetWindowTextLengthW,
        GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, GW_HWNDNEXT,
    };

    /// How long the activated window gets to take focus before keystrokes are
    /// sent. Activation is asynchronous, so a paste sent immediately can reach
    /// the old window instead.
    const FOCUS_SETTLE: Duration = Duration::from_millis(30);

    /// Upper bound on the Z-order walk used when the foreground is not a usable
    /// target. Far past any realistic desktop, and it keeps a corrupted window
    /// list from spinning forever.
    const MAX_Z_ORDER_SCAN: usize = 500;

    fn to_hwnd(handle: WindowHandle) -> HWND {
        HWND(handle.0 as *mut c_void)
    }

    fn from_hwnd(hwnd: HWND) -> WindowHandle {
        WindowHandle(hwnd.0 as isize)
    }

    /// Capture the window to lock onto.
    ///
    /// Normally that is the foreground window. It is not when the capture is
    /// requested from the tray menu: handling that click, the shell's taskbar
    /// (or AudioBud's own menu window) holds the foreground, and pinning to
    /// either is useless. Tauri's tray API reports the menu click, not the
    /// window that was in front before the menu opened, and polling the
    /// foreground on a timer just to have an answer ready is a background cost
    /// paid for a rare click. So the fallback is the top window in Z order that
    /// a user could actually dictate into -- which, right after the shell's
    /// surfaces, is the window they were last working in.
    pub fn capture_foreground_window() -> Result<WindowIdentity, CaptureError> {
        let own_process_id = std::process::id();

        let foreground = unsafe { GetForegroundWindow() };
        if let Some(window) = eligible_identity(foreground, own_process_id) {
            return Ok(window);
        }

        if let Some(window) = top_eligible_window(own_process_id) {
            return Ok(window);
        }

        // Nothing to lock onto. Report the foreground being AudioBud's own as
        // such (#164), so the caller can say why rather than blame the desktop.
        match identity_of(foreground) {
            Some(identity) if identity.process_id == own_process_id => Err(CaptureError::OwnWindow),
            _ => Err(CaptureError::NoForegroundWindow),
        }
    }

    /// The process and thread that own `handle` right now, or `None` if no
    /// window has that handle any more.
    pub fn probe_identity(handle: WindowHandle) -> Option<(u32, u32)> {
        identity_of(to_hwnd(handle)).map(|w| (w.process_id, w.thread_id))
    }

    /// The window that currently holds the foreground, if any.
    pub fn foreground_window() -> Option<WindowHandle> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then(|| from_hwnd(hwnd))
    }

    /// Whether `target` is the window that currently holds the foreground.
    pub fn foreground_is(target: WindowIdentity) -> bool {
        foreground_window() == Some(target.handle)
    }

    /// Bring the locked window to the foreground.
    pub fn activate_target(target: WindowIdentity) -> Result<(), String> {
        activate(to_hwnd(target.handle))
    }

    /// Hand the foreground back to whatever held it before the borrow. The
    /// transcript is already delivered by this point, so a failed hand-back is
    /// reported, not propagated.
    pub fn restore_foreground(previous: Option<WindowHandle>, target: WindowIdentity) {
        let Some(previous) = previous else {
            return;
        };
        if previous == target.handle {
            return;
        }
        if let Err(e) = activate(to_hwnd(previous)) {
            warn!("Failed to restore the previous foreground window: {}", e);
        }
    }

    fn identity_of(hwnd: HWND) -> Option<WindowIdentity> {
        if hwnd.0.is_null() {
            return None;
        }
        let mut process_id = 0u32;
        // Returns 0 for a handle that is no longer a window, which covers the
        // IsWindow check as well as reading the owner.
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if thread_id == 0 {
            return None;
        }
        Some(WindowIdentity {
            handle: from_hwnd(hwnd),
            process_id,
            thread_id,
        })
    }

    /// The window's class name, empty when it cannot be read.
    fn class_name_of(hwnd: HWND) -> String {
        // 256 matches the documented maximum length of a registered class name.
        let mut buffer = [0u16; 256];
        let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..written as usize])
    }

    /// `hwnd` as a lockable target, or `None` if it is hidden, untitled, one of
    /// AudioBud's own windows, or a shell surface.
    fn eligible_identity(hwnd: HWND, own_process_id: u32) -> Option<WindowIdentity> {
        let identity = identity_of(hwnd)?;
        let class_name = class_name_of(hwnd);
        let facts = WindowFacts {
            identity,
            class_name: &class_name,
            has_title: unsafe { GetWindowTextLengthW(hwnd) } > 0,
            visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        };
        is_eligible_target(&facts, own_process_id).then_some(identity)
    }

    /// The first window in Z order a user could dictate into.
    fn top_eligible_window(own_process_id: u32) -> Option<WindowIdentity> {
        let mut hwnd = unsafe { GetTopWindow(None) }.ok()?;
        for _ in 0..MAX_Z_ORDER_SCAN {
            if let Some(identity) = eligible_identity(hwnd, own_process_id) {
                return Some(identity);
            }
            hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) }.ok()?;
        }
        None
    }

    /// Bring `hwnd` to the foreground.
    ///
    /// Windows refuses `SetForegroundWindow` unless the calling thread meets one
    /// of its foreground-change conditions, which AudioBud does not: the global
    /// hotkey is handled on a keyboard manager thread, not by foreground input.
    /// The privilege belongs to the thread that owns the CURRENT foreground
    /// window, so this thread attaches its input queue to that one -- not to the
    /// target's, which has no say in the matter -- for the length of the call
    /// (#163). The attachment is always undone, including when activation fails.
    fn activate(hwnd: HWND) -> Result<(), String> {
        if unsafe { GetWindowThreadProcessId(hwnd, None) } == 0 {
            return Err("target window no longer exists".to_string());
        }

        let this_thread = unsafe { GetCurrentThreadId() };
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_thread = if foreground.0.is_null() {
            0
        } else {
            unsafe { GetWindowThreadProcessId(foreground, None) }
        };

        let attached = foreground_thread != 0
            && foreground_thread != this_thread
            && unsafe { AttachThreadInput(this_thread, foreground_thread, true) }.as_bool();

        let activated = unsafe { SetForegroundWindow(hwnd) }.as_bool();

        if attached {
            unsafe {
                let _ = AttachThreadInput(this_thread, foreground_thread, false);
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
    use super::{CaptureError, WindowIdentity};
    use crate::output_target::WindowHandle;

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

    pub fn foreground_window() -> Option<WindowHandle> {
        None
    }

    /// Nothing can be confirmed to hold focus here, so every check falls through
    /// to an activation attempt, which fails closed below.
    pub fn foreground_is(_target: WindowIdentity) -> bool {
        false
    }

    /// Unreachable while capture is unsupported, and fails closed so no
    /// keystroke is sent to an unintended window.
    pub fn activate_target(_target: WindowIdentity) -> Result<(), String> {
        Err("window targeting is not supported on this platform".to_string())
    }

    pub fn restore_foreground(_previous: Option<WindowHandle>, _target: WindowIdentity) {}
}
