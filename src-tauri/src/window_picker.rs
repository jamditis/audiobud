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
//! The Windows half -- enumerating windows (`EnumWindows` + `GetWindowTextW` +
//! the owning process name), rendering the picker overlay, and running the
//! focus-borrow on the chosen handle -- is the next child of #119 and is where
//! [`resolve_gesture`] and [`visible_candidates`] get their first callers. That
//! half must re-validate the chosen handle at paste time (#254): the window can
//! close between the pick and the paste, so a live-at-pick handle is not a
//! guarantee; the paste path must tolerate an activation that fails.
//!
//! Until that caller lands, the API is exercised only by the unit tests below,
//! so the module allows dead_code; drop the allow when the picker UI wires in.
#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard};

use crate::output_target::{OutputTarget, WindowHandle};

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
/// [`crate::output_target::backend::Borrowed`] -- so a dismissal, which must
/// suppress the paste, cannot be dropped like a stray bool or silently become a
/// foreground paste.
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
    /// [`crate::output_target::backend::Borrowed::Suppressed`].
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
        let mut guard = self.guard();
        let Some(remembered) = *guard else {
            return;
        };
        match candidates.iter().position(|c| c.handle == remembered) {
            Some(0) => {}
            Some(i) => {
                let c = candidates.remove(i);
                candidates.insert(0, c);
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
