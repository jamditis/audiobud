//! One-shot window picker: route a single transcript to a chosen window (#124).
//!
//! The target lock (#120) pins EVERY later transcript to one window until the
//! user unlocks. The one-shot picker is the other half of the epic (#119): the
//! user picks a window for THIS transcript only, and delivery returns to the
//! foreground next time. It reuses the [`OutputTarget`] vocabulary and the same
//! focus-borrow paste path a `Pinned` target uses, but stores no lasting lock.
//!
//! This module is the platform-independent core. It does three things:
//!   - filters and labels the candidate windows the OS enumeration returns,
//!   - turns the user's terminal gesture into a [`PickOutcome`] a dismissal
//!     cannot turn into a stray paste, and
//!   - optionally remembers the last pick ([`LastPick`]) so a repeat route is a
//!     single confirm.
//!
//! [`backend`] holds the platform half -- enumerating windows (`EnumWindows` +
//! `GetWindowTextW` + the owning process name), opening the picker window, and
//! arming the pick the paste path consumes (#259). A pick is re-validated at
//! paste time through the shared identity check (#254): the window can close
//! between the pick and the paste, so a live-at-pick handle is not a guarantee.
//!
//! Parts of the API are convenience the UI does not use yet, so the module
//! allows dead_code.
#![allow(dead_code)]

pub mod backend;

use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::output_target::{OutputTarget, WindowHandle, WindowIdentity};

/// One window as the OS enumeration reports it, before filtering. The Windows
/// backend fills this from `EnumWindows`: `handle` from the `HWND`, `title` from
/// `GetWindowTextW`, `app` from the owning process name, and `visible` from
/// `IsWindowVisible`. The core takes these already-fetched so it stays testable
/// without a window system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawWindow {
    pub handle: WindowHandle,
    pub title: String,
    /// The owning application name, when the backend could resolve it.
    pub app: Option<String>,
    pub visible: bool,
}

/// A window the user may pick: visible, titled, and not one of AudioBud's own.
/// The intended producer is [`visible_candidates`], which trims the title,
/// normalizes the app name, and drops blanks. The fields are public so the
/// picker UI can render them, which also means a caller can build one directly;
/// [`WindowCandidate::label`] still guards against a blank app rather than
/// assume the normalization ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCandidate {
    pub handle: WindowHandle,
    /// The window title, trimmed and non-empty as produced by
    /// [`visible_candidates`].
    pub title: String,
    /// The owning application name when known, trimmed and never blank as
    /// produced by [`visible_candidates`].
    pub app: Option<String>,
}

impl WindowCandidate {
    /// A one-line label for the picker row. The issue asks for app names, NOT
    /// just raw titles: three untitled-but-distinct browser windows read as one
    /// entry otherwise. So show both, `app: title`, and fall back to the title
    /// alone when the app is unknown, blank, or already the whole title.
    pub fn label(&self) -> String {
        match self.app.as_deref().map(str::trim) {
            Some(app) if !app.is_empty() && app != self.title => {
                format!("{app}: {}", self.title)
            }
            _ => self.title.clone(),
        }
    }
}

/// Filter raw enumeration output down to the windows worth offering.
///
/// Drops, in order: hidden windows, windows with an empty or whitespace title
/// (they have no readable label), AudioBud's own windows (`exclude` -- the
/// overlay and picker surfaces, so a paste can never route back into the picker
/// itself), and duplicate handles (keeping the first). Titles are trimmed.
///
/// `exclude` is small (AudioBud's handful of windows) and candidates are few, so
/// linear membership checks are used to avoid adding a `Hash` bound to
/// [`WindowHandle`] for this one call.
pub fn visible_candidates(
    raw: impl IntoIterator<Item = RawWindow>,
    exclude: &[WindowHandle],
) -> Vec<WindowCandidate> {
    let mut kept: Vec<WindowCandidate> = Vec::new();
    for w in raw {
        if !w.visible {
            continue;
        }
        let title = w.title.trim();
        if title.is_empty() {
            continue;
        }
        if exclude.contains(&w.handle) {
            continue;
        }
        if kept.iter().any(|c| c.handle == w.handle) {
            continue;
        }
        // Normalize the app name to match the title: trim it and treat a blank
        // as unknown, so every consumer sees a clean field, not just `label`.
        let app = w
            .app
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_string);
        kept.push(WindowCandidate {
            handle: w.handle,
            title: title.to_string(),
            app,
        });
    }
    kept
}

/// A [`WindowHandle`] as it crosses the Tauri boundary: a decimal STRING, never
/// a JS number. A handle is an `isize`, 64-bit on Win64, so a large `HWND`
/// exceeds a JavaScript number's 2^53 safe range and would be silently rounded
/// into a different window on the way to the paste path. The overlay carries the
/// value opaquely and echoes it back, so a string round-trips exactly.
pub fn handle_id(handle: WindowHandle) -> String {
    handle.0.to_string()
}

/// Parse a handle id produced by [`handle_id`]. Returns `None` for anything that
/// is not one, so a malformed gesture from the webview fails safe (the caller
/// treats it as a dismissal) instead of borrowing focus to a guessed handle.
pub fn parse_handle_id(id: &str) -> Option<WindowHandle> {
    id.trim().parse::<isize>().ok().map(WindowHandle)
}

/// One row as the overlay renders it: the opaque handle and the label the
/// backend already composed ([`WindowCandidate::label`]). The frontend never
/// recomputes the label, so the row text and its source cannot drift.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PickerWindow {
    pub handle: String,
    pub label: String,
}

/// A candidate together with the identity it had WHEN IT WAS OFFERED.
///
/// The identity is captured during enumeration, not when the user clicks. A
/// window can close in between and Windows can hand its `HWND` to an unrelated
/// new window, so identifying the handle at click time would happily capture the
/// impostor and every later liveness check would agree with it. Holding the
/// enumeration-time identity lets [`arm_pick`] demand that the window on the
/// other end of the handle is still the one the user was shown (#254).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferedWindow {
    pub candidate: WindowCandidate,
    pub identity: WindowIdentity,
}

/// Pair each candidate with the identity enumeration read for it, dropping any
/// candidate whose identity is missing -- a window that went between the two
/// reads, which must not be offered.
pub fn offered_windows(
    candidates: Vec<WindowCandidate>,
    identities: &[WindowIdentity],
) -> Vec<OfferedWindow> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            identities
                .iter()
                .find(|i| i.handle == candidate.handle)
                .map(|identity| OfferedWindow {
                    candidate,
                    identity: *identity,
                })
        })
        .collect()
}

/// The candidates behind the offered rows, for [`resolve_gesture`].
pub fn offered_candidates(offered: &[OfferedWindow]) -> Vec<WindowCandidate> {
    offered.iter().map(|o| o.candidate.clone()).collect()
}

/// Shape the offered rows for the overlay to render.
pub fn offer_rows(offered: &[OfferedWindow]) -> Vec<PickerWindow> {
    offered
        .iter()
        .map(|o| PickerWindow {
            handle: handle_id(o.candidate.handle),
            label: o.candidate.label(),
        })
        .collect()
}

/// The overlay's terminal gesture as it arrives over the Tauri boundary. The
/// tag and its lowercase variant names mirror the frontend `PickerGesture`
/// (`src/lib/window-picker-overlay.ts`) one-to-one, so the two halves share a
/// single vocabulary and the mapping below is the only translation step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PickerGesture {
    Chose { handle: String },
    Foreground,
    Dismiss,
}

impl PickerGesture {
    /// Map the wire gesture onto the core's [`PickGesture`]. A handle that is
    /// not a valid id reads as a dismissal, which [`resolve_gesture`] turns into
    /// [`PickOutcome::Cancel`]: a suppressed paste is the safe reading of a
    /// gesture the backend cannot make sense of.
    pub fn to_gesture(&self) -> PickGesture {
        match self {
            PickerGesture::Chose { handle } => match parse_handle_id(handle) {
                Some(window) => PickGesture::Chose(window),
                None => PickGesture::Dismiss,
            },
            PickerGesture::Foreground => PickGesture::FocusForeground,
            PickerGesture::Dismiss => PickGesture::Dismiss,
        }
    }
}

/// What one resolved pick armed, for the log line and the overlay's reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PickArmed {
    /// The next transcript goes to this window, once.
    Window,
    /// The next transcript follows the foreground, as usual.
    Foreground,
    /// Nothing was armed: the user dismissed, or the chosen window was already
    /// gone. Either way no paste is redirected.
    Cancelled,
}

/// Turn a resolved [`PickOutcome`] into the route to arm.
///
/// A chosen window is honored only when the handle STILL BELONGS TO THE WINDOW
/// THAT WAS OFFERED: `identify` reads the owner now, and it must match the
/// identity enumeration recorded for that row. A window that closed under the
/// user's click fails that test, and so does one whose handle Windows has
/// already recycled into another application -- the case a click-time capture
/// would wave through, because the impostor is perfectly alive (#254). Either
/// way the pick is refused rather than pointed at a window the user never saw.
///
/// An explicit foreground send arms a route too, not nothing: it has to survive
/// as a pending state so it can override a target lock for this one transcript,
/// which is what the user asked for by choosing "use the current window".
pub fn arm_pick(
    outcome: PickOutcome,
    offered: &[OfferedWindow],
    identify: impl FnOnce(WindowHandle) -> Option<WindowIdentity>,
) -> (PickArmed, Option<PendingRoute>) {
    match outcome.delivery_target() {
        Some(OutputTarget::Pinned(handle)) => {
            let offered_identity = offered
                .iter()
                .find(|o| o.candidate.handle == handle)
                .map(|o| o.identity);
            match (offered_identity, identify(handle)) {
                (Some(offered), Some(current)) if offered == current => {
                    (PickArmed::Window, Some(PendingRoute::Window(current)))
                }
                _ => (PickArmed::Cancelled, None),
            }
        }
        Some(OutputTarget::Foreground) => (PickArmed::Foreground, Some(PendingRoute::Foreground)),
        None => (PickArmed::Cancelled, None),
    }
}

/// The windows the picker is currently offering. Held as Tauri state so the
/// resolve command validates the gesture against exactly the rows the user saw,
/// which is what keeps [`resolve_gesture`] able to reject a handle that was
/// never presented.
#[derive(Default)]
pub struct PickerSession(Mutex<Vec<OfferedWindow>>);

impl PickerSession {
    /// Record the rows now on offer, replacing any earlier ones.
    pub fn offer(&self, offered: Vec<OfferedWindow>) {
        *self.guard() = offered;
    }

    /// The rows currently on offer, each with the identity it was offered with.
    pub fn offered(&self) -> Vec<OfferedWindow> {
        self.guard().clone()
    }

    /// Whether a pick is in progress. A finishing transcript must not be pasted
    /// while it is: the picker holds the foreground, so the paste would land in
    /// AudioBud's own window (#164).
    pub fn is_open(&self) -> bool {
        !self.guard().is_empty()
    }

    /// Forget the offer once the pick has ended, so a late gesture cannot be
    /// resolved against a stale window list.
    pub fn clear(&self) {
        self.guard().clear();
    }

    fn guard(&self) -> MutexGuard<'_, Vec<OfferedWindow>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// What one pick armed for the next transcript.
///
/// The foreground is a route in its own right, not the absence of one. A user
/// who picks "use the current window" while a target lock is held is asking for
/// THIS transcript to escape the lock; representing that as "nothing armed"
/// would drop it straight back into the locked window (#120).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingRoute {
    /// Send the next transcript to this window.
    Window(WindowIdentity),
    /// Send the next transcript to whatever holds the foreground, overriding any
    /// lock for that one transcript.
    Foreground,
}

/// Where a pending one-shot pick sends the paste about to fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickDelivery {
    /// Deliver this transcript to the picked window. Carries the whole
    /// identity, not just the handle, because every later step of the delivery
    /// re-checks it (#254).
    Deliver(WindowIdentity),
    /// Deliver this transcript to the foreground window, ignoring any lock.
    Foreground,
    /// The picked window is gone. SUPPRESS the paste, exactly as a lost lock
    /// does ([`crate::output_target::Resolved::LockLost`]): a recycled handle
    /// must never receive the transcript (#254).
    PickLost,
}

/// The window one pick armed, consumed by the next paste and then forgotten --
/// this is what makes the picker one-shot rather than a lock. Tauri-managed
/// state alongside [`crate::output_target::PinnedTarget`]. The full identity is
/// stored, not just the handle, so the paste path can re-run the shared identity
/// check before typing anything (#254).
#[derive(Default)]
pub struct PendingPick(Mutex<Option<PendingRoute>>);

impl PendingPick {
    /// Route the next transcript along `route`, replacing any earlier pick.
    pub fn arm(&self, route: PendingRoute) {
        *self.guard() = Some(route);
    }

    /// Drop any pending pick, so the next transcript follows the usual rules.
    pub fn clear(&self) {
        *self.guard() = None;
    }

    /// Whether a pick is waiting to be delivered.
    pub fn is_armed(&self) -> bool {
        self.guard().is_some()
    }

    /// Consume the pending pick for the paste about to fire, or `None` when no
    /// pick is pending and the usual target rules apply.
    ///
    /// The pick is taken whichever way `is_alive` answers: one pick routes one
    /// transcript, and a pick whose window died is not held back for the next
    /// dictation. A dead window yields [`PickDelivery::PickLost`] so the caller
    /// suppresses the paste instead of falling back to the foreground.
    pub fn take_resolved(
        &self,
        is_alive: impl FnOnce(WindowIdentity) -> bool,
    ) -> Option<PickDelivery> {
        Some(match self.guard().take()? {
            PendingRoute::Foreground => PickDelivery::Foreground,
            PendingRoute::Window(picked) => {
                if is_alive(picked) {
                    PickDelivery::Deliver(picked)
                } else {
                    PickDelivery::PickLost
                }
            }
        })
    }

    /// Borrow the pick, recovering the guard if a previous holder panicked --
    /// same reasoning as [`LastPick::guard`].
    fn guard(&self) -> MutexGuard<'_, Option<PendingRoute>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The user's terminal action in the picker. Kept distinct from the outcome so
/// the two ways a paste can be suppressed-or-sent stay explicit at the UI edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickGesture {
    /// The user chose the window with this handle.
    Chose(WindowHandle),
    /// The user asked to send to the current foreground window instead of
    /// picking. Whether the picker even offers this (for example bound to Esc)
    /// is a UI decision the overlay makes (#124: decide during design); the core
    /// only needs it distinct from [`PickGesture::Dismiss`] so an explicit
    /// foreground send is never confused with a plain dismissal.
    FocusForeground,
    /// The user dismissed the picker without choosing.
    Dismiss,
}

/// The result of one pick. A distinct type from [`OutputTarget`] -- like
/// [`crate::output_target::Resolved`] -- so a dismissal, which must suppress the
/// paste, cannot be dropped like a stray bool or silently become a foreground
/// paste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickOutcome {
    /// Deliver this ONE transcript to the target, then forget it: the picker
    /// creates no lasting lock (that is [`crate::output_target::PinnedTarget`],
    /// #120). Choosing a window yields `Pinned`; choosing the foreground yields
    /// `Foreground`.
    DeliverOnce(OutputTarget),
    /// The pick was cancelled. SUPPRESS the paste; do NOT fall back to the
    /// foreground -- pasting into whatever now holds focus is the exact misfire
    /// the one-shot flow exists to avoid, the same fail-safe discipline as
    /// [`crate::output_target::Resolved::LockLost`].
    Cancel,
}

impl PickOutcome {
    /// The target to paste to, or `None` when the paste must be suppressed.
    /// Centralizes the "cancel is not foreground" rule so no caller can turn a
    /// dismissal into a stray paste by forgetting to handle the `Cancel` arm.
    pub fn delivery_target(self) -> Option<OutputTarget> {
        match self {
            PickOutcome::DeliverOnce(target) => Some(target),
            PickOutcome::Cancel => None,
        }
    }
}

/// Turn a terminal gesture into a [`PickOutcome`] against the windows that were
/// actually offered.
///
/// A `Chose` handle is honored only when it is still among `offered`. If it is
/// not -- the window closed after enumeration, or a stale gesture named a handle
/// never presented -- this fails safe to [`PickOutcome::Cancel`] rather than
/// borrowing focus to a handle the OS may have recycled. That is the same TOCTOU
/// the paste path must guard again at delivery time (#254); catching it here
/// keeps an already-gone window from ever reaching the focus-borrow.
pub fn resolve_gesture(gesture: PickGesture, offered: &[WindowCandidate]) -> PickOutcome {
    match gesture {
        PickGesture::Chose(handle) => {
            if offered.iter().any(|c| c.handle == handle) {
                PickOutcome::DeliverOnce(OutputTarget::Pinned(handle))
            } else {
                PickOutcome::Cancel
            }
        }
        PickGesture::FocusForeground => PickOutcome::DeliverOnce(OutputTarget::Foreground),
        PickGesture::Dismiss => PickOutcome::Cancel,
    }
}

/// Optional memory of the last window the user picked, so a repeat route is one
/// confirm instead of a fresh hunt. Tauri-managed state like
/// [`crate::output_target::PinnedTarget`]; the paste path reads and updates it
/// across invocations. `None` means nothing has been picked yet. This is a
/// convenience only -- unlike a lock it never redirects a paste on its own; it
/// just reorders what the picker shows.
#[derive(Default)]
pub struct LastPick(Mutex<Option<WindowHandle>>);

impl LastPick {
    /// Record `window` as the most recent pick.
    pub fn remember(&self, window: WindowHandle) {
        *self.guard() = Some(window);
    }

    /// Drop the remembered pick.
    pub fn forget(&self) {
        *self.guard() = None;
    }

    /// The remembered pick, if any.
    pub fn get(&self) -> Option<WindowHandle> {
        *self.guard()
    }

    /// Move the remembered pick to the front of `candidates` for quick repeat
    /// routing. If the remembered window is no longer offered it has closed, so
    /// forget it rather than keep pointing at a gone handle. A no-op when nothing
    /// is remembered or it is already first.
    pub fn promote_to_front(&self, candidates: &mut Vec<WindowCandidate>) {
        self.promote(candidates, |c| c.handle);
    }

    /// [`LastPick::promote_to_front`] for the rows the backend actually offers,
    /// which carry their enumeration-time identity alongside the candidate.
    pub fn promote_offered(&self, offered: &mut Vec<OfferedWindow>) {
        self.promote(offered, |o| o.candidate.handle);
    }

    /// Shared body of the two promotions above, over anything that can name its
    /// window handle, so the "remembered but no longer offered means forget it"
    /// rule lives in one place.
    fn promote<T>(&self, items: &mut Vec<T>, handle_of: impl Fn(&T) -> WindowHandle) {
        let mut guard = self.guard();
        let Some(remembered) = *guard else {
            return;
        };
        match items.iter().position(|item| handle_of(item) == remembered) {
            Some(0) => {}
            Some(i) => {
                let item = items.remove(i);
                items.insert(0, item);
            }
            None => *guard = None,
        }
    }

    /// Borrow the memory, recovering the guard if a previous holder panicked.
    /// The mutex only guards a `Copy` `Option<WindowHandle>` with no cross-field
    /// invariant, so a poisoned guard's value is always consistent; recovering it
    /// keeps one panic from bricking every later pick on `unwrap` (AGENTS.md:
    /// avoid unwrap in production).
    fn guard(&self) -> MutexGuard<'_, Option<WindowHandle>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output_target::PinnedTarget;

    fn raw(handle: isize, title: &str, app: Option<&str>, visible: bool) -> RawWindow {
        RawWindow {
            handle: WindowHandle(handle),
            title: title.to_string(),
            app: app.map(str::to_string),
            visible,
        }
    }

    #[test]
    fn candidates_drop_hidden_untitled_excluded_and_duplicates() {
        let raws = vec![
            raw(1, "  Notes  ", Some("TextEdit"), true), // kept, title trimmed
            raw(2, "Hidden", Some("App"), false),        // dropped: not visible
            raw(3, "   ", Some("App"), true),            // dropped: blank title
            raw(4, "Overlay", Some("AudioBud"), true),   // dropped: excluded (self)
            raw(1, "Notes again", Some("TextEdit"), true), // dropped: duplicate handle
        ];
        let got = visible_candidates(raws, &[WindowHandle(4)]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].handle, WindowHandle(1));
        assert_eq!(got[0].title, "Notes"); // trimmed
    }

    #[test]
    fn candidates_normalize_the_app_name() {
        let got = visible_candidates(
            vec![
                raw(1, "Doc", Some("  TextEdit  "), true), // padded app trimmed
                raw(2, "Bare", Some("   "), true),         // blank app -> None
                raw(3, "None", None, true),
            ],
            &[],
        );
        assert_eq!(got[0].app.as_deref(), Some("TextEdit"));
        assert_eq!(got[1].app, None);
        assert_eq!(got[2].app, None);
    }

    #[test]
    fn label_shows_app_and_title_then_falls_back() {
        let with_app = WindowCandidate {
            handle: WindowHandle(1),
            title: "notes.txt".to_string(),
            app: Some("TextEdit".to_string()),
        };
        assert_eq!(with_app.label(), "TextEdit: notes.txt");

        let no_app = WindowCandidate {
            handle: WindowHandle(2),
            title: "Untitled".to_string(),
            app: None,
        };
        assert_eq!(no_app.label(), "Untitled");

        // A blank or redundant app name adds nothing, so show the title alone.
        let blank_app = WindowCandidate {
            handle: WindowHandle(3),
            title: "Terminal".to_string(),
            app: Some("   ".to_string()),
        };
        assert_eq!(blank_app.label(), "Terminal");
        let same_app = WindowCandidate {
            handle: WindowHandle(4),
            title: "Slack".to_string(),
            app: Some("Slack".to_string()),
        };
        assert_eq!(same_app.label(), "Slack");
    }

    #[test]
    fn choosing_an_offered_window_delivers_once_to_it() {
        let offered = visible_candidates(vec![raw(7, "Mail", Some("Mail"), true)], &[]);
        let outcome = resolve_gesture(PickGesture::Chose(WindowHandle(7)), &offered);
        assert_eq!(
            outcome,
            PickOutcome::DeliverOnce(OutputTarget::Pinned(WindowHandle(7)))
        );
        assert_eq!(
            outcome.delivery_target(),
            Some(OutputTarget::Pinned(WindowHandle(7)))
        );
    }

    #[test]
    fn choosing_a_window_creates_no_lasting_lock() {
        // The one-shot pick must not touch the target lock (#120): a single
        // paste is routed, and delivery returns to the foreground next time.
        let lock = PinnedTarget::default();
        let offered = visible_candidates(vec![raw(7, "Mail", Some("Mail"), true)], &[]);
        let _ = resolve_gesture(PickGesture::Chose(WindowHandle(7)), &offered);
        assert!(!lock.is_locked());
    }

    #[test]
    fn choosing_a_gone_window_fails_safe_to_cancel() {
        // The chosen handle is not among those offered -- it closed after
        // enumeration. Suppress rather than paste into a recycled handle (#254).
        let offered = visible_candidates(vec![raw(7, "Mail", Some("Mail"), true)], &[]);
        let outcome = resolve_gesture(PickGesture::Chose(WindowHandle(999)), &offered);
        assert_eq!(outcome, PickOutcome::Cancel);
        assert_eq!(outcome.delivery_target(), None);
    }

    #[test]
    fn foreground_gesture_delivers_once_to_foreground() {
        let outcome = resolve_gesture(PickGesture::FocusForeground, &[]);
        assert_eq!(outcome, PickOutcome::DeliverOnce(OutputTarget::Foreground));
        assert_eq!(outcome.delivery_target(), Some(OutputTarget::Foreground));
    }

    #[test]
    fn dismiss_suppresses_and_is_not_foreground() {
        let outcome = resolve_gesture(PickGesture::Dismiss, &[]);
        assert_eq!(outcome, PickOutcome::Cancel);
        // The whole point of a distinct Cancel: a dismissal never pastes.
        assert_eq!(outcome.delivery_target(), None);
    }

    #[test]
    fn last_pick_remembers_and_forgets() {
        let last = LastPick::default();
        assert_eq!(last.get(), None);
        last.remember(WindowHandle(3));
        assert_eq!(last.get(), Some(WindowHandle(3)));
        last.forget();
        assert_eq!(last.get(), None);
    }

    #[test]
    fn promote_moves_remembered_pick_to_front() {
        let last = LastPick::default();
        last.remember(WindowHandle(2));
        let mut candidates = visible_candidates(
            vec![
                raw(1, "First", None, true),
                raw(2, "Second", None, true),
                raw(3, "Third", None, true),
            ],
            &[],
        );
        last.promote_to_front(&mut candidates);
        assert_eq!(candidates[0].handle, WindowHandle(2));
        // The rest keep their relative order.
        assert_eq!(candidates[1].handle, WindowHandle(1));
        assert_eq!(candidates[2].handle, WindowHandle(3));
    }

    #[test]
    fn promote_offered_moves_the_remembered_row_to_the_front() {
        let last = LastPick::default();
        last.remember(WindowHandle(2));
        let mut offered = offer(vec![
            raw(1, "First", None, true),
            raw(2, "Second", None, true),
            raw(3, "Third", None, true),
        ]);
        last.promote_offered(&mut offered);
        assert_eq!(offered[0].candidate.handle, WindowHandle(2));
        assert_eq!(offered[0].identity.handle, WindowHandle(2));
        assert_eq!(offered[1].candidate.handle, WindowHandle(1));
    }

    #[test]
    fn promote_forgets_a_pick_no_longer_offered() {
        let last = LastPick::default();
        last.remember(WindowHandle(9));
        let mut candidates = visible_candidates(vec![raw(1, "Only", None, true)], &[]);
        last.promote_to_front(&mut candidates);
        // Window 9 is gone, so the memory is cleared and the list is untouched.
        assert_eq!(last.get(), None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].handle, WindowHandle(1));
    }

    #[test]
    fn promote_is_a_noop_with_no_memory_or_already_first() {
        let last = LastPick::default();
        let mut candidates = visible_candidates(
            vec![raw(1, "First", None, true), raw(2, "Second", None, true)],
            &[],
        );
        last.promote_to_front(&mut candidates); // nothing remembered
        assert_eq!(candidates[0].handle, WindowHandle(1));

        last.remember(WindowHandle(1)); // already at front
        last.promote_to_front(&mut candidates);
        assert_eq!(candidates[0].handle, WindowHandle(1));
        assert_eq!(candidates[1].handle, WindowHandle(2));
    }

    fn identity(handle: isize, pid: u32, tid: u32) -> WindowIdentity {
        WindowIdentity {
            handle: WindowHandle(handle),
            process_id: pid,
            thread_id: tid,
        }
    }

    #[test]
    fn a_handle_id_round_trips_including_values_a_js_number_would_round() {
        for handle in [0isize, 1, -1, 0x0001_2345, isize::MAX, isize::MIN] {
            let id = handle_id(WindowHandle(handle));
            assert_eq!(parse_handle_id(&id), Some(WindowHandle(handle)));
        }
        // The reason ids are strings: this HWND is past 2^53, so a JS number
        // would hand the paste path a different window.
        let big = WindowHandle(9_007_199_254_740_993);
        assert_eq!(handle_id(big), "9007199254740993");
        assert_eq!(parse_handle_id("9007199254740993"), Some(big));
    }

    #[test]
    fn a_malformed_handle_id_is_not_a_handle() {
        for id in ["", "  ", "0x10", "12.5", "not-a-handle"] {
            assert_eq!(parse_handle_id(id), None);
        }
    }

    /// The rows the backend would offer for `raws`, with each window's identity
    /// as enumeration read it (process and thread stand in for the real ones).
    fn offer(raws: Vec<RawWindow>) -> Vec<OfferedWindow> {
        let identities: Vec<WindowIdentity> = raws
            .iter()
            .map(|w| identity(w.handle.0, 100, 200))
            .collect();
        offered_windows(visible_candidates(raws, &[]), &identities)
    }

    #[test]
    fn rows_carry_the_backend_label_and_a_string_handle() {
        let offered = offer(vec![
            raw(9_007_199_254_740_993, "notes.txt", Some("TextEdit"), true),
            raw(2, "Terminal", None, true),
        ]);
        let rows = offer_rows(&offered);
        assert_eq!(rows[0].handle, "9007199254740993");
        assert_eq!(rows[0].label, "TextEdit: notes.txt");
        assert_eq!(rows[1].handle, "2");
        assert_eq!(rows[1].label, "Terminal");
    }

    #[test]
    fn wire_gestures_map_onto_the_core_vocabulary() {
        assert_eq!(
            PickerGesture::Chose {
                handle: "42".to_string()
            }
            .to_gesture(),
            PickGesture::Chose(WindowHandle(42))
        );
        assert_eq!(
            PickerGesture::Foreground.to_gesture(),
            PickGesture::FocusForeground
        );
        assert_eq!(PickerGesture::Dismiss.to_gesture(), PickGesture::Dismiss);
    }

    #[test]
    fn a_gesture_with_an_unreadable_handle_fails_safe_to_dismiss() {
        let gesture = PickerGesture::Chose {
            handle: "nonsense".to_string(),
        };
        assert_eq!(gesture.to_gesture(), PickGesture::Dismiss);
        assert_eq!(
            resolve_gesture(gesture.to_gesture(), &[]),
            PickOutcome::Cancel
        );
    }

    #[test]
    fn wire_gestures_deserialize_from_the_overlays_shape() {
        let chose: PickerGesture =
            serde_json::from_str(r#"{"kind":"chose","handle":"42"}"#).expect("valid gesture");
        assert_eq!(
            chose,
            PickerGesture::Chose {
                handle: "42".to_string()
            }
        );
        let foreground: PickerGesture =
            serde_json::from_str(r#"{"kind":"foreground"}"#).expect("valid gesture");
        assert_eq!(foreground, PickerGesture::Foreground);
        let dismiss: PickerGesture =
            serde_json::from_str(r#"{"kind":"dismiss"}"#).expect("valid gesture");
        assert_eq!(dismiss, PickerGesture::Dismiss);
    }

    #[test]
    fn arming_a_chosen_window_records_the_identity_it_was_offered_with() {
        let offered = offer(vec![raw(7, "Mail", Some("Mail"), true)]);
        let picked = offered[0].identity;
        let armed = arm_pick(
            PickOutcome::DeliverOnce(OutputTarget::Pinned(WindowHandle(7))),
            &offered,
            |h| {
                assert_eq!(h, WindowHandle(7));
                Some(picked)
            },
        );
        assert_eq!(
            armed,
            (PickArmed::Window, Some(PendingRoute::Window(picked)))
        );
    }

    #[test]
    fn arming_a_window_that_died_between_offer_and_click_arms_nothing() {
        let offered = offer(vec![raw(7, "Mail", Some("Mail"), true)]);
        let armed = arm_pick(
            PickOutcome::DeliverOnce(OutputTarget::Pinned(WindowHandle(7))),
            &offered,
            |_| None,
        );
        assert_eq!(armed, (PickArmed::Cancelled, None));
    }

    #[test]
    fn arming_refuses_a_handle_recycled_since_it_was_offered() {
        // The click-time owner is alive and would pass every later liveness
        // check -- but it is a different window wearing the offered window's
        // handle (#254). Only the identity captured at enumeration can catch it.
        let offered = offer(vec![raw(7, "Mail", Some("Mail"), true)]);
        let impostor = identity(7, 999, 200);
        assert_ne!(offered[0].identity, impostor);
        assert_eq!(
            arm_pick(
                PickOutcome::DeliverOnce(OutputTarget::Pinned(WindowHandle(7))),
                &offered,
                |_| Some(impostor),
            ),
            (PickArmed::Cancelled, None)
        );
    }

    #[test]
    fn arming_refuses_a_handle_that_was_never_offered() {
        let offered = offer(vec![raw(7, "Mail", Some("Mail"), true)]);
        assert_eq!(
            arm_pick(
                PickOutcome::DeliverOnce(OutputTarget::Pinned(WindowHandle(8))),
                &offered,
                |_| Some(identity(8, 100, 200)),
            ),
            (PickArmed::Cancelled, None)
        );
    }

    #[test]
    fn arming_foreground_arms_a_route_of_its_own() {
        // Not "nothing armed": the foreground send has to outrank a target lock
        // for this one transcript, which only a pending route can do.
        assert_eq!(
            arm_pick(
                PickOutcome::DeliverOnce(OutputTarget::Foreground),
                &[],
                |_| panic!("foreground needs no identity"),
            ),
            (PickArmed::Foreground, Some(PendingRoute::Foreground))
        );
    }

    #[test]
    fn arming_a_cancelled_pick_arms_nothing() {
        assert_eq!(
            arm_pick(PickOutcome::Cancel, &[], |_| panic!(
                "a cancelled pick needs no identity"
            )),
            (PickArmed::Cancelled, None)
        );
    }

    #[test]
    fn offered_windows_drop_a_candidate_with_no_identity() {
        // Enumeration read the window, then it closed before its identity could
        // be taken. Offering it would hand out a handle with nothing behind it.
        let candidates = visible_candidates(
            vec![raw(1, "Mail", None, true), raw(2, "Notes", None, true)],
            &[],
        );
        let offered = offered_windows(candidates, &[identity(1, 100, 200)]);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].candidate.handle, WindowHandle(1));
    }

    #[test]
    fn a_session_offers_then_forgets_its_rows() {
        let session = PickerSession::default();
        assert!(session.offered().is_empty());
        assert!(!session.is_open());
        let offered = offer(vec![raw(1, "Mail", None, true)]);
        session.offer(offered.clone());
        assert_eq!(session.offered(), offered);
        assert!(session.is_open());
        session.clear();
        assert!(!session.is_open());
        // A late gesture now resolves against nothing, so it cannot pick.
        assert_eq!(
            resolve_gesture(
                PickGesture::Chose(WindowHandle(1)),
                &offered_candidates(&session.offered()),
            ),
            PickOutcome::Cancel
        );
    }

    #[test]
    fn a_pending_pick_routes_exactly_one_paste() {
        let pending = PendingPick::default();
        assert!(!pending.is_armed());
        // With nothing armed, the paste falls through to the usual target rules.
        assert_eq!(pending.take_resolved(|_| panic!("no pick to check")), None);

        let picked = identity(7, 100, 200);
        pending.arm(PendingRoute::Window(picked));
        assert!(pending.is_armed());
        assert_eq!(
            pending.take_resolved(|w| {
                assert_eq!(w, picked);
                true
            }),
            Some(PickDelivery::Deliver(picked))
        );
        // One pick, one paste: the second transcript is a normal one.
        assert!(!pending.is_armed());
        assert_eq!(pending.take_resolved(|_| true), None);
    }

    #[test]
    fn a_picked_window_that_closed_suppresses_the_paste() {
        let pending = PendingPick::default();
        pending.arm(PendingRoute::Window(identity(7, 100, 200)));
        // Same fail-safe as a lost lock: never fall back to the foreground.
        assert_eq!(
            pending.take_resolved(|_| false),
            Some(PickDelivery::PickLost)
        );
        assert!(!pending.is_armed());
    }

    #[test]
    fn a_recycled_handle_is_not_the_picked_window() {
        // End to end through the shared identity check (#254): the handle is a
        // window again, but a different process owns it now.
        let pending = PendingPick::default();
        let picked = identity(7, 100, 200);
        pending.arm(PendingRoute::Window(picked));
        let delivery = pending
            .take_resolved(|w| crate::output_target::identity_is_alive(w, |_| Some((999, 200))));
        assert_eq!(delivery, Some(PickDelivery::PickLost));
    }

    #[test]
    fn a_foreground_pick_overrides_a_lock_for_exactly_one_transcript() {
        // The user picked "use the current window" while a lock was held, so
        // this transcript must escape the lock -- and only this one.
        let pending = PendingPick::default();
        pending.arm(PendingRoute::Foreground);
        assert!(pending.is_armed());
        assert_eq!(
            pending.take_resolved(|_| panic!("the foreground has no identity to check")),
            Some(PickDelivery::Foreground)
        );
        assert!(!pending.is_armed());
        // The next transcript follows the usual rules again, lock included.
        assert_eq!(pending.take_resolved(|_| true), None);
    }

    #[test]
    fn clearing_a_pending_pick_returns_to_the_usual_target() {
        let pending = PendingPick::default();
        pending.arm(PendingRoute::Window(identity(7, 100, 200)));
        pending.clear();
        assert!(!pending.is_armed());
        assert_eq!(pending.take_resolved(|_| panic!("no pick to check")), None);
    }

    #[test]
    fn last_pick_recovers_from_a_poisoned_lock() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let last = LastPick::default();
        last.remember(WindowHandle(5));
        // A panic while holding the guard poisons the mutex. Recovery must let
        // later reads proceed instead of panicking on every subsequent pick.
        let blew_up = catch_unwind(AssertUnwindSafe(|| {
            let _guard = last.guard();
            panic!("holder blew up");
        }));
        assert!(blew_up.is_err());
        assert_eq!(last.get(), Some(WindowHandle(5)));
    }
}
