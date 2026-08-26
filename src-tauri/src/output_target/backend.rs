//! Platform half of the output target: capturing a window and delivering to it.
//!
//! These operations sit behind one interface so the paste path never spells out
//! a platform:
//!   - [`capture_foreground_window`] -- what the lock toggle pins to,
//!   - [`capture_delivery`] -- the target one dictation is started for, read
//!     once at recording start and carried on its `DictationContext` (#160),
//!   - [`resolve_captured_delivery`] -- that captured target, re-checked
//!     immediately before its paste,
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

use super::{CaptureError, CaptureSource, LockToggle, PinnedTarget, TargetLoss, WindowIdentity};

/// Emitted when a pinned paste was suppressed because the locked window is gone
/// (#120). The frontend turns this into a brief notice; the lock is already
/// dropped by the time it fires, so the next dictation is a normal foreground
/// paste.
pub const TARGET_LOCK_LOST_EVENT: &str = "target-lock-lost";

/// Emitted when a delivery reached no window because the window that dictation
/// was started for had closed, while the lock the user can see has since moved
/// on and still stands (#160).
///
/// Distinct from [`TARGET_LOCK_LOST_EVENT`] because the two say different things
/// to the user: this one must NOT claim their current lock is gone. Without it a
/// suppressed delivery in this case is silent, and with the default
/// `ClipboardHandling::DontModify` the transcript is gone with it.
pub const TARGET_WINDOW_GONE_EVENT: &str = "target-window-gone";

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
/// `source` says which gesture asked, because the tray menu holds the foreground
/// itself while its click is handled (see [`capture_foreground_window`]). The
/// tray menu is rebuilt either way so its checkmark follows the real state.
pub fn toggle_target_lock(app: &AppHandle, source: CaptureSource) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized");
        return;
    };

    match pinned.toggle(|| capture_foreground_window(source)) {
        LockToggle::Locked(window) => info!(
            "Output locked to window {:#x} (process {})",
            window.handle.0, window.process_id
        ),
        LockToggle::Unlocked => info!("Output lock released; delivery follows the foreground"),
        LockToggle::NotLocked(error) => warn!("Could not lock the output target: {}", error),
    }

    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Capture where a dictation starting now will be delivered, for its
/// [`DictationContext`](crate::dictation_context::DictationContext) (#160).
///
/// Read once, at recording start, and it carries the whole [`WindowIdentity`]:
/// the same identity every later step re-checks, so the dictation is never
/// resolved from a bare handle the OS may have recycled (#254). Everything
/// downstream carries this value, so a lock toggled while the user is still
/// speaking governs the next dictation rather than the one in flight.
pub fn capture_delivery(app: &AppHandle) -> Delivery {
    let locked = match app.try_state::<PinnedTarget>() {
        Some(pinned) => pinned.locked(),
        None => {
            warn!("Target lock state is not initialized; delivering to the foreground");
            None
        }
    };

    match locked {
        Some(identity) => Delivery::Pinned(identity),
        None => Delivery::Foreground,
    }
}

/// Resolve delivery for the target this dictation captured at recording start,
/// or `None` when the paste must be suppressed because that window is gone
/// (#120).
///
/// The lock itself is deliberately not consulted: `captured` is what this
/// dictation was started for, so a lock toggled while the user was speaking can
/// neither redirect this paste nor rescue it. What is re-checked is the captured
/// identity, because a window can close during a dictation. A dead target is
/// dropped through [`drop_lock_for`], which clears the lock only if it still
/// points at this same window and announces the loss only when it cleared one.
pub fn resolve_captured_delivery(app: &AppHandle, captured: Delivery) -> Option<Delivery> {
    let Delivery::Pinned(identity) = captured else {
        return Some(Delivery::Foreground);
    };

    // The identity validated here is the one the context has carried since
    // recording started, so there is no read of the lock to race with: the
    // window checked is by construction the window this delivery will be aimed
    // at.
    if window_is_alive(identity) {
        Some(Delivery::Pinned(identity))
    } else {
        drop_lock_for(app, identity);
        None
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

/// Drop the lock on `target` because its window has gone, and tell the user the
/// transcript did not reach it.
///
/// Only this delivery's own target is cleared: the user may have unlocked and
/// re-locked to another window while this paste was running, and a dead target
/// from the older delivery must not take the newer lock down with it. That case
/// still has to be announced, though, in its own words -- the delivery failed
/// either way, and staying quiet about it loses a finished transcript without a
/// trace, since a suppressed delivery is deliberately not a paste error and the
/// default clipboard handling leaves no copy behind (#160).
fn drop_lock_for(app: &AppHandle, target: WindowIdentity) {
    let Some(loss) = app
        .try_state::<PinnedTarget>()
        .map(|pinned| pinned.retire_dead_target(target))
    else {
        warn!("Target lock state is not initialized; a dead target went unreported");
        return;
    };

    match loss {
        TargetLoss::LockCleared => announce_lock_lost(app),
        TargetLoss::ObsoleteTarget => announce_target_window_gone(app, target),
    }
}

/// Tell the user this transcript reached no window, without touching the lock or
/// the tray: the window that died is not the one they have locked now.
fn announce_target_window_gone(app: &AppHandle, target: WindowIdentity) {
    warn!(
        "Window {:#x} closed before its transcript was delivered; the current lock is untouched",
        target.handle.0
    );
    let _ = app.emit(TARGET_WINDOW_GONE_EVENT, ());
}

/// Whether the locked window is still the window that was locked. Wraps the
/// shared identity check with this platform's probe (#254).
pub fn window_is_alive(locked: WindowIdentity) -> bool {
    super::identity_is_alive(locked, probe_identity)
}

/// Why the target could not be confirmed as the window about to receive input.
///
/// The two cases need opposite handling, so they are distinct: a window that has
/// gone is a settled outcome the user has already been told about, while a
/// window that is still there but will not come forward is a failure the user
/// has to hear about, or a transcript disappears without a word (#120).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusLost {
    /// The target window has closed. Its lock is dropped and the notice sent.
    TargetGone,
    /// The window is alive, but the system would not bring it forward. The lock
    /// still stands, so a retry can work.
    ActivationRefused(String),
}

impl std::fmt::Display for FocusLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusLost::TargetGone => write!(f, "the locked window closed during delivery"),
            FocusLost::ActivationRefused(reason) => write!(f, "{}", reason),
        }
    }
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
    /// is sent, and this reports [`FocusLost::TargetGone`] so the caller sends
    /// nothing more. If the target merely lost focus, it is re-activated once; a
    /// refused activation is [`FocusLost::ActivationRefused`] rather than typing
    /// into the window that took focus.
    pub fn ensure(&self) -> Result<(), FocusLost> {
        let Some(target) = self.target else {
            return Ok(());
        };

        if !window_is_alive(target) {
            drop_lock_for(self.app, target);
            return Err(FocusLost::TargetGone);
        }

        if foreground_is(target) {
            return Ok(());
        }

        warn!("Target window lost focus mid-delivery; re-activating it");
        activate_target(target).map_err(FocusLost::ActivationRefused)
    }
}

/// Give `target` the foreground, run `action`, then hand focus back to the
/// window that had it.
///
/// The identity is re-validated here, not just when the target was resolved:
/// the paste path waits on the Enigo mutex in between, and Windows recycles
/// handle values, so a window that died in that gap could otherwise be
/// activated by a handle that now belongs to something else (#254).
/// A refused activation is reported as [`FocusLost::ActivationRefused`], not as
/// a suppression: the window is still there, so the delivery failed rather than
/// being called off, and the caller must say so instead of dropping the
/// transcript quietly.
pub fn borrow_focus<T>(
    app: &AppHandle,
    target: WindowIdentity,
    action: impl FnOnce() -> T,
) -> Result<Borrowed<T>, FocusLost> {
    if !window_is_alive(target) {
        drop_lock_for(app, target);
        return Ok(Borrowed::Suppressed);
    }

    // The whole identity of the window being borrowed from, not just its
    // handle: it can close while the transcript is being delivered, and handing
    // the foreground back through a handle Windows has since recycled would
    // raise a window the user never had (#254).
    let previous = foreground_identity();
    activate_target(target).map_err(FocusLost::ActivationRefused)?;
    let outcome = action();
    restore_foreground(previous, target);

    Ok(Borrowed::Delivered(outcome))
}

#[cfg(windows)]
pub use imp::{
    activate_target, capture_foreground_window, foreground_identity, foreground_is, probe_identity,
    restore_foreground,
};

#[cfg(not(windows))]
pub use fallback::{
    activate_target, capture_foreground_window, foreground_identity, foreground_is, probe_identity,
    restore_foreground,
};

#[cfg(windows)]
mod imp {
    use super::{CaptureError, CaptureSource, WindowIdentity};
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
    /// From the shortcut this is strictly the foreground window: the user
    /// pressed the key while looking at the window they mean, so if that window
    /// is not a usable target -- it is AudioBud's own, or the bare desktop --
    /// the honest answer is to refuse. Silently pinning some other window would
    /// send later dictation somewhere the user never chose.
    ///
    /// From the tray menu the foreground cannot be trusted at all: while the
    /// menu item's callback runs, the shell's taskbar (or AudioBud's own menu
    /// window) holds the foreground. Tauri's tray API reports the menu click,
    /// not the window that was in front before the menu opened, and polling the
    /// foreground on a timer just to have an answer ready is a background cost
    /// paid for a rare click. So that path falls back to the top window in Z
    /// order a user could dictate into -- which, right behind the shell's
    /// surfaces, is the window they were last working in.
    pub fn capture_foreground_window(
        source: CaptureSource,
    ) -> Result<WindowIdentity, CaptureError> {
        let own_process_id = std::process::id();

        let foreground = unsafe { GetForegroundWindow() };
        if let Some(window) = eligible_identity(foreground, own_process_id) {
            return Ok(window);
        }

        if source == CaptureSource::TrayMenu {
            if let Some(window) = top_eligible_window(own_process_id) {
                return Ok(window);
            }
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

    /// The window that currently holds the foreground, with its identity, if
    /// there is one.
    pub fn foreground_identity() -> Option<WindowIdentity> {
        identity_of(unsafe { GetForegroundWindow() })
    }

    /// Whether `target` is the window that currently holds the foreground.
    pub fn foreground_is(target: WindowIdentity) -> bool {
        let hwnd = unsafe { GetForegroundWindow() };
        !hwnd.0.is_null() && from_hwnd(hwnd) == target.handle
    }

    /// Bring the locked window to the foreground.
    pub fn activate_target(target: WindowIdentity) -> Result<(), String> {
        activate(to_hwnd(target.handle))
    }

    /// Hand the foreground back to whatever held it before the borrow.
    ///
    /// The window is re-validated first: it may have closed while the transcript
    /// was being delivered, and Windows recycles handles, so activating the bare
    /// handle could raise an unrelated window instead (#254). The transcript is
    /// already delivered by this point, so a hand-back that is skipped or fails
    /// is reported, not propagated.
    pub fn restore_foreground(previous: Option<WindowIdentity>, target: WindowIdentity) {
        let Some(previous) = previous else {
            return;
        };
        if previous.handle == target.handle {
            return;
        }
        if !super::window_is_alive(previous) {
            warn!("Previous foreground window is gone; leaving focus where it is");
            return;
        }
        if let Err(e) = activate(to_hwnd(previous.handle)) {
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
    use super::{CaptureError, CaptureSource, WindowIdentity};
    use crate::output_target::WindowHandle;

    /// No window-targeting backend on this platform yet (#119), so nothing can
    /// be locked and the paste path always sees `Foreground`.
    pub fn capture_foreground_window(
        _source: CaptureSource,
    ) -> Result<WindowIdentity, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    /// Unreachable while capture is unsupported. `None` reads as "not alive",
    /// which drops any lock that somehow exists rather than pasting into it.
    pub fn probe_identity(_handle: WindowHandle) -> Option<(u32, u32)> {
        None
    }

    pub fn foreground_identity() -> Option<WindowIdentity> {
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

    pub fn restore_foreground(_previous: Option<WindowIdentity>, _target: WindowIdentity) {}
}
