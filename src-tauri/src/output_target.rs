//! Where a finished transcript is delivered: the output target.
//!
//! The default is [`OutputTarget::Foreground`] -- the paste lands in whatever
//! window is focused when it fires, which is AudioBud's behavior today. A user
//! can "lock" the currently focused window; while locked, every transcript goes
//! to that window regardless of what is focused at send time (issue #120). The
//! locked handle is held in Tauri-managed state ([`PinnedTarget`]) alongside
//! `EnigoState`. Each dictation reads it once, at recording start, into its
//! [`DictationContext`](crate::dictation_context::DictationContext); the paste
//! path then delivers to that captured target rather than re-reading the lock
//! (#160).
//!
//! This module is the platform-independent core: the lock/unlock state machine,
//! the compare-and-clear release a stale delivery uses, the window-identity
//! check that rejects a recycled handle (#254), and the self-window exclusion
//! (#164). [`backend`] holds the platform half: capturing a dictation's target,
//! re-checking it, and the focus-borrow paste (save foreground, activate the
//! pinned window, paste, restore), which is Windows-only for now (#119).
//!
//! Parts of the API are used only by the picker (#124), which lands later, so
//! the module allows dead_code.
#![allow(dead_code)]

pub mod backend;

use std::fmt;
use std::sync::Mutex;

/// A captured native window handle. On Windows this holds an `HWND` as the
/// `isize` that `GetForegroundWindow` returns. Other platforms have no
/// window-targeting backend yet (#119), so a handle is only ever captured there
/// once a backend exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowHandle(pub isize);

/// A window plus the identity it had when it was captured.
///
/// The bare handle is not enough to recognize a window later: Windows reuses
/// `HWND` values, so a destroyed window's handle can be handed to an unrelated
/// new window and still pass `IsWindow` (#254). Recording the owning process and
/// thread at capture time lets [`identity_is_alive`] tell "still the window the
/// user chose" from "a different window wearing the same handle". A window keeps
/// its owning thread for its whole life, so both fields are stable while the
/// window exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    pub handle: WindowHandle,
    pub process_id: u32,
    pub thread_id: u32,
}

/// Why a window could not be captured for locking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// No window held the foreground, or it disappeared while being read.
    NoForegroundWindow,
    /// The foreground window is one of AudioBud's own (#164). Locking onto
    /// AudioBud would send every later transcript into the settings window or
    /// the overlay instead of the user's app.
    OwnWindow,
    /// This platform has no window-targeting backend yet (#119).
    Unsupported,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::NoForegroundWindow => write!(f, "no foreground window to lock onto"),
            CaptureError::OwnWindow => write!(f, "the focused window belongs to AudioBud"),
            CaptureError::Unsupported => {
                write!(f, "window targeting is not supported on this platform")
            }
        }
    }
}

/// Whether a locked window is still the same window.
///
/// `probe` reports the process and thread that own `locked.handle` right now, or
/// `None` if no window has that handle any more (on Windows,
/// `GetWindowThreadProcessId`, which returns 0 for a dead handle). A handle the
/// OS has recycled reads as a different owner and so is NOT alive: the caller
/// then drops the lock and suppresses the paste rather than typing a transcript
/// into a window the user never chose (#254).
///
/// This is the one identity check for the whole subsystem; the one-shot picker
/// (#124) re-validates its chosen handle through it too.
pub fn identity_is_alive(
    locked: WindowIdentity,
    probe: impl FnOnce(WindowHandle) -> Option<(u32, u32)>,
) -> bool {
    match probe(locked.handle) {
        Some((process_id, thread_id)) => {
            process_id == locked.process_id && thread_id == locked.thread_id
        }
        None => false,
    }
}

/// Whether `candidate` is one of AudioBud's own windows (#164).
///
/// Both the settings window and the recording overlay belong to this process, so
/// the owning process id is the whole test -- no window title or class matching
/// is needed, and it holds for every window the app may add later.
pub fn is_own_window(candidate: WindowIdentity, own_process_id: u32) -> bool {
    candidate.process_id == own_process_id
}

/// Accept a freshly captured window as a lock target, or reject AudioBud's own
/// (#164). Shared by the target lock and the picker (#124) so neither can offer
/// the app's own windows.
pub fn accept_capture(
    candidate: WindowIdentity,
    own_process_id: u32,
) -> Result<WindowIdentity, CaptureError> {
    if is_own_window(candidate, own_process_id) {
        Err(CaptureError::OwnWindow)
    } else {
        Ok(candidate)
    }
}

/// Whether `class_name` is one of the Windows shell surfaces rather than an
/// application window.
///
/// The tray menu is the reason this exists: while the menu item's callback runs,
/// the shell's taskbar owns the foreground, so a plain foreground capture would
/// pin dictation to the taskbar. These are the shell's own top-level classes;
/// UWP application windows (`ApplicationFrameWindow`) are deliberately NOT here,
/// because those are real targets a user can dictate into.
pub fn is_shell_window(class_name: &str) -> bool {
    matches!(
        class_name,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "Progman"
            | "WorkerW"
            | "ForegroundStaging"
            | "MultitaskingViewFrame"
    )
}

/// What a backend reports about one window, so the eligibility rules below stay
/// testable without a window system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFacts<'a> {
    pub identity: WindowIdentity,
    /// The window class, used to spot shell surfaces.
    pub class_name: &'a str,
    /// Whether the window has a title at all. An untitled top-level window is
    /// almost always a helper window, not something a user dictates into.
    pub has_title: bool,
    pub visible: bool,
}

/// Whether a window may be locked onto or offered by the picker: visible,
/// titled, not AudioBud's own (#164), and not a shell surface.
pub fn is_eligible_target(facts: &WindowFacts, own_process_id: u32) -> bool {
    facts.visible
        && facts.has_title
        && !is_own_window(facts.identity, own_process_id)
        && !is_shell_window(facts.class_name)
}

/// Where a lock request came from, which decides how hard the backend looks for
/// a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSource {
    /// The global shortcut. The user pressed it while looking at the window they
    /// mean, so the foreground window is the answer or there is none: guessing
    /// at another window would pin dictation somewhere never asked for.
    Shortcut,
    /// The tray menu. Opening the menu takes the foreground away from the user's
    /// window, so the foreground cannot be trusted here and the backend falls
    /// back to the top window the user could have been working in.
    TrayMenu,
}

/// What one press of the lock toggle did, for the notice the caller shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockToggle {
    /// Delivery is now pinned to this window.
    Locked(WindowIdentity),
    /// The lock was released; delivery follows the foreground again.
    Unlocked,
    /// Nothing was captured, so the app stays unlocked.
    NotLocked(CaptureError),
}

/// Where the paste about to fire should be delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputTarget {
    /// Whatever window is focused when the paste fires (today's behavior).
    Foreground,
    /// A specific window captured at lock time.
    Pinned(WindowHandle),
}

/// Tauri-managed lock state. Registered alongside `EnigoState`; the paste path
/// reads it at send time to resolve the [`OutputTarget`]. `None` means no lock
/// is held and delivery follows the foreground.
#[derive(Default)]
pub struct PinnedTarget(pub Mutex<Option<WindowIdentity>>);

impl PinnedTarget {
    /// Lock delivery to `window` -- the one focused at the moment of locking --
    /// and return the target now in force. Locking again re-pins to the new
    /// window, so a second lock is also how you retarget.
    pub fn lock_to(&self, window: WindowIdentity) -> OutputTarget {
        *self.guard() = Some(window);
        OutputTarget::Pinned(window.handle)
    }

    /// Clear any lock and return to foreground delivery.
    pub fn unlock(&self) {
        *self.guard() = None;
    }

    /// Release the lock only if it still points at `expected`, and report
    /// whether it did.
    ///
    /// A delivery that discovers its target has died must not clear whatever
    /// lock is held by then: a user can unlock and re-lock to another window
    /// while a paste is still running, and blindly clearing would silently drop
    /// that new lock. Comparing first keeps a stale delivery from speaking for
    /// the current one.
    pub fn unlock_if(&self, expected: WindowIdentity) -> bool {
        let mut guard = self.guard();
        if *guard == Some(expected) {
            *guard = None;
            true
        } else {
            false
        }
    }

    /// Whether a window is currently locked.
    pub fn is_locked(&self) -> bool {
        self.guard().is_some()
    }

    /// The locked window, if any.
    pub fn locked(&self) -> Option<WindowIdentity> {
        *self.guard()
    }

    /// One press of the lock toggle: release an existing lock, or capture a new
    /// target with `capture` (on Windows, the foreground window).
    ///
    /// The lock is held across `capture` so two fast presses cannot interleave
    /// into a lock the user did not ask for. A failed capture leaves the app
    /// unlocked -- it never falls back to a stale target.
    pub fn toggle(
        &self,
        capture: impl FnOnce() -> Result<WindowIdentity, CaptureError>,
    ) -> LockToggle {
        let mut guard = self.guard();
        if guard.is_some() {
            *guard = None;
            return LockToggle::Unlocked;
        }
        match capture() {
            Ok(window) => {
                *guard = Some(window);
                LockToggle::Locked(window)
            }
            Err(error) => LockToggle::NotLocked(error),
        }
    }

    /// Borrow the lock, recovering the guard if a previous holder panicked.
    /// The mutex only guards a `Copy` `Option<WindowIdentity>` with no
    /// cross-field invariant, so a poisoned guard's value is always consistent.
    /// Recovering it keeps one panic in an `is_alive` callback from poisoning
    /// the mutex and bricking every later paste with a panic on `unwrap`
    /// (AGENTS.md: avoid unwrap in production).
    fn guard(&self) -> std::sync::MutexGuard<'_, Option<WindowIdentity>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A captured window: handle `h`, owned by process `pid` / thread `tid`.
    fn win(h: isize, pid: u32, tid: u32) -> WindowIdentity {
        WindowIdentity {
            handle: WindowHandle(h),
            process_id: pid,
            thread_id: tid,
        }
    }

    #[test]
    fn default_is_foreground_and_unlocked() {
        let t = PinnedTarget::default();
        assert!(!t.is_locked());
        // Nothing is locked, so a dictation starting now captures no window and
        // delivery follows the foreground.
        assert_eq!(t.locked(), None);
    }

    #[test]
    fn lock_pins_the_given_window() {
        let t = PinnedTarget::default();
        let w = win(42, 100, 200);
        assert_eq!(t.lock_to(w), OutputTarget::Pinned(w.handle));
        assert!(t.is_locked());
        // A dictation starting now captures the whole identity, not just the
        // handle, because that is what every later re-check needs (#254).
        assert_eq!(t.locked(), Some(w));
    }

    #[test]
    fn unlock_returns_to_foreground() {
        let t = PinnedTarget::default();
        t.lock_to(win(7, 1, 2));
        t.unlock();
        assert!(!t.is_locked());
        assert_eq!(t.locked(), None);
    }

    #[test]
    fn a_lock_toggled_mid_dictation_does_not_change_what_was_captured() {
        // The point of capturing at start (#160): a dictation already under way
        // holds its own target, so releasing or re-pointing the lock while the
        // user is still speaking governs the NEXT dictation, not this one.
        let t = PinnedTarget::default();
        let started_with = win(42, 100, 200);
        t.lock_to(started_with);
        let captured = t.locked();

        t.unlock();
        assert_eq!(captured, Some(started_with));

        let other = win(7, 500, 600);
        t.lock_to(other);
        assert_eq!(captured, Some(started_with));
        assert_eq!(t.locked(), Some(other));
    }

    #[test]
    fn poisoned_lock_recovers_instead_of_bricking() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let t = PinnedTarget::default();
        let w = win(5, 1, 2);
        // Capturing runs inside the guard, so a capture that unwinds poisons the
        // mutex. Recovery must let later calls proceed rather than panic on
        // every subsequent lock.
        let blew_up = catch_unwind(AssertUnwindSafe(|| {
            t.toggle(|| panic!("capture blew up"));
        }));
        assert!(blew_up.is_err());
        // The state is still readable rather than panicking on a poisoned lock,
        // and nothing was locked by the failed capture.
        assert!(!t.is_locked());
        assert_eq!(t.lock_to(w), OutputTarget::Pinned(w.handle));
        assert_eq!(t.locked(), Some(w));
    }

    #[test]
    fn re_locking_replaces_the_previous_window() {
        let t = PinnedTarget::default();
        t.lock_to(win(1, 1, 1));
        let second = win(2, 2, 2);
        assert_eq!(t.lock_to(second), OutputTarget::Pinned(second.handle));
        assert_eq!(t.locked(), Some(second));
    }

    #[test]
    fn a_window_that_kept_its_owner_is_alive() {
        let locked = win(42, 100, 200);
        assert!(identity_is_alive(locked, |h| {
            assert_eq!(h, locked.handle);
            Some((100, 200))
        }));
    }

    #[test]
    fn a_recycled_handle_is_not_alive() {
        // The handle exists again, but it belongs to another process now: the
        // OS handed the number to an unrelated window (#254). Bare IsWindow
        // would say "alive" here and leak the transcript into that window.
        let locked = win(42, 100, 200);
        assert!(!identity_is_alive(locked, |_| Some((999, 200))));
        // Same process, different thread: another window of the same app.
        assert!(!identity_is_alive(locked, |_| Some((100, 999))));
    }

    #[test]
    fn a_gone_handle_is_not_alive() {
        let locked = win(42, 100, 200);
        assert!(!identity_is_alive(locked, |_| None));
    }

    #[test]
    fn a_recycled_handle_releases_the_lock_it_impersonates() {
        // The end-to-end reason identity is carried on the dictation context: a
        // captured window whose handle the OS has recycled reads as gone, and
        // the delivery releases exactly that lock (backend::drop_lock_for) so
        // the transcript is never typed into the window wearing its handle.
        let t = PinnedTarget::default();
        let captured = win(42, 100, 200);
        t.lock_to(captured);
        assert!(!identity_is_alive(captured, |_| Some((999, 200))));
        assert!(t.unlock_if(captured));
        assert!(!t.is_locked());
    }

    #[test]
    fn capture_rejects_audiobuds_own_windows() {
        // Launching a second instance focuses the settings window (#164). A
        // capture must refuse it instead of locking onto AudioBud itself.
        let own = win(1, 4242, 7);
        assert!(is_own_window(own, 4242));
        assert_eq!(accept_capture(own, 4242), Err(CaptureError::OwnWindow));
    }

    #[test]
    fn capture_accepts_another_applications_window() {
        let other = win(1, 5, 7);
        assert!(!is_own_window(other, 4242));
        assert_eq!(accept_capture(other, 4242), Ok(other));
    }

    #[test]
    fn a_stale_delivery_cannot_clear_a_newer_lock() {
        // A paste to the first window is still running when the user re-locks
        // to another. The first window then dies: clearing the lock blindly
        // would drop the lock the user can see and is still using.
        let t = PinnedTarget::default();
        let first = win(1, 10, 20);
        let second = win(2, 30, 40);
        t.lock_to(first);
        t.lock_to(second);

        assert!(!t.unlock_if(first));
        assert_eq!(t.locked(), Some(second));

        // The current target still clears normally.
        assert!(t.unlock_if(second));
        assert!(!t.is_locked());
        // And clearing an already-empty lock reports that it did nothing.
        assert!(!t.unlock_if(second));
    }

    #[test]
    fn shell_surfaces_are_not_lock_targets() {
        // The taskbar owns the foreground while a tray menu click is handled,
        // so locking from the tray would otherwise pin dictation to it.
        assert!(is_shell_window("Shell_TrayWnd"));
        assert!(is_shell_window("Shell_SecondaryTrayWnd"));
        assert!(is_shell_window("Progman"));
        assert!(is_shell_window("WorkerW"));
        // Real application windows, including UWP frames, must stay eligible.
        assert!(!is_shell_window("ApplicationFrameWindow"));
        assert!(!is_shell_window("CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(!is_shell_window("Chrome_WidgetWin_1"));
    }

    fn facts(identity: WindowIdentity, class_name: &str) -> WindowFacts<'_> {
        WindowFacts {
            identity,
            class_name,
            has_title: true,
            visible: true,
        }
    }

    #[test]
    fn an_ordinary_window_is_an_eligible_target() {
        let other = win(1, 5, 7);
        assert!(is_eligible_target(
            &facts(other, "Chrome_WidgetWin_1"),
            4242
        ));
    }

    #[test]
    fn hidden_untitled_own_and_shell_windows_are_not_eligible() {
        let other = win(1, 5, 7);
        let own = win(2, 4242, 7);

        let hidden = WindowFacts {
            visible: false,
            ..facts(other, "Chrome_WidgetWin_1")
        };
        assert!(!is_eligible_target(&hidden, 4242));

        let untitled = WindowFacts {
            has_title: false,
            ..facts(other, "Chrome_WidgetWin_1")
        };
        assert!(!is_eligible_target(&untitled, 4242));

        assert!(!is_eligible_target(&facts(own, "Chrome_WidgetWin_1"), 4242));
        assert!(!is_eligible_target(&facts(other, "Shell_TrayWnd"), 4242));
    }

    #[test]
    fn toggle_locks_then_unlocks() {
        let t = PinnedTarget::default();
        let w = win(11, 1, 2);
        assert_eq!(t.toggle(|| Ok(w)), LockToggle::Locked(w));
        assert!(t.is_locked());
        // The second press releases; capture must not even run.
        assert_eq!(
            t.toggle(|| panic!("captured while already locked")),
            LockToggle::Unlocked
        );
        assert!(!t.is_locked());
    }

    #[test]
    fn a_failed_capture_leaves_the_app_unlocked() {
        let t = PinnedTarget::default();
        assert_eq!(
            t.toggle(|| Err(CaptureError::OwnWindow)),
            LockToggle::NotLocked(CaptureError::OwnWindow)
        );
        assert!(!t.is_locked());
        assert_eq!(t.locked(), None);
    }
}
