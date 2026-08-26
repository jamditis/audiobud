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
use tauri_specta::Event;

use super::{
    CaptureError, CaptureSource, LockToggle, LockedLabel, LostLockNotice, OutputTargetLockEvent,
    PinnedTarget, WindowIdentity, WindowLabel,
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
/// tray menu is rebuilt either way so its checkmark follows the real state,
/// and [`OutputTargetLockEvent`] is emitted so the indicator surfaces (#255)
/// follow along too.
pub fn toggle_target_lock(app: &AppHandle, source: CaptureSource) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized");
        return;
    };

    match pinned.toggle(|| capture_foreground_window(source)) {
        LockToggle::Locked(window) => {
            info!(
                "Output locked to window {:#x} (process {})",
                window.handle.0, window.process_id
            );
            // A fresh lock supersedes whatever the tray remembered about the
            // last one going stale (#266 review).
            if let Some(notice) = app.try_state::<LostLockNotice>() {
                notice.clear();
            }
            // The window is guaranteed alive right now, which is the one
            // moment its label is reliably queryable -- cache it so a later
            // loss (#266 review) can still name it after it (and often its
            // whole process) is gone.
            let label = window_label(window);
            if let Some(cache) = app.try_state::<LockedLabel>() {
                cache.set(window, label.clone());
            }
            let (app_name, title) = label;
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
        // ("lost") latch, which the backend already unlocked when it
        // happened. The tray's own memory of that loss still needs clearing
        // here, or it would keep showing "lock lost" after the overlay's
        // latch was dismissed (#266 review). The dismissal must also be
        // emitted (#266 review, finding 3): the webview that dismissed
        // already updated itself optimistically, but a second webview
        // showing the same stale latch (another settings window, the
        // overlay) would otherwise never hear about the dismissal and stay
        // stuck on "stale" until an unrelated lock/unlock event happened to
        // pass through.
        let had_notice = app
            .try_state::<LostLockNotice>()
            .is_some_and(|notice| notice.clear());
        if had_notice {
            let _ = OutputTargetLockEvent::Unlocked.emit(app);
            crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
        }
        return;
    }
    pinned.unlock();
    if let Some(notice) = app.try_state::<LostLockNotice>() {
        notice.clear();
    }
    info!("Output lock released from the indicator");
    let _ = OutputTargetLockEvent::Unlocked.emit(app);
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Read the current lock state for the indicator surfaces (#255).
///
/// Reports [`OutputTargetLockEvent::Lost`] when nothing is locked but
/// [`LostLockNotice`] still remembers the last loss (#266 review, finding 1).
/// The `Lost` kind was originally event-only, on the theory that a mount
/// after the loss could just read `Unlocked` -- but the event fires once,
/// to whichever webview happens to be listening at that moment, and a
/// second webview mounting afterwards (settings opened after the overlay
/// already showed the stale target, say) missed it entirely and quietly
/// disagreed with the tray, which does consult the notice. Consulting it
/// here too makes the notice authoritative across every webview, not just
/// the one that was listening when the loss happened.
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
        None => app
            .try_state::<LostLockNotice>()
            .and_then(|notice| notice.get())
            .map(|(app_name, title)| OutputTargetLockEvent::Lost {
                app: app_name,
                title,
            })
            .unwrap_or(OutputTargetLockEvent::Unlocked),
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
pub fn resolve_paste_target(app: &AppHandle) -> Option<Delivery> {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        return Some(Delivery::Foreground);
    };

    // Read before resolve() so a lost lock's label can still be reported --
    // resolve() clears the lock in the same step that reports LockLost.
    let locked_before = pinned.locked();

    // resolve hands back the identity it validated under its own guard, so no
    // second read of the lock can slip a different window in between.
    match pinned.resolve(window_is_alive) {
        super::Resolved::Foreground => Some(Delivery::Foreground),
        super::Resolved::Pinned(identity) => Some(Delivery::Pinned(identity)),
        super::Resolved::LockLost => {
            let label = locked_before
                .map(|identity| lost_label(app, identity))
                .unwrap_or((None, None));
            announce_lock_lost(app, label);
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

/// The label to report for a window that just turned out to be gone.
///
/// Prefers whatever [`LockedLabel`] cached for `identity` while the window
/// was still alive: by the time a loss is discovered, `window_label` querying
/// live routinely comes back `(None, None)` -- the window's `GetWindowTextW`
/// fails outright, and often its whole owning process has exited too, so
/// `OpenProcess` fails as well (#266 review). A live query is still the
/// fallback for the rare case nothing was cached (state not yet managed, or
/// this identity was never the one actually locked).
fn lost_label(app: &AppHandle, identity: WindowIdentity) -> WindowLabel {
    app.try_state::<LockedLabel>()
        .and_then(|cache| cache.get(identity))
        .unwrap_or_else(|| window_label(identity))
}

/// Tell the user the lock is gone, once, and put the tray and indicator
/// surfaces back in step (#255).
///
/// `label` is the locked window's last known app/title, read by the caller
/// before the lock was dropped -- by the time this runs the lock is already
/// gone, so this is the only chance to report who it was.
///
/// The bare toast (`TARGET_LOCK_LOST_EVENT`) always fires: a real paste
/// attempt to the old target really did fail, whatever has happened since.
/// The *persistent* state -- `LostLockNotice` and `OutputTargetLockEvent::Lost`
/// -- is conditioned on [`super::record_lost_notice`], which checks whether a
/// new lock has already been established elsewhere since this loss was
/// detected (#266 review, finding 2). If so, that lock's own `Locked` event
/// is already the truth, and persisting `Lost` here would land after it and
/// contradict it -- the indicator would show "stale" while the backend is
/// actually pinned to something else.
fn announce_lock_lost(app: &AppHandle, label: WindowLabel) {
    warn!("Locked window is gone; the transcript was not delivered to it");
    let _ = app.emit(TARGET_LOCK_LOST_EVENT, ());

    let (Some(pinned), Some(notice)) = (
        app.try_state::<PinnedTarget>(),
        app.try_state::<LostLockNotice>(),
    ) else {
        return;
    };
    if !super::record_lost_notice(&pinned, &notice, label.clone()) {
        return;
    }

    let (app_name, title) = label;
    let _ = OutputTargetLockEvent::Lost {
        app: app_name,
        title,
    }
    .emit(app);
    // The lock is already released, so the tray checkmark and the indicator
    // surfaces would otherwise keep claiming a lock that no longer exists.
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Drop the lock on `target` because its window has gone, and tell the user.
///
/// Only this delivery's own target is cleared: the user may have unlocked and
/// re-locked to another window while this paste was running, and a dead target
/// from the older delivery must not take the newer lock down with it. When the
/// lock has already moved on, there is nothing to announce -- the lock the user
/// can see is still good -- but this delivery is abandoned all the same.
fn drop_lock_for(app: &AppHandle, target: WindowIdentity) {
    let cleared = app
        .try_state::<PinnedTarget>()
        .is_some_and(|pinned| pinned.unlock_if(target));
    if cleared {
        // The window died mid-delivery: prefer the label cached from when it
        // was locked (#266 review) over a fresh query, which routinely comes
        // back empty for a window that just closed.
        announce_lock_lost(app, lost_label(app, target));
    }
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
    use super::{CaptureError, CaptureSource, WindowIdentity, WindowLabel};
    use crate::output_target::{class_fingerprint, is_eligible_target, WindowFacts, WindowHandle};
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
        GetClassNameW, GetForegroundWindow, GetTopWindow, GetWindow, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
        GW_HWNDNEXT,
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

    /// The identity of whatever window holds `handle` right now, or `None` if
    /// no window holds it any more. Shared by the target lock and the picker
    /// (#124) so both judge a handle the same way.
    pub fn probe_identity(handle: WindowHandle) -> Option<WindowIdentity> {
        identity_of(to_hwnd(handle))
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
    pub fn window_label(identity: WindowIdentity) -> WindowLabel {
        (
            process_name(identity.process_id),
            window_title(to_hwnd(identity.handle)),
        )
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
            // Recorded here, at every capture and every probe alike, so the two
            // are always comparable (#254).
            class: class_fingerprint(&class_name_of(hwnd)),
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
    use super::{CaptureError, CaptureSource, WindowIdentity, WindowLabel};
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
    pub fn probe_identity(_handle: WindowHandle) -> Option<WindowIdentity> {
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

    /// No label backend on this platform yet (#119, #255): nothing can be
    /// locked, so this is unreachable, but it reports "unknown" rather than
    /// fabricate a name.
    pub fn window_label(_identity: WindowIdentity) -> WindowLabel {
        (None, None)
    }
}
