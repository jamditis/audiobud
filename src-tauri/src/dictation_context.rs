//! Per-dictation context: the intent captured once at recording start and
//! carried unchanged to paste time.
//!
//! Today every stage re-derives its decisions from the live global settings.
//! `effective_raw_output` is recomputed three times per dictation in
//! `actions.rs` (`:602`, `:701`, `:804`), each reading `get_settings` again.
//! That works only because those inputs happen not to change mid-dictation.
//! Output target (#120) breaks the assumption: the destination window is chosen
//! at recording start and MUST NOT be re-read at paste time, because by then the
//! user has deliberately moved focus elsewhere.
//!
//! This is the object epic #142 calls the missing abstraction -- the thing the
//! other output-routing children (#122, #123, #124) all need. It follows the
//! precedent history retry already sets: replaying a dictation's `raw_requested`
//! / `post_process_requested` rather than re-reading the current settings
//! (`commands/history.rs`).
//!
//! Pure logic only: no Tauri, no globals. A context is built once at start and
//! threaded through. Wiring the three `actions.rs` sites and the paste path onto
//! it (so they read the context instead of re-resolving from live settings) is
//! the next child of the epic; until that lands nothing constructs one, hence
//! the module-level dead_code allow, mirroring `output_target.rs`.
#![allow(dead_code)]

use crate::output_target::OutputTarget;

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
}
