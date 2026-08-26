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
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_specta::Event;

use super::{
    CaptureError, CaptureSource, LockToggle, LockedLabel, OutputTargetLockEvent, PinnedTarget,
    TargetLoss, WindowIdentity, WindowLabel,
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

/// Emitted when a delivery reached no window because the window that dictation
/// was started for had closed, while the lock the user can see has since moved
/// on and still stands (#160).
///
/// Distinct from [`TARGET_LOCK_LOST_EVENT`] because the two say different things
/// to the user: this one must NOT claim their current lock is gone. Without it a
/// suppressed delivery in this case is silent, and with the default
/// `ClipboardHandling::DontModify` the transcript is gone with it.
pub const TARGET_WINDOW_GONE_EVENT: &str = "target-window-gone";

/// Emitted after a transcript was actually typed into a pinned window (issue
/// #165): positive confirmation naming the window it reached.
///
/// Only a pinned delivery -- a target lock (#120) or a one-shot pick (#124) --
/// gets this. A plain foreground paste lands wherever the user is already
/// looking, so a misfire there is a visible typo the moment it happens; under
/// a lock or a pick they are deliberately looking somewhere else, and without
/// this a silent success is indistinguishable from a silent misdelivery.
///
/// `app`/`title` are the raw strings [`window_label`] read, exactly like
/// [`OutputTargetLockEvent`]'s -- untruncated, with the frontend owning name
/// precedence and truncation. `source` says whether the destination was a
/// standing target lock or a one-shot pick, so the frontend's fallback copy
/// (when both label lookups come back empty) can describe the right kind of
/// destination instead of always assuming a lock (#279 review).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct TranscriptDeliveredEvent {
    pub app: Option<String>,
    pub title: Option<String>,
    pub source: DeliverySource,
}

/// Who chose the window one paste is aimed at. The two are delivered exactly
/// alike; they differ only in what a failure means, so the cleanup for a lost
/// window has to know which it is holding.
///
/// Serialized lowercase (`"lock"` / `"pick"`) for [`TranscriptDeliveredEvent`],
/// matching [`OutputTargetLockEvent`]'s `kind` tag convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum DeliverySource {
    /// The target lock (#120): the window this dictation was started for, read
    /// from the lock at recording start. Losing it clears the lock and says so.
    Lock,
    /// A one-shot pick (#124): a window chosen for this transcript only. Losing
    /// it must NOT touch the lock -- the user may hold an unrelated one -- and
    /// says the pick is gone, not the lock.
    Pick,
}

/// Where one paste is delivered. Carries the whole [`WindowIdentity`], not just
/// the handle, because every step of the delivery re-checks it (#254).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Whatever window holds focus when the paste fires.
    Foreground,
    /// A specific window, and who aimed the paste at it.
    Pinned(WindowIdentity, DeliverySource),
}

impl DeliverySource {
    /// Whether losing this delivery's window means the target lock is gone.
    ///
    /// Only the lock's own delivery does. A one-shot pick holds no lock, so
    /// clearing one on its behalf would take down a lock the user set
    /// separately and is still relying on, and the "locked window is gone"
    /// notice would name something that never happened (#124).
    pub fn clears_the_lock(self) -> bool {
        matches!(self, DeliverySource::Lock)
    }
}

impl Delivery {
    /// The window this delivery is aimed at, if it is not the plain foreground.
    pub fn target(self) -> Option<WindowIdentity> {
        match self {
            Delivery::Foreground => None,
            Delivery::Pinned(window, _) => Some(window),
        }
    }

    /// Who aimed it. The foreground path has no window to lose, so its answer is
    /// never consulted; it reads as the lock's for want of anything to clean up.
    pub fn source(self) -> DeliverySource {
        match self {
            Delivery::Foreground => DeliverySource::Lock,
            Delivery::Pinned(_, source) => source,
        }
    }
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
///
/// The emission is skipped if the lock has already moved past the generation
/// this toggle produced (#266 review round 4): two overlapping toggles --
/// two presses of the shortcut, or the shortcut racing the tray item -- can
/// each finish mutating before either emits, and without this check the
/// slower one's emission could land after the faster one's and show a state
/// the backend has already moved past (a stale "Locked" arriving after a
/// newer "Unlocked", or vice versa). Because the generation only ever
/// increases, only the toggle that produced the CURRENT generation passes
/// this check, so exactly one of any two overlapping toggles gets to
/// publish -- whichever one actually happened last.
pub fn toggle_target_lock(app: &AppHandle, source: CaptureSource) {
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized");
        return;
    };

    let (toggle, generation) = pinned.toggle(|| capture_foreground_window(source));
    match toggle {
        LockToggle::Locked(window) => {
            info!(
                "Output locked to window {:#x} (process {})",
                window.handle.0, window.process_id
            );
            // The window is guaranteed alive right now, which is the one
            // moment its label is reliably queryable -- cache it so a later
            // loss (#266 review) can still name it after it (and often its
            // whole process) is gone.
            let label = window_label(window);
            if let Some(cache) = app.try_state::<LockedLabel>() {
                cache.set(window, label.clone());
            }
            if pinned.generation() == generation {
                let (app_name, title) = label;
                let _ = OutputTargetLockEvent::Locked {
                    app: app_name,
                    title,
                }
                .emit(app);
            }
        }
        LockToggle::Unlocked => {
            info!("Output lock released; delivery follows the foreground");
            if pinned.generation() == generation {
                let _ = OutputTargetLockEvent::Unlocked.emit(app);
            }
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
        if pinned.dismiss_lost_notice() {
            let _ = OutputTargetLockEvent::Unlocked.emit(app);
            crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
        }
        return;
    }
    // unlock() clears any lost-lock notice in the same critical section as
    // the mutation, so there is nothing left to do about it here.
    let generation = pinned.unlock();
    info!("Output lock released from the indicator");
    // Publish only while this release is still the newest word on the lock. A
    // lock taken in the gap between unlocking and announcing has already
    // emitted its own Locked; a blind Unlocked here would land after it and
    // leave every indicator claiming there is no lock while the backend holds
    // one -- the same generation check toggle_target_lock makes.
    if pinned.generation() == generation {
        let _ = OutputTargetLockEvent::Unlocked.emit(app);
        crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
    }
}

/// Read the current lock state for the indicator surfaces (#255).
///
/// Reports [`OutputTargetLockEvent::Lost`] when nothing is locked but
/// [`PinnedTarget::lost_notice`] still remembers the last loss (#266 review,
/// finding 1). The `Lost` kind was originally event-only, on the theory that
/// a mount after the loss could just read `Unlocked` -- but the event fires
/// once, to whichever webview happens to be listening at that moment, and a
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
        None => pinned
            .lost_notice()
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

/// Capture where a dictation starting now will be delivered, for its
/// [`DictationContext`](crate::dictation_context::DictationContext) (#160).
///
/// Read once, at recording start, and it carries the whole [`WindowIdentity`]:
/// the same identity every later step re-checks, so the dictation is never
/// resolved from a bare handle the OS may have recycled (#254). Everything
/// downstream carries this value, so a lock toggled while the user is still
/// speaking governs the next dictation rather than the one in flight.
///
/// A one-shot pick (#124) is deliberately NOT captured here. The picker is used
/// between dictations -- pick a window, then speak -- and the pick has to
/// outrank whatever this captured, including a lock captured before the pick was
/// even made. It is consumed instead at delivery, in
/// [`resolve_captured_delivery`], which is still exactly once per pick.
pub fn capture_delivery(app: &AppHandle) -> Delivery {
    let locked = match app.try_state::<PinnedTarget>() {
        Some(pinned) => pinned.locked(),
        None => {
            warn!("Target lock state is not initialized; delivering to the foreground");
            None
        }
    };

    match locked {
        Some(identity) => Delivery::Pinned(identity, DeliverySource::Lock),
        None => Delivery::Foreground,
    }
}

/// Resolve delivery for the target this dictation captured at recording start,
/// or `None` when the paste must be suppressed because that window is gone
/// (#120).
///
/// `sequence` is which dictation this is, in the order they were started, so a
/// one-shot pick is honored only by the dictation it was made for (#124): an
/// older transcript arriving late leaves the pick alone and goes where it was
/// captured.
///
/// The picker speaks first (#124). A pick in progress withholds the transcript
/// outright -- pasting into the window the user is picking from is exactly the
/// misfire the flow exists to avoid -- and otherwise a pending one-shot route is
/// consumed here: it routes THIS transcript and is then spent, overriding both
/// the captured target and any lock. A pick whose window has gone suppresses the
/// paste rather than letting it fall back to whatever now holds focus.
///
/// Otherwise the lock itself is deliberately not consulted: `captured` is what
/// this dictation was started for, so a lock toggled while the user was speaking
/// can neither redirect this paste nor rescue it. What is re-checked is the
/// captured identity, because a window can close during a dictation. A dead
/// target is dropped through [`drop_lock_for`], which clears the lock only if it
/// still points at this same window and announces the loss either way.
pub fn resolve_captured_delivery(
    app: &AppHandle,
    captured: Delivery,
    sequence: u64,
) -> Option<Delivery> {
    use crate::window_picker::{PasteVerdict, PickDelivery};

    match crate::window_picker::backend::paste_verdict(app, sequence) {
        // A pick is up, so this transcript finished mid-pick. The picker holds
        // the foreground, and delivering now would type the transcript into
        // AudioBud's own window (#164) -- including along a route armed before
        // this picker was opened, which is why the guard comes first and takes
        // nothing. Withhold the keystrokes; the text still reaches the clipboard
        // and history.
        PasteVerdict::WithholdForPicker => return None,
        PasteVerdict::Route(PickDelivery::Deliver(window)) => {
            return Some(Delivery::Pinned(window, DeliverySource::Pick))
        }
        // The user explicitly chose the current window, so this transcript
        // escapes the lock -- returning here is what makes that override real.
        PasteVerdict::Route(PickDelivery::Foreground) => return Some(Delivery::Foreground),
        PasteVerdict::Route(PickDelivery::PickLost) => return None,
        PasteVerdict::Captured => {}
    }

    let Delivery::Pinned(identity, source) = captured else {
        return Some(Delivery::Foreground);
    };

    // The identity validated here is the one the context has carried since
    // recording started, so there is no read of the lock to race with: the
    // window checked is by construction the window this delivery will be aimed
    // at.
    if window_is_alive(identity) {
        Some(Delivery::Pinned(identity, source))
    } else {
        abandon_target(app, identity, source);
        None
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

/// The application one delivery is about to reach, for the per-application
/// output settings (#123): the pinned or picked window's program name, or the
/// program name of whatever window holds focus for a plain foreground delivery.
///
/// Read live, at delivery time, because that is the moment the answer has to be
/// true -- the same moment `deliver_to_target` is deciding what to type. `None`
/// means no profile can apply and the global settings stand: a delivery that was
/// suppressed, a window whose program the OS will not name, and every delivery
/// on the platforms with no window backend yet (#119).
pub fn delivery_app_name(delivery: Option<Delivery>) -> Option<String> {
    let identity = match delivery? {
        Delivery::Pinned(identity, _) => identity,
        Delivery::Foreground => foreground_identity()?,
    };
    window_label(identity).0
}

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

/// Pick the label to report for a delivery that just succeeded (issue #165).
///
/// Unlike [`lost_label`] -- where the window is already gone and a live query
/// routinely comes back empty -- a delivery just proved the window alive a
/// moment ago, so the live lookup is tried first and used whenever it comes
/// back with anything. This matters because a locked window's content can
/// change after the lock was captured (a tab switch, a different document in
/// the same editor, #279 review round 2): reporting the label cached at lock
/// time would name whatever was open back then, not what the transcript
/// actually landed in. The cache remains the fallback -- for a live query
/// that fails outright (`(None, None)`), and only for a `Lock` delivery,
/// since that is the only source with a cache to fall back to; a one-shot
/// pick (#124) has none, so a failed live lookup for it reports unknown
/// rather than borrowing an unrelated lock's cached name.
///
/// `is_alive` guards the live lookup itself (#279 review round 5): between
/// the last focus check and this call the window can close, and the OS can
/// recycle its handle for an unrelated window before `live` runs.
/// `window_label`/`GetWindowTextW` would then read that replacement window's
/// title while the process name still comes from the identity's captured
/// PID, producing a false hybrid (a Chrome app name paired with a Notepad
/// title). `is_alive` re-validates the *whole* identity -- not just whether
/// the handle still resolves to *some* window -- immediately before `live`
/// runs, so a recycled handle is treated exactly like a live lookup that
/// came back empty: the cache is consulted instead, per the same rules as
/// above.
pub fn resolve_delivered_label(
    source: DeliverySource,
    cached: Option<WindowLabel>,
    is_alive: impl FnOnce() -> bool,
    live: impl FnOnce() -> WindowLabel,
) -> WindowLabel {
    let live_label = if is_alive() { live() } else { (None, None) };
    if live_label != (None, None) {
        return live_label;
    }
    if source.clears_the_lock() {
        if let Some(label) = cached {
            return label;
        }
    }
    live_label
}

/// Resolve [`resolve_delivered_label`] against the real cache, the real
/// identity probe, and the real platform label lookup.
fn delivered_label(
    app: &AppHandle,
    identity: WindowIdentity,
    source: DeliverySource,
) -> WindowLabel {
    let cached = app
        .try_state::<LockedLabel>()
        .and_then(|cache| cache.get(identity));
    resolve_delivered_label(
        source,
        cached,
        || window_is_alive(identity),
        || window_label(identity),
    )
}

/// Tell the user a transcript was delivered to a pinned window, naming it
/// (issue #165). Called once a delivery to `identity` has actually happened --
/// the paste succeeded and, per [`Delivery::target`], the window was not the
/// plain foreground.
pub fn announce_delivered(app: &AppHandle, identity: WindowIdentity, source: DeliverySource) {
    let (app_name, title) = delivered_label(app, identity, source);
    // The pipeline is about to finish and hide the overlay right behind this
    // (#279 review round 2); mark it so that hide gives the confirmation chip
    // this event is about to trigger enough time to actually be read instead
    // of the usual quick fade meant for a plain paste.
    crate::overlay::mark_delivery_confirmation_pending();
    // The window title routinely carries sensitive context -- document names,
    // page titles, client names -- and the app's default log_level (Debug)
    // admits everything down to `debug!` into the persistent handy.log, so
    // `debug!` is not an opt-in tier here (#279 review round 3): the title is
    // left out of the log entirely, not merely demoted. Only the handle and
    // app/process name, which is already what the lock/unlock events log
    // elsewhere, are logged.
    info!(
        "Transcript delivered to window {:#x} (app: {:?})",
        identity.handle.0, app_name
    );
    let _ = TranscriptDeliveredEvent {
        app: app_name,
        title,
        source,
    }
    .emit(app);
}

/// Tell the user the lock is gone, once, and put the tray and indicator
/// surfaces back in step (#255).
///
/// `label` is the locked window's last known app/title, read by the caller
/// before the lock was dropped -- by the time this runs the lock is already
/// gone, so this is the only chance to report who it was. `generation` is the
/// value [`PinnedTarget::retire_dead_target`]'s `LockCleared` produced.
///
/// The bare toast (`TARGET_LOCK_LOST_EVENT`) always fires: a real paste
/// attempt to the old target really did fail, whatever has happened since.
/// The *persistent* state -- the lost-lock notice and
/// `OutputTargetLockEvent::Lost` -- is conditioned on
/// [`PinnedTarget::record_lost_notice`], which atomically checks whether the
/// lock's generation has already moved past `generation` (#266 review round
/// 4). If so, a newer lock or unlock has already been established elsewhere
/// since this loss was detected, that operation's own event is already the
/// truth, and persisting `Lost` here would land after it and contradict it
/// -- the indicator would show "stale" while the backend is actually pinned
/// to something else (or plainly unlocked).
fn announce_lock_lost(app: &AppHandle, pinned: &PinnedTarget, generation: u64, label: WindowLabel) {
    warn!("Locked window is gone; the transcript was not delivered to it");
    let _ = app.emit(TARGET_LOCK_LOST_EVENT, ());

    if !pinned.record_lost_notice(generation, label.clone()) {
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

/// Give up on `target`, cleaning up the way its source requires.
///
/// A lost lock is cleared and announced as such; a lost one-shot pick announces
/// itself and leaves the lock alone, because the pick never held one and the
/// user's separate lock is still perfectly good (#124).
fn abandon_target(app: &AppHandle, target: WindowIdentity, source: DeliverySource) {
    if source.clears_the_lock() {
        drop_lock_for(app, target);
    } else {
        crate::window_picker::backend::announce_pick_lost(app);
    }
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
    let Some(pinned) = app.try_state::<PinnedTarget>() else {
        warn!("Target lock state is not initialized; a dead target went unreported");
        return;
    };

    match pinned.retire_dead_target(target) {
        // The window died mid-delivery: prefer the label cached from when it
        // was locked (#266 review) over a fresh query, which routinely comes
        // back empty for a window that just closed. `generation` is only
        // meaningful for this branch -- an obsolete target never touches the
        // indicator state at all, so it carries none.
        TargetLoss::LockCleared(generation) => {
            announce_lock_lost(app, &pinned, generation, lost_label(app, target));
        }
        TargetLoss::ObsoleteTarget => announce_target_window_gone(app, target),
    }
}

/// Tell the user this transcript reached no window, without touching the lock
/// state or the tray: the window that died is one they had already moved on
/// from, whether by unlocking or by locking onto something else.
fn announce_target_window_gone(app: &AppHandle, target: WindowIdentity) {
    warn!(
        "Window {:#x} closed before its transcript was delivered; the lock state is untouched",
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
    /// The window picker opened while the delivery was under way, so a
    /// foreground paste would now land in AudioBud's own window (#164). The
    /// notice is already sent; nothing more is typed.
    PickerOpened,
    /// The window is alive, but the system would not bring it forward. The lock
    /// still stands, so a retry can work.
    ActivationRefused(String),
}

impl std::fmt::Display for FocusLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusLost::TargetGone => write!(f, "the locked window closed during delivery"),
            FocusLost::PickerOpened => {
                write!(f, "the window picker opened during delivery")
            }
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
    source: DeliverySource,
}

impl<'a> FocusHold<'a> {
    /// A hold for `delivery`: its target, if it has one, and who aimed it there
    /// so a window lost mid-delivery is cleaned up the right way.
    pub fn new(app: &'a AppHandle, delivery: Delivery) -> Self {
        Self {
            app,
            target: delivery.target(),
            source: delivery.source(),
        }
    }

    /// Confirm the next keystroke will reach the intended window.
    ///
    /// Fails closed. A foreground delivery is withheld if the window picker has
    /// opened since it was resolved ([`FocusLost::PickerOpened`]), because its
    /// keystrokes would land in the picker. If the target has closed, the lock
    /// is dropped, the notice is sent, and this reports
    /// [`FocusLost::TargetGone`] so the caller sends nothing more. If the target merely lost focus, it is re-activated once; a
    /// refused activation is [`FocusLost::ActivationRefused`] rather than typing
    /// into the window that took focus.
    pub fn ensure(&self) -> Result<(), FocusLost> {
        let Some(target) = self.target else {
            // A foreground delivery has no window of its own to re-activate, so
            // this is its only guard -- and it is needed at every keystroke, not
            // just when the delivery was resolved: the Enigo mutex wait, the
            // clipboard write and `paste_delay_ms` all sit in between, and a
            // picker opened in any of those gaps holds the foreground now.
            if !crate::window_picker::foreground_keystrokes_allowed(
                crate::window_picker::backend::pick_in_progress(self.app),
            ) {
                crate::window_picker::backend::announce_pick_in_progress(self.app);
                return Err(FocusLost::PickerOpened);
            }
            return Ok(());
        };

        if !window_is_alive(target) {
            abandon_target(self.app, target, self.source);
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
    source: DeliverySource,
    action: impl FnOnce() -> T,
) -> Result<Borrowed<T>, FocusLost> {
    if !window_is_alive(target) {
        abandon_target(app, target, source);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output_target::{class_fingerprint, WindowHandle};

    fn window() -> WindowIdentity {
        WindowIdentity {
            handle: WindowHandle(7),
            process_id: 100,
            thread_id: 200,
            class: class_fingerprint("Test_WindowClass"),
        }
    }

    #[test]
    fn a_delivery_names_its_target_and_who_aimed_it() {
        assert_eq!(Delivery::Foreground.target(), None);
        for source in [DeliverySource::Lock, DeliverySource::Pick] {
            let delivery = Delivery::Pinned(window(), source);
            assert_eq!(delivery.target(), Some(window()));
            assert_eq!(delivery.source(), source);
        }
    }

    #[test]
    fn only_the_locks_own_delivery_clears_the_lock() {
        // A one-shot pick that loses its window must leave a lock the user set
        // separately completely alone (#124); the lock's delivery is the only
        // one entitled to clear it.
        assert!(DeliverySource::Lock.clears_the_lock());
        assert!(!DeliverySource::Pick.clears_the_lock());
    }

    #[test]
    fn a_locked_delivery_prefers_the_live_label_over_a_stale_cache() {
        // The window's content can change after the lock was captured -- a tab
        // switch, a different document in the same editor (#279 review round
        // 2) -- so a live lookup that comes back with anything wins over
        // whatever was cached at lock time, even though the cache exists.
        let cached = (Some("Terminal".to_string()), Some("zsh".to_string()));
        let live = (
            Some("Terminal".to_string()),
            Some("vim - notes.md".to_string()),
        );
        let label =
            resolve_delivered_label(DeliverySource::Lock, Some(cached), || true, || live.clone());
        assert_eq!(label, live);
    }

    #[test]
    fn a_locked_delivery_falls_back_to_its_cached_label_when_the_live_lookup_is_empty() {
        // The live query can fail outright even for a window that is still
        // there (a transient OS refusal); the label cached at lock time
        // (#266 review) is still better than reporting nothing.
        let cached = (Some("Terminal".to_string()), Some("zsh".to_string()));
        let label = resolve_delivered_label(
            DeliverySource::Lock,
            Some(cached.clone()),
            || true,
            || (None, None),
        );
        assert_eq!(label, cached);
    }

    #[test]
    fn a_locked_delivery_falls_back_to_a_live_lookup_with_no_cache_entry() {
        // Nothing cached -- state not yet managed, or this identity was never
        // actually the one locked -- so the live label is used instead of
        // reporting an empty name.
        let live = (Some("Notepad".to_string()), None);
        let label = resolve_delivered_label(DeliverySource::Lock, None, || true, || live.clone());
        assert_eq!(label, live);
    }

    #[test]
    fn a_picked_delivery_always_reads_the_label_live() {
        // A one-shot pick (#124) caches nothing of its own, so even a
        // (deliberately impossible in practice) cache hit must not be used --
        // only Lock deliveries are entitled to the cache.
        let cached = (Some("Stale".to_string()), None);
        let live = (Some("Fresh".to_string()), None);
        let label =
            resolve_delivered_label(DeliverySource::Pick, Some(cached), || true, || live.clone());
        assert_eq!(label, live);
    }

    #[test]
    fn a_picked_delivery_reports_unknown_rather_than_borrow_an_unrelated_cache() {
        // A one-shot pick has no cache of its own; an empty live lookup for
        // one must not fall back to a Lock's cache even if one happens to be
        // present, or the confirmation would name a window the pick never
        // touched.
        let cached = (Some("Unrelated Lock".to_string()), None);
        let label =
            resolve_delivered_label(DeliverySource::Pick, Some(cached), || true, || (None, None));
        assert_eq!(label, (None, None));
    }

    #[test]
    fn a_recycled_handle_is_never_read_as_the_delivered_window() {
        // Between the last focus check and this lookup the window can close
        // and the OS can recycle its handle for an unrelated window (#279
        // review round 5). `is_alive` catches that: when it says the handle
        // no longer belongs to the identity that was delivered to, `live`
        // must not run at all, so a Notepad title can never be read onto a
        // Chrome app name captured from the identity's stale PID.
        let cached = (Some("Terminal".to_string()), Some("zsh".to_string()));
        let label = resolve_delivered_label(
            DeliverySource::Lock,
            Some(cached.clone()),
            || false,
            || panic!("live must not run once is_alive says the identity no longer matches"),
        );
        assert_eq!(label, cached);
    }

    #[test]
    fn a_recycled_handle_with_no_cache_reports_unknown() {
        // A one-shot pick (#124) has no cache to fall back to, so a recycled
        // handle for it must report unknown rather than read the
        // replacement window's label.
        let label = resolve_delivered_label(
            DeliverySource::Pick,
            None,
            || false,
            || panic!("live must not run once is_alive says the identity no longer matches"),
        );
        assert_eq!(label, (None, None));
    }

    #[test]
    fn transcript_delivered_event_carries_the_resolved_label() {
        // Payload shaping: the event mirrors the (app, title) tuple exactly,
        // both halves optional, matching OutputTargetLockEvent's shape.
        let event = TranscriptDeliveredEvent {
            app: Some("Terminal".to_string()),
            title: Some("zsh".to_string()),
            source: DeliverySource::Lock,
        };
        assert_eq!(event.app.as_deref(), Some("Terminal"));
        assert_eq!(event.title.as_deref(), Some("zsh"));
        assert_eq!(event.source, DeliverySource::Lock);

        let unnamed = TranscriptDeliveredEvent {
            app: None,
            title: None,
            source: DeliverySource::Pick,
        };
        assert_eq!(unnamed.app, None);
        assert_eq!(unnamed.title, None);
        assert_eq!(unnamed.source, DeliverySource::Pick);
    }
}
