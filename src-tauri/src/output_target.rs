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

use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

/// Serializes experimental target creation with disable-time cleanup.
///
/// The settings cache changes before its runtime effects run. Without one
/// shared lock, an action can read "enabled", cleanup can finish, and then the
/// older action can create a lock or pick after cleanup. Holding this guard
/// across each targeting mutation makes cleanup the last writer after a disable.
static EXPERIMENTAL_TARGETING_GATE: Mutex<()> = Mutex::new(());

pub(crate) fn experimental_targeting_guard() -> MutexGuard<'static, ()> {
    EXPERIMENTAL_TARGETING_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
///
/// Process and thread alone are not unique, though: one GUI thread can destroy a
/// window and create another, and the new one can inherit the recycled handle,
/// matching on both counts. The window class is recorded as well, which tells
/// those apart whenever the replacement is a different kind of window -- a
/// dialog where a document window was, say.
///
/// RESIDUAL RISK: a replacement of the SAME class on the SAME thread reusing the
/// SAME handle still reads as alive. Closing that gap needs a positive signal
/// that the original window died (a `WinEvent` destruction hook), which means a
/// hook, a message loop to service it, and per-window bookkeeping -- a different
/// shape of change than this comparison, tracked as its own follow-up. Until
/// then the check is deliberately cheap and stateless, and every delivery is
/// still guarded by the foreground re-checks around each keystroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    pub handle: WindowHandle,
    pub process_id: u32,
    pub thread_id: u32,
    /// The window class at capture time, as [`class_fingerprint`] reduces it.
    pub class: ClassFingerprint,
}

/// A window class reduced to one comparable value.
///
/// The class is kept as a fingerprint rather than a `String` so an identity
/// stays `Copy`: it is held in Tauri-managed state, matched out of a mutex
/// guard, and passed by value through every layer of the paste path. Nothing
/// reads the class back -- it is only ever compared with another capture of the
/// same window -- so the name itself is not worth carrying.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClassFingerprint(pub u64);

/// Reduce a window class name to a [`ClassFingerprint`].
pub fn class_fingerprint(class_name: &str) -> ClassFingerprint {
    use std::hash::{Hash, Hasher};
    // DefaultHasher is seeded identically every time, so a fingerprint is
    // stable for as long as it needs to be: one capture compared with one
    // later probe, in the same run of the app.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    class_name.hash(&mut hasher);
    ClassFingerprint(hasher.finish())
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
/// `probe` reads the identity of whatever window holds `locked.handle` right
/// now, or `None` if no window holds it any more (on Windows,
/// `GetWindowThreadProcessId`, which returns 0 for a dead handle). Every
/// recorded field must match: a handle the OS has recycled reads as a different
/// owner or a different class of window, and so is NOT alive. The caller then
/// drops the lock and suppresses the paste rather than typing a transcript into
/// a window the user never chose (#254). See [`WindowIdentity`] for what this
/// catches and the one case it does not.
///
/// This is the one identity check for the whole subsystem; the one-shot picker
/// (#124) re-validates its chosen handle through it too.
pub fn identity_is_alive(
    locked: WindowIdentity,
    probe: impl FnOnce(WindowHandle) -> Option<WindowIdentity>,
) -> bool {
    // Comparing the whole identity keeps this honest as the struct grows: a new
    // field is compared the moment it is captured, rather than being forgotten
    // in a hand-written field list.
    probe(locked.handle) == Some(locked)
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

/// What the death of one delivery's target meant for the lock.
///
/// A delivery carries the target it was started for, so the window it is aimed
/// at and the window currently locked can differ: the user may have unlocked, or
/// re-locked elsewhere, while the transcript was still being produced (#160).
/// Both cases lose the transcript and both must be said out loud; only one of
/// them may touch the lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetLoss {
    /// The dead window was the one still locked, so the lock was released. The
    /// user is told their lock is gone. Carries the generation the release
    /// produced, for [`PinnedTarget::record_lost_notice`] (#266 review round
    /// 4): a caller that announces the loss re-checks this against the
    /// lock's generation immediately before persisting the notice, so a
    /// fresher lock or unlock established in between is never contradicted
    /// by a stale "lost" state arriving after it.
    LockCleared(u64),
    /// The dead window had already been superseded -- unlocked, or re-locked to
    /// another window -- so whatever the user set since stands untouched. They
    /// must still be told this transcript reached no window, but NOT that a lock
    /// was lost: the notice has to read true both when another window is locked
    /// and when they deliberately unlocked and nothing is.
    ObsoleteTarget,
}

/// A window's human-readable label: the app (process) name and the window
/// title, exactly as [`backend::window_label`] read them. Either half may be
/// `None`. Shared by every place that carries or caches one of these --
/// [`OutputTargetLockEvent`], [`LockedLabel`], [`PinnedTarget`]'s lost-lock
/// notice, and the tray's own derivation -- so the "app-or-title, both
/// optional" shape is spelled once instead of as a
/// `(Option<String>, Option<String>)` tuple type at each call site (#266
/// review).
pub type WindowLabel = (Option<String>, Option<String>);

/// A lock-state snapshot as reported to the indicator surfaces (#255): the
/// recording overlay, the tray, and settings. Mirrors the frontend's
/// `LockSnapshot` in `src/lib/output-target-indicator.ts`:
/// - `Unlocked`: delivery follows the foreground window.
/// - `Locked`: a window is pinned and was alive when this was built.
/// - `Lost`: a pinned window closed; [`TargetLoss::LockCleared`] just dropped
///   the lock. Originally event-only, on the theory that `PinnedTarget` being
///   already cleared by the time this fires means a poll afterwards reads
///   `Unlocked` -- but a webview that mounts (or a settings window that
///   opens) after the one-shot event has already fired would then silently
///   disagree with a tray or overlay that is still showing the loss (#266
///   review, finding 1). `backend::get_output_target_lock` now also consults
///   [`PinnedTarget::lost_notice`], the same persisted memory of the loss the
///   tray reads, so a snapshot query returns `Lost` for as long as that
///   notice stands. The frontend holds `Lost` as a latch until the user
///   dismisses it or a new lock/unlock replaces it.
///
/// `app`/`title` are the raw strings the platform label lookup read (an
/// app/process name and the window title). Either may be `None`. They are
/// sent untruncated -- the frontend core owns truncation and name precedence
/// so the source of the name and the source of the display cannot drift.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(tag = "kind")]
pub enum OutputTargetLockEvent {
    #[serde(rename = "unlocked")]
    Unlocked,
    #[serde(rename = "locked")]
    Locked {
        app: Option<String>,
        title: Option<String>,
    },
    #[serde(rename = "lost")]
    Lost {
        app: Option<String>,
        title: Option<String>,
    },
}

/// [`PinnedTarget`]'s guarded state: the lock itself, a generation counter,
/// and the lost-lock notice -- all three behind the one mutex so a mutation
/// and the decision of whether to persist a notice about it can happen
/// atomically. See [`PinnedTarget`]'s doc for why that atomicity is the
/// point (#266 review round 4).
#[derive(Default)]
struct LockState {
    window: Option<WindowIdentity>,
    /// Bumped on every mutation (lock, unlock, a resolve that finds the
    /// target gone). Lets a caller that read a value out from under the
    /// guard -- necessarily true of anything published after the guard is
    /// released, like an event emission -- ask "is what I have still the
    /// latest thing that happened" without re-deriving it.
    generation: u64,
    /// The last loss's label, kept only for as long as nothing has locked or
    /// unlocked since. See [`PinnedTarget::record_lost_notice`].
    lost_notice: Option<WindowLabel>,
}

/// Tauri-managed lock state. Registered alongside `EnigoState`; the paste path
/// reads it at send time to resolve the [`OutputTarget`]. `None` means no lock
/// is held and delivery follows the foreground.
///
/// Mutation and the publication that follows it (an `OutputTargetLockEvent`
/// emission, the tray's own rebuild) are necessarily two separate steps: the
/// mutation happens under this type's mutex, but Tauri event emission and
/// menu rebuilding cannot happen while holding it (they touch other state,
/// including the app handle itself, and doing IPC-adjacent work inside a
/// std::sync::Mutex critical section is its own hazard). Two overlapping
/// operations -- two toggle presses, or a delivery discovering a loss racing
/// a fresh lock from the tray -- can interleave across that gap: a slower
/// mutation's publication can land after a faster, later mutation's, showing
/// a state the backend has already moved past (#266 review round 4).
///
/// The generation counter in [`LockState`] is the fix, chosen over threading
/// a distinct "publish this" flag through every layer: every mutating method
/// hands back the generation it produced, and a caller re-reads
/// [`generation`](Self::generation) immediately before publishing, skipping
/// the publish if something newer has already happened. Because the
/// generation only ever increases, the newest mutation is always the one
/// whose re-read still matches what it produced, so it is the one and only
/// operation that gets to publish -- regardless of how the two operations'
/// wall-clock timing actually interleaves. [`record_lost_notice`] goes one
/// step further for the loss notice specifically: the compare-and-set
/// happens under the very same guard as the check, in one critical section,
/// closing the gap completely rather than narrowing it to "one read, then a
/// re-read".
#[derive(Default)]
pub struct PinnedTarget(Mutex<LockState>);

impl PinnedTarget {
    /// Lock delivery to `window` -- the one focused at the moment of locking --
    /// and return the target now in force plus the generation this produced.
    /// Locking again re-pins to the new window, so a second lock is also how
    /// you retarget. Clears any lost-lock notice: a fresh lock supersedes
    /// whatever the tray remembered about an earlier one going stale.
    pub fn lock_to(&self, window: WindowIdentity) -> (OutputTarget, u64) {
        let mut guard = self.guard();
        guard.window = Some(window);
        guard.generation += 1;
        guard.lost_notice = None;
        (OutputTarget::Pinned(window.handle), guard.generation)
    }

    /// Clear any lock and return to foreground delivery, and the generation
    /// this produced. Also clears any lost-lock notice.
    pub fn unlock(&self) -> u64 {
        let mut guard = self.guard();
        guard.window = None;
        guard.generation += 1;
        guard.lost_notice = None;
        guard.generation
    }

    /// Release the lock only if it still points at `expected`, and report the
    /// generation this produced if it did.
    ///
    /// A delivery that discovers its target has died must not clear whatever
    /// lock is held by then: a user can unlock and re-lock to another window
    /// while a paste is still running, and blindly clearing would silently drop
    /// that new lock. Comparing first keeps a stale delivery from speaking for
    /// the current one.
    pub fn unlock_if(&self, expected: WindowIdentity) -> Option<u64> {
        let mut guard = self.guard();
        if guard.window == Some(expected) {
            guard.window = None;
            guard.generation += 1;
            Some(guard.generation)
        } else {
            None
        }
    }

    /// Retire the dead target of one delivery, and report what that meant for
    /// the lock the user can see.
    ///
    /// Wraps [`Self::unlock_if`] so a caller cannot read "the lock did not
    /// change" as "there is nothing to tell the user". Both outcomes are a
    /// transcript that reached no window: the difference is only whether the
    /// lock the user is looking at went with it (#160).
    pub fn retire_dead_target(&self, target: WindowIdentity) -> TargetLoss {
        match self.unlock_if(target) {
            Some(generation) => TargetLoss::LockCleared(generation),
            None => TargetLoss::ObsoleteTarget,
        }
    }

    /// Whether a window is currently locked.
    pub fn is_locked(&self) -> bool {
        self.guard().window.is_some()
    }

    /// The locked window, if any.
    pub fn locked(&self) -> Option<WindowIdentity> {
        self.guard().window
    }

    /// The current generation, for a caller about to compare a value it
    /// produced earlier against the latest state (#266 review round 4).
    pub fn generation(&self) -> u64 {
        self.guard().generation
    }

    /// One press of the lock toggle: release an existing lock, or capture a new
    /// target with `capture` (on Windows, the foreground window). Returns the
    /// generation the transition produced alongside it.
    ///
    /// The lock is held across `capture` so two fast presses cannot interleave
    /// into a lock the user did not ask for. A failed capture leaves the app
    /// unlocked -- it never falls back to a stale target, and does not bump
    /// the generation, since nothing changed.
    pub fn toggle(
        &self,
        capture: impl FnOnce() -> Result<WindowIdentity, CaptureError>,
    ) -> (LockToggle, u64) {
        let mut guard = self.guard();
        if guard.window.is_some() {
            guard.window = None;
            guard.generation += 1;
            guard.lost_notice = None;
            return (LockToggle::Unlocked, guard.generation);
        }
        match capture() {
            Ok(window) => {
                guard.window = Some(window);
                guard.generation += 1;
                guard.lost_notice = None;
                (LockToggle::Locked(window), guard.generation)
            }
            Err(error) => (LockToggle::NotLocked(error), guard.generation),
        }
    }

    /// Atomically record `label` as the lost-lock notice, but only if the
    /// lock's generation is still `expected_generation`, and report whether it
    /// was recorded (#266 review round 4).
    ///
    /// `expected_generation` comes from [`retire_dead_target`](Self::retire_dead_target)'s
    /// `TargetLoss::LockCleared`, the only mutation that actually loses a
    /// lock. The check and the write happen under one guard, which is what
    /// actually closes the race a plain "check `is_locked`, then separately
    /// write the notice" cannot: a concurrent `toggle`/`unlock`/`retire_dead_target`
    /// either fully precedes this call (bumps the generation first, so the
    /// compare fails and nothing is written) or fully follows it (that
    /// mutation clears `lost_notice` itself, in the same guard it bumps the
    /// generation in) -- there is no gap in which a lock lands and a stale
    /// notice for the window it replaced still gets written over it.
    pub fn record_lost_notice(&self, expected_generation: u64, label: WindowLabel) -> bool {
        let mut guard = self.guard();
        if guard.generation != expected_generation {
            return false;
        }
        guard.lost_notice = Some(label);
        true
    }

    /// Clear the lost-lock notice -- a dismissal, or a wrap-up after telling
    /// the user about it -- and report whether one was actually cleared.
    pub fn dismiss_lost_notice(&self) -> bool {
        let mut guard = self.guard();
        let had_one = guard.lost_notice.is_some();
        guard.lost_notice = None;
        had_one
    }

    /// The lost-lock notice, if the lock has not moved on from it.
    pub fn lost_notice(&self) -> Option<WindowLabel> {
        self.guard().lost_notice.clone()
    }

    /// Borrow the lock, recovering the guard if a previous holder panicked.
    /// `LockState` is a plain value with no invariant that a panic mid-mutation
    /// could leave broken halfway (every field is written independently and
    /// each is meaningful on its own), so a poisoned guard's value is always
    /// safe to keep using. Recovering it keeps one panic in an `is_alive`
    /// callback from poisoning the mutex and bricking every later paste with a
    /// panic on `unwrap` (AGENTS.md: avoid unwrap in production).
    fn guard(&self) -> std::sync::MutexGuard<'_, LockState> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Caches a locked window's label from the moment it was captured, keyed to
/// its identity.
///
/// By the time a loss is detected, the window (often its whole owning
/// process) is already gone, so re-querying the label live -- as the
/// original #255 wiring did -- routinely comes back `(None, None)` and the
/// indicator can only say "a locked window" (#266 review). Reading the label
/// while the window was still alive, at lock time, and caching it here for
/// [`TargetLoss::LockCleared`] to reuse gives the loss notice back its name.
///
/// Keyed to the identity so a lock that was replaced before it was ever lost
/// cannot leave a stale label behind for a *different* window that later
/// goes stale.
#[derive(Default)]
pub struct LockedLabel(Mutex<Option<(WindowIdentity, WindowLabel)>>);

impl LockedLabel {
    /// Cache `label` for `identity`, replacing whatever was cached before.
    pub fn set(&self, identity: WindowIdentity, label: WindowLabel) {
        *self.guard() = Some((identity, label));
    }

    /// The cached label for `identity`, or `None` if nothing is cached for it
    /// -- including when the cache holds a different, superseded window.
    pub fn get(&self, identity: WindowIdentity) -> Option<WindowLabel> {
        self.guard()
            .as_ref()
            .filter(|(cached, _)| *cached == identity)
            .map(|(_, label)| label.clone())
    }

    /// Drop whatever is cached.
    pub fn clear(&self) {
        *self.guard() = None;
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, Option<(WindowIdentity, WindowLabel)>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// The lost-lock notice used to be its own Tauri-managed type (`LostLockNotice`),
// written from a `record_lost_notice` free function that checked
// `PinnedTarget::is_locked` and then wrote the notice as two separate mutex
// acquisitions. That gap was exactly the race #266's review round 4 found: a
// lock landing between the check and the write got a stale notice for the
// window it replaced restored over it. The notice now lives inside
// `PinnedTarget`'s own `LockState`, behind the same guard as the lock and the
// generation counter, so `PinnedTarget::record_lost_notice` can check and
// write in one atomic step.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experimental_targeting_actions_finish_before_disable_cleanup_enters() {
        use std::sync::mpsc;
        use std::time::Duration;

        let action_guard = experimental_targeting_guard();
        let (attempted_tx, attempted_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let cleanup = std::thread::spawn(move || {
            attempted_tx.send(()).unwrap();
            let _cleanup_guard = experimental_targeting_guard();
            entered_tx.send(()).unwrap();
        });

        attempted_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            entered_rx.try_recv().is_err(),
            "disable cleanup entered while a targeting action held the gate"
        );

        drop(action_guard);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        cleanup.join().unwrap();
    }

    /// A captured window: handle `h`, owned by process `pid` / thread `tid`.
    fn win(h: isize, pid: u32, tid: u32) -> WindowIdentity {
        classed_win(h, pid, tid, "Chrome_WidgetWin_1")
    }

    /// The same, with the window class spelled out.
    fn classed_win(h: isize, pid: u32, tid: u32, class_name: &str) -> WindowIdentity {
        WindowIdentity {
            handle: WindowHandle(h),
            process_id: pid,
            thread_id: tid,
            class: class_fingerprint(class_name),
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
        let (target, generation) = t.lock_to(w);
        assert_eq!(target, OutputTarget::Pinned(w.handle));
        assert_eq!(generation, t.generation());
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
        assert_eq!(t.lock_to(w).0, OutputTarget::Pinned(w.handle));
        assert_eq!(t.locked(), Some(w));
    }

    #[test]
    fn re_locking_replaces_the_previous_window() {
        let t = PinnedTarget::default();
        t.lock_to(win(1, 1, 1));
        let second = win(2, 2, 2);
        assert_eq!(t.lock_to(second).0, OutputTarget::Pinned(second.handle));
        assert_eq!(t.locked(), Some(second));
    }

    #[test]
    fn generation_advances_on_every_mutation_but_not_on_a_failed_capture() {
        let t = PinnedTarget::default();
        let start = t.generation();

        let (_, after_lock) = t.lock_to(win(1, 1, 1));
        assert!(after_lock > start);

        let after_unlock = t.unlock();
        assert!(after_unlock > after_lock);

        // A failed capture changes nothing, so the generation does not move.
        let (toggle, after_failed_toggle) = t.toggle(|| Err(CaptureError::OwnWindow));
        assert_eq!(toggle, LockToggle::NotLocked(CaptureError::OwnWindow));
        assert_eq!(after_failed_toggle, after_unlock);

        let (toggle, after_toggle_lock) = t.toggle(|| Ok(win(2, 2, 2)));
        assert!(matches!(toggle, LockToggle::Locked(_)));
        assert!(after_toggle_lock > after_unlock);

        // Retiring the now-dead target as the current lock bumps the
        // generation again.
        match t.retire_dead_target(win(2, 2, 2)) {
            TargetLoss::LockCleared(after_retire) => assert!(after_retire > after_toggle_lock),
            TargetLoss::ObsoleteTarget => panic!("expected the current lock to clear"),
        }
    }

    #[test]
    fn a_window_that_kept_its_owner_is_alive() {
        let locked = win(42, 100, 200);
        assert!(identity_is_alive(locked, |h| {
            assert_eq!(h, locked.handle);
            Some(locked)
        }));
    }

    #[test]
    fn a_recycled_handle_is_not_alive() {
        // The handle exists again, but it belongs to another process now: the
        // OS handed the number to an unrelated window (#254). Bare IsWindow
        // would say "alive" here and leak the transcript into that window.
        let locked = win(42, 100, 200);
        assert!(!identity_is_alive(locked, |_| Some(win(42, 999, 200))));
        // Same process, different thread: another window of the same app.
        assert!(!identity_is_alive(locked, |_| Some(win(42, 100, 999))));
    }

    #[test]
    fn a_same_thread_replacement_of_another_class_is_not_alive() {
        // One GUI thread can destroy a window and create another that inherits
        // the handle, so process and thread both still match. A different class
        // -- a dialog where a document window was -- gives it away (#254).
        let locked = classed_win(42, 100, 200, "CabinetWClass");
        let replacement = classed_win(42, 100, 200, "#32770");
        assert!(!identity_is_alive(locked, |_| Some(replacement)));
        // The window itself, unchanged, still reads as alive.
        assert!(identity_is_alive(locked, |_| Some(locked)));
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
        assert!(!identity_is_alive(captured, |_| Some(win(42, 999, 200))));
        assert!(matches!(
            t.retire_dead_target(captured),
            TargetLoss::LockCleared(_)
        ));
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
    fn an_obsolete_target_is_announced_without_clearing_the_newer_lock() {
        // The silent-loss case (#160): a dictation started against one window
        // finishes after the user re-locked elsewhere, and its own window has
        // since closed. The transcript reached no window, so the user must hear
        // about it -- but the lock they can see is still good and must survive.
        let t = PinnedTarget::default();
        let started_with = win(1, 10, 20);
        let locked_now = win(2, 30, 40);
        t.lock_to(started_with);
        t.lock_to(locked_now);

        assert_eq!(
            t.retire_dead_target(started_with),
            TargetLoss::ObsoleteTarget
        );
        assert_eq!(t.locked(), Some(locked_now));

        // Same when the user simply unlocked mid-dictation: still a lost
        // transcript to report, still nothing to clear.
        t.unlock();
        assert_eq!(
            t.retire_dead_target(started_with),
            TargetLoss::ObsoleteTarget
        );
        assert!(!t.is_locked());
    }

    #[test]
    fn a_dead_target_that_is_still_the_lock_clears_it() {
        // The ordinary case: the window that died is the one locked, so the
        // lock goes with it and the user is told their lock is gone.
        let t = PinnedTarget::default();
        let w = win(9, 1, 2);
        t.lock_to(w);
        assert!(matches!(
            t.retire_dead_target(w),
            TargetLoss::LockCleared(_)
        ));
        assert!(!t.is_locked());
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

        assert_eq!(t.unlock_if(first), None);
        assert_eq!(t.locked(), Some(second));

        // The current target still clears normally.
        assert!(t.unlock_if(second).is_some());
        assert!(!t.is_locked());
        // And clearing an already-empty lock reports that it did nothing.
        assert_eq!(t.unlock_if(second), None);
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
        assert_eq!(t.toggle(|| Ok(w)).0, LockToggle::Locked(w));
        assert!(t.is_locked());
        // The second press releases; capture must not even run.
        assert_eq!(
            t.toggle(|| panic!("captured while already locked")).0,
            LockToggle::Unlocked
        );
        assert!(!t.is_locked());
    }

    #[test]
    fn a_failed_capture_leaves_the_app_unlocked() {
        let t = PinnedTarget::default();
        assert_eq!(
            t.toggle(|| Err(CaptureError::OwnWindow)).0,
            LockToggle::NotLocked(CaptureError::OwnWindow)
        );
        assert!(!t.is_locked());
        assert_eq!(t.locked(), None);
    }

    #[test]
    fn locked_label_cache_starts_empty() {
        let c = LockedLabel::default();
        assert_eq!(c.get(win(1, 1, 1)), None);
    }

    #[test]
    fn locked_label_cache_returns_the_label_for_the_matching_identity() {
        let c = LockedLabel::default();
        let w = win(1, 10, 20);
        let label = (Some("Terminal".to_string()), Some("zsh".to_string()));
        c.set(w, label.clone());
        assert_eq!(c.get(w), Some(label));
    }

    #[test]
    fn locked_label_cache_does_not_answer_for_a_different_window() {
        // A lock that was replaced before it was ever lost must not leave its
        // label behind for the new window (#266 review).
        let c = LockedLabel::default();
        let first = win(1, 10, 20);
        let second = win(2, 30, 40);
        c.set(first, (Some("First".to_string()), None));
        c.set(second, (Some("Second".to_string()), None));
        assert_eq!(c.get(first), None);
        assert_eq!(c.get(second), Some((Some("Second".to_string()), None)));
    }

    #[test]
    fn locked_label_cache_clear_forgets_it() {
        let c = LockedLabel::default();
        let w = win(1, 10, 20);
        c.set(w, (Some("Terminal".to_string()), None));
        c.clear();
        assert_eq!(c.get(w), None);
    }

    #[test]
    fn lost_notice_starts_empty() {
        let t = PinnedTarget::default();
        assert_eq!(t.lost_notice(), None);
        // Nothing to dismiss yet.
        assert!(!t.dismiss_lost_notice());
    }

    /// Retire `target` as the current lock and return the generation that
    /// produced, panicking if it turned out to already be obsolete -- a
    /// helper so the `record_lost_notice` tests below can focus on what
    /// happens around the generation rather than re-deriving it each time.
    fn retire_and_expect_cleared(t: &PinnedTarget, target: WindowIdentity) -> u64 {
        match t.retire_dead_target(target) {
            TargetLoss::LockCleared(generation) => generation,
            TargetLoss::ObsoleteTarget => panic!("expected the lock to clear"),
        }
    }

    #[test]
    fn record_lost_notice_records_when_the_generation_still_matches() {
        let t = PinnedTarget::default();
        let generation = t.generation();
        let label = (Some("Terminal".to_string()), None);

        assert!(t.record_lost_notice(generation, label.clone()));
        assert_eq!(t.lost_notice(), Some(label));
    }

    #[test]
    fn record_lost_notice_is_suppressed_by_a_lock_established_in_the_meantime() {
        // The race #266's review round 4 closes: a delivery discovers its
        // target is gone (`retire_dead_target` already cleared PinnedTarget
        // and handed back the generation that transition produced), but a
        // lock on a new window lands on another thread before the loss can
        // be recorded. Persisting "lost" now would arrive after that
        // window's own "locked" state and contradict it -- so the stale
        // generation must make the write a no-op.
        let t = PinnedTarget::default();
        let target = win(1, 10, 20);
        t.lock_to(target);
        let lost_generation = retire_and_expect_cleared(&t, target);

        // A new lock lands before the loss is recorded.
        t.lock_to(win(2, 30, 40));

        assert!(!t.record_lost_notice(lost_generation, (Some("Old window".to_string()), None)));
        assert_eq!(t.lost_notice(), None);
        // The new lock is untouched by the suppressed write.
        assert!(t.is_locked());
        assert_eq!(t.locked(), Some(win(2, 30, 40)));
    }

    #[test]
    fn record_lost_notice_overwrites_an_earlier_unrecorded_loss() {
        let t = PinnedTarget::default();
        let target = win(1, 1, 1);
        t.lock_to(target);
        let generation = retire_and_expect_cleared(&t, target);
        t.record_lost_notice(generation, (Some("Stale from before".to_string()), None));

        // A later loss at the same (still-unlocked) generation replaces it.
        let label = (Some("Terminal".to_string()), None);
        assert!(t.record_lost_notice(generation, label.clone()));
        assert_eq!(t.lost_notice(), Some(label));
    }

    #[test]
    fn a_fresh_lock_clears_any_pending_lost_notice() {
        // Whichever operation runs last atomically clears `lost_notice`
        // alongside its own mutation, so a compare-and-set for an earlier
        // loss can never win the race against it (#266 review round 4).
        let t = PinnedTarget::default();
        let target = win(1, 1, 1);
        t.lock_to(target);
        let lost_generation = retire_and_expect_cleared(&t, target);
        assert!(t.record_lost_notice(lost_generation, (Some("Gone".to_string()), None)));
        assert_eq!(t.lost_notice(), Some((Some("Gone".to_string()), None)));

        // Locking again -- the fix for the race, not just a coincidence --
        // clears the notice in the same critical section as the mutation.
        t.lock_to(win(2, 2, 2));
        assert_eq!(t.lost_notice(), None);
    }

    #[test]
    fn dismiss_lost_notice_clears_it_and_reports_whether_one_existed() {
        let t = PinnedTarget::default();
        assert!(!t.dismiss_lost_notice());

        let target = win(1, 1, 1);
        t.lock_to(target);
        let generation = retire_and_expect_cleared(&t, target);
        t.record_lost_notice(generation, (Some("Terminal".to_string()), None));

        assert!(t.dismiss_lost_notice());
        assert_eq!(t.lost_notice(), None);
        assert!(!t.dismiss_lost_notice());
    }

    #[test]
    fn toggle_locked_transition_clears_any_pending_lost_notice() {
        let t = PinnedTarget::default();
        let target = win(1, 1, 1);
        t.lock_to(target);
        let generation = retire_and_expect_cleared(&t, target);
        t.record_lost_notice(generation, (Some("Gone".to_string()), None));
        assert!(t.lost_notice().is_some());

        t.toggle(|| Ok(win(2, 2, 2)));
        assert_eq!(t.lost_notice(), None);
    }
}
