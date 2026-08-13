//! Where a finished transcript is delivered: the output target.
//!
//! The default is [`OutputTarget::Foreground`] -- the paste lands in whatever
//! window is focused when it fires, which is AudioBud's behavior today. A user
//! can "lock" the currently focused window; while locked, every transcript goes
//! to that window regardless of what is focused at send time (issue #120). The
//! locked handle is held in Tauri-managed state ([`PinnedTarget`]) alongside
//! `EnigoState`, and the paste path reads it at send time.
//!
//! This module is the platform-independent core: the lock/unlock state machine
//! and the closed-window failsafe. Realizing a pinned target is a focus-borrow
//! in the paste path (save foreground, activate the pinned window, paste,
//! restore) and is Windows-only for now; that half is the next child of the
//! epic (#119) and is where [`PinnedTarget::resolve`] gets its first caller.
//!
//! Until that caller lands, the API is exercised only by the unit tests below,
//! so the module allows dead_code; drop the allow when the paste path wires in.
#![allow(dead_code)]

use std::sync::Mutex;

/// A captured native window handle. On Windows this holds an `HWND` as the
/// `isize` that `GetForegroundWindow` returns. Other platforms have no
/// window-targeting backend yet (#119), so a handle is only ever captured there
/// once a backend exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowHandle(pub isize);

/// Where the paste about to fire should be delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputTarget {
    /// Whatever window is focused when the paste fires (today's behavior).
    Foreground,
    /// A specific window captured at lock time.
    Pinned(WindowHandle),
}

/// The outcome of resolving the target for one paste: either a concrete
/// delivery target, or the signal that a stale lock was just dropped. This is a
/// distinct type from [`OutputTarget`] so the "lock lost" transition, which the
/// caller must surface once, cannot be dropped like a stray bool, and so an
/// illegal pairing (a pinned target that also lost its lock) is unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// Deliver to this target: `Foreground` normally, `Pinned` when a live
    /// window is locked.
    Deliver(OutputTarget),
    /// The locked window had closed, so the lock was dropped. Do NOT fall back
    /// to the foreground: pasting the transcript into whatever app now holds
    /// focus is the exact leak target-locking exists to prevent (#120). The
    /// caller must SUPPRESS this paste and surface the "lock lost" notice once,
    /// then let the user re-lock or re-dictate. The lock is already cleared
    /// here, so the next resolve is a plain `Deliver(Foreground)`.
    LockLost,
}

/// Tauri-managed lock state. Registered alongside `EnigoState`; the paste path
/// reads it at send time to resolve the [`OutputTarget`]. `None` means no lock
/// is held and delivery follows the foreground.
#[derive(Default)]
pub struct PinnedTarget(pub Mutex<Option<WindowHandle>>);

impl PinnedTarget {
    /// Lock delivery to `window` -- the one focused at the moment of locking --
    /// and return the target now in force. Locking again re-pins to the new
    /// window, so a second lock is also how you retarget.
    pub fn lock_to(&self, window: WindowHandle) -> OutputTarget {
        *self.guard() = Some(window);
        OutputTarget::Pinned(window)
    }

    /// Clear any lock and return to foreground delivery.
    pub fn unlock(&self) {
        *self.guard() = None;
    }

    /// Whether a window is currently locked.
    pub fn is_locked(&self) -> bool {
        self.guard().is_some()
    }

    /// Resolve the target for the paste about to fire.
    ///
    /// `is_alive` reports whether the locked window still exists (on Windows,
    /// `IsWindow(hwnd)`). It is not consulted when nothing is locked. If the
    /// locked window has gone, this FAILS SAFE: it drops the lock and returns
    /// [`Resolved::LockLost`] rather than borrowing focus to a handle the OS may
    /// have recycled -- a transcript landing in the wrong app is the exact
    /// failure this feature exists to prevent (#120).
    ///
    /// Liveness here is advisory, not a guarantee: the window can still close
    /// between this call and the focus-borrow that acts on a `Pinned` result, so
    /// that paste path must itself tolerate an activation that fails rather than
    /// assume the handle is good.
    pub fn resolve(&self, is_alive: impl FnOnce(WindowHandle) -> bool) -> Resolved {
        let mut guard = self.guard();
        match *guard {
            None => Resolved::Deliver(OutputTarget::Foreground),
            Some(window) => {
                if is_alive(window) {
                    Resolved::Deliver(OutputTarget::Pinned(window))
                } else {
                    *guard = None;
                    Resolved::LockLost
                }
            }
        }
    }

    /// Borrow the lock, recovering the guard if a previous holder panicked.
    /// The mutex only guards a `Copy` `Option<WindowHandle>` with no
    /// cross-field invariant, so a poisoned guard's value is always consistent.
    /// Recovering it keeps one panic in an `is_alive` callback from poisoning
    /// the mutex and bricking every later paste with a panic on `unwrap`
    /// (AGENTS.md: avoid unwrap in production).
    fn guard(&self) -> std::sync::MutexGuard<'_, Option<WindowHandle>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_foreground_and_unlocked() {
        let t = PinnedTarget::default();
        assert!(!t.is_locked());
        // is_alive must not even be consulted when nothing is locked.
        let resolved = t.resolve(|_| panic!("is_alive called with no lock"));
        assert_eq!(resolved, Resolved::Deliver(OutputTarget::Foreground));
    }

    #[test]
    fn lock_pins_the_given_window() {
        let t = PinnedTarget::default();
        let w = WindowHandle(42);
        assert_eq!(t.lock_to(w), OutputTarget::Pinned(w));
        assert!(t.is_locked());
        let resolved = t.resolve(|h| {
            assert_eq!(h, w);
            true
        });
        assert_eq!(resolved, Resolved::Deliver(OutputTarget::Pinned(w)));
        // Resolving a live target must NOT clear the lock.
        assert!(t.is_locked());
    }

    #[test]
    fn unlock_returns_to_foreground() {
        let t = PinnedTarget::default();
        t.lock_to(WindowHandle(7));
        t.unlock();
        assert!(!t.is_locked());
        let resolved = t.resolve(|_| true);
        assert_eq!(resolved, Resolved::Deliver(OutputTarget::Foreground));
    }

    #[test]
    fn dead_target_fails_safe_and_drops_the_lock() {
        let t = PinnedTarget::default();
        t.lock_to(WindowHandle(99));
        // Locked window has closed: resolve must fail safe and report LockLost
        // so the caller can surface the notice once.
        assert_eq!(t.resolve(|_| false), Resolved::LockLost);
        // The lock is gone, so the next paste is a plain foreground paste and
        // is_alive is never consulted again.
        assert!(!t.is_locked());
        let again = t.resolve(|_| panic!("stale lock still consulted"));
        assert_eq!(again, Resolved::Deliver(OutputTarget::Foreground));
    }

    #[test]
    fn poisoned_lock_recovers_instead_of_bricking() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let t = PinnedTarget::default();
        t.lock_to(WindowHandle(5));
        // An is_alive callback that unwinds while resolve holds the guard
        // poisons the mutex. Recovery must let later calls proceed rather than
        // panic on every subsequent lock.
        let blew_up = catch_unwind(AssertUnwindSafe(|| {
            t.resolve(|_| panic!("is_alive blew up"));
        }));
        assert!(blew_up.is_err());
        // The lock is still readable and the pinned window survived the panic;
        // a normal resolve now succeeds instead of panicking on a poisoned lock.
        assert!(t.is_locked());
        assert_eq!(
            t.resolve(|_| true),
            Resolved::Deliver(OutputTarget::Pinned(WindowHandle(5)))
        );
    }

    #[test]
    fn re_locking_replaces_the_previous_window() {
        let t = PinnedTarget::default();
        t.lock_to(WindowHandle(1));
        assert_eq!(
            t.lock_to(WindowHandle(2)),
            OutputTarget::Pinned(WindowHandle(2))
        );
        let resolved = t.resolve(|_| true);
        assert_eq!(
            resolved,
            Resolved::Deliver(OutputTarget::Pinned(WindowHandle(2)))
        );
    }
}
