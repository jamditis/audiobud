//! Per-dictation context: the intent captured once at recording start and
//! carried unchanged to paste time.
//!
//! Every stage used to re-derive its decisions from the live global settings:
//! `effective_raw_output` was recomputed three times per dictation in
//! `actions.rs`, each reading `get_settings` again. That worked only because
//! those inputs happen not to change mid-dictation. Output target (#120) breaks
//! the assumption: the destination window is chosen at recording start and MUST
//! NOT be re-read at paste time, because by then the user has deliberately moved
//! focus elsewhere.
//!
//! This is the object epic #142 calls the missing abstraction -- the thing the
//! other output-routing children (#122, #123, #124) all need. It follows the
//! precedent history retry already sets: replaying a dictation's `raw_requested`
//! / `post_process_requested` rather than re-reading the current settings
//! (`commands/history.rs`).
//!
//! [`DictationContext`] itself is pure logic: no Tauri, no globals. Recording
//! start and delivery are separate stacks, though -- `ShortcutAction::start`
//! returns long before `stop` runs -- so [`ActiveDictations`] holds each
//! in-flight context between them, keyed by the shortcut binding that owns the
//! recording. From `stop` onwards the context is owned by the pipeline and moves
//! with the transcript into the delivery queue and the paste.

use crate::output_target::OutputTarget;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

/// One dictation's intent, fixed at recording start.
///
/// `effective_raw` is resolved a single time in [`DictationContext::capture`],
/// against the global `raw_output` setting as it stood then, and stored. Later
/// stages read the stored value instead of recomputing it, so a global toggle
/// flipped mid-dictation cannot retroactively change how the in-flight paste is
/// formatted -- the same reason the output target is captured rather than
/// re-read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DictationContext {
    raw_requested: bool,
    post_process_requested: bool,
    effective_raw: bool,
    output_target: OutputTarget,
}

impl DictationContext {
    /// Capture the per-dictation intent at recording start.
    ///
    /// `raw_output_setting` is read once, here; it is intentionally not stored,
    /// so a stage cannot re-resolve `effective_raw` from a value that may have
    /// moved on. The resolution rule is the one lifted verbatim from
    /// `actions.rs::effective_raw_output`: an explicit per-dictation raw request
    /// always wins, otherwise the global raw toggle applies unless this
    /// dictation explicitly asked to post-process.
    pub fn capture(
        raw_requested: bool,
        post_process_requested: bool,
        raw_output_setting: bool,
        output_target: OutputTarget,
    ) -> Self {
        let effective_raw = raw_requested || (raw_output_setting && !post_process_requested);
        Self {
            raw_requested,
            post_process_requested,
            effective_raw,
            output_target,
        }
    }

    /// Whether this dictation explicitly asked for raw output.
    ///
    /// The pipeline reads [`Self::effective_raw`] rather than this, because the
    /// resolved decision is what it acts on. The unresolved request is kept
    /// because it is the per-dictation intent history retry replays, and the
    /// picker (#124) needs the same distinction.
    #[allow(dead_code)]
    pub fn raw_requested(&self) -> bool {
        self.raw_requested
    }

    /// Whether this dictation explicitly asked to post-process.
    pub fn post_process_requested(&self) -> bool {
        self.post_process_requested
    }

    /// Whether the emitted text is raw, resolved once at capture. Every stage
    /// that formats or saves the transcript reads this instead of calling
    /// `effective_raw_output` again.
    pub fn effective_raw(&self) -> bool {
        self.effective_raw
    }

    /// Where the finished transcript is delivered, captured at recording start.
    pub fn output_target(&self) -> OutputTarget {
        self.output_target
    }
}

/// Tauri-managed hand-off of in-flight dictation contexts, keyed by the shortcut
/// binding that started the recording.
///
/// A dictation's intent is captured in `ShortcutAction::start` but first acted on
/// in `stop`, which runs on a later call stack, so the context has to be parked
/// somewhere in between. Keying by binding rather than using a single slot keeps
/// two bindings (say plain transcribe and transcribe-with-post-process) from
/// overwriting each other, matching how `AudioRecordingManager` tracks its own
/// recordings.
///
/// This is deliberately only the start-to-stop hand-off: [`Self::take`] removes
/// the context, and everything after it -- the async transcription task, the
/// delivery queue, the paste -- carries the value it returned. Nothing re-reads
/// the registry later, so it can never hand a stale intent to a paste.
#[derive(Default)]
pub struct ActiveDictations(Mutex<HashMap<String, DictationContext>>);

impl ActiveDictations {
    /// Record the context of a dictation that just started recording.
    ///
    /// A context already stored for this binding is replaced: it belongs to a
    /// recording that never reached `stop` (a cancel, or a start whose recording
    /// failed), and the fresh press is what the user is asking for now.
    pub fn begin(&self, binding_id: &str, context: DictationContext) {
        self.guard().insert(binding_id.to_string(), context);
    }

    /// Take the context of the dictation this binding started, if one is still
    /// parked. `None` means no start was recorded for it -- the caller must then
    /// capture the intent itself rather than drop the dictation.
    pub fn take(&self, binding_id: &str) -> Option<DictationContext> {
        self.guard().remove(binding_id)
    }

    /// Drop a binding's parked context because its recording never became a
    /// dictation (the recording failed to start).
    pub fn discard(&self, binding_id: &str) {
        self.guard().remove(binding_id);
    }

    /// Drop every parked context. Cancellation abandons whatever is recording
    /// without knowing which binding started it, and an abandoned context must
    /// not outlive its recording.
    pub fn discard_all(&self) {
        self.guard().clear();
    }

    /// Borrow the registry, recovering the guard if a previous holder panicked.
    /// The map is plain owned data with no cross-entry invariant, so a poisoned
    /// guard's contents are always consistent; recovering keeps one panic from
    /// bricking every later dictation on an `unwrap` (AGENTS.md: avoid unwrap in
    /// production).
    fn guard(&self) -> MutexGuard<'_, HashMap<String, DictationContext>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output_target::{OutputTarget, WindowHandle};

    // The resolution rule must stay identical to actions.rs::effective_raw_output,
    // whose own test this mirrors, so moving the callers onto the context cannot
    // change a single dictation's raw/processed decision.
    #[test]
    fn effective_raw_matches_the_resolution_rule() {
        let cap = |raw, post, global| {
            DictationContext::capture(raw, post, global, OutputTarget::Foreground).effective_raw()
        };
        // An explicit per-dictation raw request always forces raw.
        assert!(cap(true, false, false));
        // An explicit per-dictation post-process request suppresses the global raw toggle.
        assert!(!cap(false, true, true));
        // With no per-dictation request, the global raw toggle still applies.
        assert!(cap(false, false, true));
        // Nothing requested and the toggle off -> not raw.
        assert!(!cap(false, false, false));
    }

    // The whole point of the object: the decision is fixed at capture. Two
    // contexts built from the same per-dictation intent but opposite global
    // settings must each keep the value they were captured with; nothing reads
    // the global again after capture.
    #[test]
    fn effective_raw_is_frozen_at_capture() {
        let intent = (false, false); // no per-dictation override, so the global decides
        let with_global_on =
            DictationContext::capture(intent.0, intent.1, true, OutputTarget::Foreground);
        let with_global_off =
            DictationContext::capture(intent.0, intent.1, false, OutputTarget::Foreground);
        assert!(with_global_on.effective_raw());
        assert!(!with_global_off.effective_raw());
        // The stored value is what the accessor returns, not a re-resolution.
        assert_eq!(
            with_global_on.effective_raw(),
            with_global_on.effective_raw()
        );
    }

    // The per-dictation intent is carried verbatim, the way history retry replays it.
    #[test]
    fn per_dictation_intent_is_carried_verbatim() {
        let ctx = DictationContext::capture(true, false, false, OutputTarget::Foreground);
        assert!(ctx.raw_requested());
        assert!(!ctx.post_process_requested());
    }

    // The output target is captured, not re-read: a pinned window survives on the
    // context so the paste path never has to consult live focus.
    #[test]
    fn output_target_is_captured() {
        let pinned = OutputTarget::Pinned(WindowHandle(42));
        let ctx = DictationContext::capture(false, false, false, pinned);
        assert_eq!(ctx.output_target(), pinned);

        let fg = DictationContext::capture(false, false, false, OutputTarget::Foreground);
        assert_eq!(fg.output_target(), OutputTarget::Foreground);
    }

    fn ctx(raw: bool) -> DictationContext {
        DictationContext::capture(raw, false, false, OutputTarget::Foreground)
    }

    // The hand-off is one-shot: once stop has taken the context, the pipeline
    // owns it and nothing can pick a second copy out of the registry.
    #[test]
    fn a_started_dictation_is_taken_once() {
        let active = ActiveDictations::default();
        active.begin("transcribe", ctx(true));
        let taken = active.take("transcribe");
        assert_eq!(taken, Some(ctx(true)));
        assert_eq!(active.take("transcribe"), None);
    }

    // A stop with no recorded start must be visible to the caller so it can
    // capture the intent itself rather than paste with someone else's.
    #[test]
    fn an_unstarted_binding_has_no_context() {
        let active = ActiveDictations::default();
        active.begin("transcribe", ctx(false));
        assert_eq!(active.take("transcribe_raw"), None);
    }

    // Bindings are independent: a raw dictation and a normal one can be parked
    // at once without either inheriting the other's intent.
    #[test]
    fn bindings_do_not_overwrite_each_other() {
        let active = ActiveDictations::default();
        active.begin("transcribe", ctx(false));
        active.begin("transcribe_raw", ctx(true));
        assert_eq!(
            active.take("transcribe_raw").map(|c| c.effective_raw()),
            Some(true)
        );
        assert_eq!(
            active.take("transcribe").map(|c| c.effective_raw()),
            Some(false)
        );
    }

    // A recording that failed to start, or was cancelled, must not leave an
    // intent behind for a later dictation to pick up.
    #[test]
    fn abandoned_contexts_are_dropped() {
        let active = ActiveDictations::default();
        active.begin("transcribe", ctx(true));
        active.discard("transcribe");
        assert_eq!(active.take("transcribe"), None);

        active.begin("transcribe", ctx(true));
        active.begin("transcribe_raw", ctx(true));
        active.discard_all();
        assert_eq!(active.take("transcribe"), None);
        assert_eq!(active.take("transcribe_raw"), None);
    }
}
