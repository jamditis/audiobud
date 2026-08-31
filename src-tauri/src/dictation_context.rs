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

use crate::output_target::backend::Delivery;
use std::collections::{HashMap, VecDeque};
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
    delivery_target: Delivery,
    sequence: u64,
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
        delivery_target: Delivery,
        sequence: u64,
    ) -> Self {
        let effective_raw = raw_requested || (raw_output_setting && !post_process_requested);
        Self {
            raw_requested,
            post_process_requested,
            effective_raw,
            delivery_target,
            sequence,
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
    ///
    /// A pinned target carries the whole `WindowIdentity`, not just its handle,
    /// so the delivery path re-checks the window this dictation was actually
    /// started for -- never a bare handle Windows may have recycled since (#254).
    pub fn delivery_target(&self) -> Delivery {
        self.delivery_target
    }

    /// Where this dictation falls in the order they were started.
    ///
    /// Deliveries do not finish in the order they began -- transcription,
    /// post-processing and the delivery queue all take their own time -- so
    /// "which dictation is this?" cannot be answered by whichever paste happens
    /// to arrive first. A one-shot pick (#124) is offered to the dictation the
    /// user made it FOR, which is the first one started after the pick, and this
    /// number is how the two are told apart.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

/// Counts dictations in the order they start, so a pick made between two of them
/// can be given to the right one.
///
/// Tauri-managed, ticked once per recording start. Only the ORDER matters, never
/// the value: it is compared with the count a pending pick recorded when it was
/// armed, and nothing else reads it.
#[derive(Default)]
pub struct DictationSequence(std::sync::atomic::AtomicU64);

impl DictationSequence {
    /// The number for a dictation starting now.
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    /// How many dictations have started so far.
    pub fn current(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The number for a dictation starting now, or 0 when the counter is missing --
/// in which case a pending pick is never consumed, which is the safe way round:
/// the transcript goes where the dictation was captured instead of somewhere the
/// user chose for a different one.
pub fn next_sequence(app: &tauri::AppHandle) -> u64 {
    use tauri::Manager;
    app.try_state::<DictationSequence>()
        .map(|counter| counter.next())
        .unwrap_or(0)
}

/// How many dictations have started so far, for stamping a pick as it is armed.
pub fn current_sequence(app: &tauri::AppHandle) -> u64 {
    use tauri::Manager;
    app.try_state::<DictationSequence>()
        .map(|counter| counter.current())
        .unwrap_or(0)
}

/// Idempotency gate at the final delivery boundary (#310).
///
/// The normal pipeline submits each sequence once, but a duplicated job must
/// still be harmless: clipboard, focus, and keyboard actions are side effects
/// that cannot be rolled back after the target application receives them. Keep
/// a bounded set of recent nonzero sequences and claim one before any of those
/// actions run. Sequence zero means the sequence counter was unavailable, so it
/// deliberately keeps the existing best-effort delivery behavior.
#[derive(Default)]
pub struct DeliverySequenceGate(Mutex<VecDeque<u64>>);

const RECENT_DELIVERY_SEQUENCE_CAPACITY: usize = 64;

impl DeliverySequenceGate {
    pub fn claim(&self, sequence: u64) -> bool {
        if sequence == 0 {
            return true;
        }

        let mut recent = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if recent.contains(&sequence) {
            return false;
        }

        recent.push_back(sequence);
        if recent.len() > RECENT_DELIVERY_SEQUENCE_CAPACITY {
            recent.pop_front();
        }
        true
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
    use crate::output_target::backend::DeliverySource;
    use crate::output_target::{class_fingerprint, PinnedTarget, WindowHandle, WindowIdentity};
    use std::sync::Arc;

    /// A captured window: handle `h`, owned by process `pid` / thread `tid`.
    fn win(h: isize, pid: u32, tid: u32) -> WindowIdentity {
        WindowIdentity {
            handle: WindowHandle(h),
            process_id: pid,
            thread_id: tid,
            class: class_fingerprint("Chrome_WidgetWin_1"),
        }
    }

    #[test]
    fn a_delivery_sequence_can_be_claimed_only_once() {
        let gate = DeliverySequenceGate::default();

        assert!(gate.claim(7));
        assert!(!gate.claim(7));
        assert!(gate.claim(8));
    }

    #[test]
    fn concurrent_duplicate_claims_have_one_winner() {
        let gate = Arc::new(DeliverySequenceGate::default());
        let winners = std::thread::scope(|scope| {
            let handles = (0..16)
                .map(|_| {
                    let gate = Arc::clone(&gate);
                    scope.spawn(move || gate.claim(42))
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("delivery claimant did not panic"))
                .filter(|won| *won)
                .count()
        });

        assert_eq!(winners, 1);
    }

    #[test]
    fn missing_sequence_state_keeps_best_effort_delivery() {
        let gate = DeliverySequenceGate::default();

        assert!(gate.claim(0));
        assert!(gate.claim(0));
    }

    // The resolution rule must stay identical to actions.rs::effective_raw_output,
    // whose own test this mirrors, so moving the callers onto the context cannot
    // change a single dictation's raw/processed decision.
    #[test]
    fn effective_raw_matches_the_resolution_rule() {
        let cap = |raw, post, global| {
            DictationContext::capture(raw, post, global, Delivery::Foreground, 1).effective_raw()
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
            DictationContext::capture(intent.0, intent.1, true, Delivery::Foreground, 1);
        let with_global_off =
            DictationContext::capture(intent.0, intent.1, false, Delivery::Foreground, 1);
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
        let ctx = DictationContext::capture(true, false, false, Delivery::Foreground, 1);
        assert!(ctx.raw_requested());
        assert!(!ctx.post_process_requested());
    }

    // The delivery target is captured, not re-read: a pinned window survives on
    // the context so the paste path never has to consult live focus. It carries
    // the full identity, which is what every later re-check needs (#254).
    #[test]
    fn delivery_target_is_captured_with_its_identity() {
        let window = win(42, 100, 200);
        let ctx = DictationContext::capture(
            false,
            false,
            false,
            Delivery::Pinned(window, DeliverySource::Lock),
            1,
        );
        assert_eq!(
            ctx.delivery_target(),
            Delivery::Pinned(window, DeliverySource::Lock)
        );

        let fg = DictationContext::capture(false, false, false, Delivery::Foreground, 1);
        assert_eq!(fg.delivery_target(), Delivery::Foreground);
    }

    // The reason the target is captured at all (#160): the lock can be released
    // or re-pointed while the user is still speaking, and the dictation already
    // under way must still go where it was started for. The context holds the
    // window itself, so nothing about the live lock can reach back into it.
    #[test]
    fn a_lock_toggled_mid_dictation_cannot_redirect_the_context() {
        let lock = PinnedTarget::default();
        let started_with = win(42, 100, 200);
        lock.lock_to(started_with);
        let ctx = DictationContext::capture(
            false,
            false,
            false,
            Delivery::Pinned(started_with, DeliverySource::Lock),
            1,
        );

        // Released mid-dictation.
        lock.unlock();
        assert_eq!(
            ctx.delivery_target(),
            Delivery::Pinned(started_with, DeliverySource::Lock)
        );

        // Re-pointed at another window mid-dictation: that governs the NEXT
        // dictation, not this one.
        lock.lock_to(win(7, 500, 600));
        assert_eq!(
            ctx.delivery_target(),
            Delivery::Pinned(started_with, DeliverySource::Lock)
        );
    }

    fn ctx(raw: bool) -> DictationContext {
        DictationContext::capture(raw, false, false, Delivery::Foreground, 1)
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
