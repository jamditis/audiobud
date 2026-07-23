//! Watchdog machinery for the transcription pipeline (issue #58).
//!
//! Kept free of any engine or Tauri dependency so it stays testable and is
//! shared by both the real `TranscriptionManager` and the CI mock
//! (`transcription_mock.rs` replaces `transcription.rs` in CI, so anything
//! living there would neither compile against nor be tested with the mock).

use log::{error, warn};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

/// Shortest watchdog deadline: even a tiny clip gets this long before the
/// pipeline gives up on the engine.
pub(crate) const TRANSCRIPTION_TIMEOUT_FLOOR: Duration = Duration::from_secs(120);

/// Longest watchdog deadline. A legitimate transcription that needs more than
/// this has an unusable UX anyway; bounding it keeps a wedged engine from
/// pinning the "transcribing" state for the rest of the session.
pub(crate) const TRANSCRIPTION_TIMEOUT_CEILING: Duration = Duration::from_secs(600);

/// Budget per second of recorded audio. 10x realtime covers a large Whisper
/// model on a slow CPU with headroom; anything slower is indistinguishable
/// from a hang for the user.
const TRANSCRIPTION_TIMEOUT_REALTIME_FACTOR: u64 = 10;

/// Watchdog deadline for a transcription of `sample_count` mono samples at
/// `sample_rate` Hz: 10x the audio duration, clamped to a generous
/// floor/ceiling so short clips aren't cut off early and long ones can't
/// wedge the UI forever.
pub(crate) fn transcription_watchdog_timeout(sample_count: usize, sample_rate: u32) -> Duration {
    let audio_secs = (sample_count as u64).div_ceil(u64::from(sample_rate.max(1)));
    Duration::from_secs(audio_secs.saturating_mul(TRANSCRIPTION_TIMEOUT_REALTIME_FACTOR))
        .clamp(TRANSCRIPTION_TIMEOUT_FLOOR, TRANSCRIPTION_TIMEOUT_CEILING)
}

/// Inner state of a [`GenerationGate`]: the stored value plus a counter that
/// bumps on every install/clear.
struct GateInner<T> {
    value: Option<T>,
    generation: u64,
}

/// Slot for the loaded engine, guarded by a generation counter (issue #58).
///
/// A transcription takes the engine out of the slot and puts it back when it
/// finishes. If the call wedges and its watchdog fires, the engine can come
/// back much later — after the model was unloaded or a different one loaded.
/// Every install/clear bumps the generation, and a take records the
/// generation observed, so a late restore whose generation no longer matches
/// is rejected and the stale engine is dropped instead of clobbering the slot.
pub(crate) struct GenerationGate<T> {
    inner: Mutex<GateInner<T>>,
}

impl<T> GenerationGate<T> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(GateInner {
                value: None,
                generation: 0,
            }),
        }
    }

    /// Lock the slot, recovering from poison if a holder panicked.
    fn lock(&self) -> MutexGuard<'_, GateInner<T>> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            warn!("Engine slot mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub(crate) fn is_occupied(&self) -> bool {
        self.lock().value.is_some()
    }

    /// Put a new value in the slot, replacing any previous one.
    pub(crate) fn install(&self, value: T) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.value = Some(value);
    }

    /// Empty the slot. Bumps the generation even when already empty, so a
    /// value currently taken out (a running transcription) cannot be restored
    /// after the unload.
    pub(crate) fn clear(&self) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.value = None;
    }

    /// Take the value out together with the generation observed at take time.
    pub(crate) fn take(&self) -> Option<(T, u64)> {
        let mut inner = self.lock();
        let generation = inner.generation;
        inner.value.take().map(|value| (value, generation))
    }

    /// Put a taken value back only if the slot generation is unchanged since
    /// the take. Returns `false` (dropping the stale value) if the slot was
    /// cleared or refilled in the meantime.
    #[must_use]
    pub(crate) fn try_restore(&self, value: T, taken_generation: u64) -> bool {
        let mut inner = self.lock();
        if inner.generation == taken_generation {
            inner.value = Some(value);
            true
        } else {
            false
        }
    }
}

/// How a watchdog-guarded call ended (issue #58).
#[derive(Debug)]
pub enum WatchdogOutcome<T> {
    /// The call finished within the deadline; its result is inside.
    Completed(T),
    /// The deadline passed with the worker still running. It is counted in
    /// the wedged-worker counter until it resolves.
    TimedOut,
    /// The worker panicked before producing a result. Distinct from
    /// [`WatchdogOutcome::TimedOut`] so callers don't report a false timeout.
    Panicked,
}

/// Handshake between the watchdog and its worker so the wedged-worker count
/// stays exact even when the worker finishes right at the deadline.
struct WatchdogState {
    finished: bool,
    timed_out: bool,
}

/// Run `f` and give up if it does not finish within `timeout` (issue #58).
/// The caller must treat anything but `Completed` as an error and recover the
/// UI/state itself.
///
/// Limitation: a wedged worker thread cannot be killed. On timeout it is left
/// running detached and `wedged_workers` is incremented until it resolves
/// (its late result is then logged and discarded, and the count decremented).
/// The counter lets the owner refuse new work while a wedged worker still
/// holds resources.
pub(crate) fn run_with_watchdog<T: Send + 'static>(
    operation: &'static str,
    timeout: Duration,
    wedged_workers: Arc<AtomicUsize>,
    f: impl FnOnce() -> T + Send + 'static,
) -> WatchdogOutcome<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let state = Arc::new(Mutex::new(WatchdogState {
        finished: false,
        timed_out: false,
    }));

    let worker_state = Arc::clone(&state);
    let worker_wedged = Arc::clone(&wedged_workers);
    thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(f));
        let panicked = result.is_err();
        if let Ok(value) = result {
            // Send before marking finished: once the watchdog observes
            // `finished`, a successful result is guaranteed to be in the
            // channel (so an empty channel + finished means a panic).
            let _ = tx.send(value);
        }
        let timed_out = {
            let mut st = worker_state.lock().unwrap_or_else(|p| p.into_inner());
            st.finished = true;
            if st.timed_out {
                // Increment and decrement both happen under this lock, so the
                // count can never underflow.
                worker_wedged.fetch_sub(1, Ordering::SeqCst);
            }
            st.timed_out
        };
        if timed_out {
            warn!(
                "{} resolved after its watchdog already fired; late {} discarded",
                operation,
                if panicked { "panic" } else { "result" }
            );
        }
        if panicked {
            error!("{} worker thread panicked", operation);
        }
        // tx drops here; a watchdog still waiting sees Disconnected on panic.
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => WatchdogOutcome::Completed(result),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // The worker panicked. transcribe() catches engine panics itself,
            // so this is unexpected — but it still must not wedge the pipeline.
            error!(
                "{} worker thread panicked before producing a result",
                operation
            );
            WatchdogOutcome::Panicked
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
            if st.finished {
                // The worker finished right at the deadline. On success its
                // result is already in the channel; nothing there means it
                // panicked.
                drop(st);
                match rx.try_recv() {
                    Ok(result) => WatchdogOutcome::Completed(result),
                    Err(_) => WatchdogOutcome::Panicked,
                }
            } else {
                st.timed_out = true;
                wedged_workers.fetch_add(1, Ordering::SeqCst);
                drop(st);
                error!(
                    "{} watchdog fired after {:?}; the engine appears wedged. The worker \
                     thread cannot be killed and is left running detached; further \
                     transcriptions and model loads are refused until it resolves.",
                    operation, timeout
                );
                WatchdogOutcome::TimedOut
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        run_with_watchdog, transcription_watchdog_timeout, GenerationGate, WatchdogOutcome,
        TRANSCRIPTION_TIMEOUT_CEILING, TRANSCRIPTION_TIMEOUT_FLOOR,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[test]
    fn watchdog_passes_through_a_result_that_arrives_in_time() {
        let wedged = counter();
        let outcome =
            run_with_watchdog("test", Duration::from_secs(5), Arc::clone(&wedged), || {
                "hello".to_string()
            });
        assert!(matches!(outcome, WatchdogOutcome::Completed(ref s) if s == "hello"));
        assert_eq!(wedged.load(Ordering::SeqCst), 0);
    }

    /// Reproduces issue #58: with no watchdog, a wedged engine call blocks the
    /// pipeline forever and the "transcribing" UI state is never cleared. The
    /// watchdog must report `TimedOut` at its deadline instead of waiting for
    /// the wedged call. The stand-in for a wedged engine sleeps well past the
    /// deadline (rather than literally forever) so a failing run still
    /// terminates.
    #[test]
    fn watchdog_times_out_on_a_wedged_transcribe_call() {
        let start = Instant::now();
        let outcome = run_with_watchdog("test", Duration::from_millis(100), counter(), || {
            std::thread::sleep(Duration::from_secs(3));
        });
        assert!(
            matches!(outcome, WatchdogOutcome::TimedOut),
            "a wedged transcribe call must time out, not produce a result"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the watchdog must fire at its deadline instead of waiting out the wedged call"
        );
    }

    /// A wedged worker is counted while it is still running so the owner can
    /// refuse new engine loads/transcriptions (no engine stacking), and the
    /// count clears when the worker finally resolves.
    #[test]
    fn watchdog_counts_a_wedged_worker_and_clears_it_when_it_resolves() {
        let wedged = counter();
        let outcome = run_with_watchdog(
            "test",
            Duration::from_millis(50),
            Arc::clone(&wedged),
            || std::thread::sleep(Duration::from_millis(400)),
        );
        assert!(matches!(outcome, WatchdogOutcome::TimedOut));
        assert_eq!(
            wedged.load(Ordering::SeqCst),
            1,
            "a timed-out worker must be counted as wedged while it is still running"
        );

        // The worker resolves ~350ms later; poll until the count clears.
        let deadline = Instant::now() + Duration::from_secs(5);
        while wedged.load(Ordering::SeqCst) != 0 {
            assert!(
                Instant::now() < deadline,
                "the wedged count must clear once the late worker resolves"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A worker panic must be reported as `Panicked`, not as a false
    /// "timed out after N seconds", and must not leave a wedged count behind.
    #[test]
    fn watchdog_reports_a_worker_panic_as_panicked_not_timed_out() {
        let wedged = counter();
        let outcome: WatchdogOutcome<()> =
            run_with_watchdog("test", Duration::from_secs(5), Arc::clone(&wedged), || {
                panic!("simulated engine panic")
            });
        assert!(
            matches!(outcome, WatchdogOutcome::Panicked),
            "a worker panic must surface as Panicked, not TimedOut or Completed"
        );
        assert_eq!(wedged.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn watchdog_timeout_scales_with_audio_duration_within_bounds() {
        const RATE: u32 = 16_000;
        // Short clip: clamped up to the floor.
        assert_eq!(
            transcription_watchdog_timeout(10 * RATE as usize, RATE),
            TRANSCRIPTION_TIMEOUT_FLOOR
        );
        // 30s of audio: 10x realtime = 300s, between floor and ceiling.
        assert_eq!(
            transcription_watchdog_timeout(30 * RATE as usize, RATE),
            Duration::from_secs(300)
        );
        // Very long recording: clamped down to the ceiling.
        assert_eq!(
            transcription_watchdog_timeout(600 * RATE as usize, RATE),
            TRANSCRIPTION_TIMEOUT_CEILING
        );
        // Empty audio still gets the floor (the caller guards this case anyway).
        assert_eq!(
            transcription_watchdog_timeout(0, RATE),
            TRANSCRIPTION_TIMEOUT_FLOOR
        );
    }

    #[test]
    fn generation_gate_restores_when_the_slot_is_untouched() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (value, generation) = gate.take().expect("installed value must be takeable");
        assert!(!gate.is_occupied());
        assert!(
            gate.try_restore(value, generation),
            "a take/restore with no intervening slot change must succeed"
        );
        assert!(gate.is_occupied());
    }

    /// A late-returning transcription must not resurrect its engine after the
    /// model was unloaded while it was out.
    #[test]
    fn generation_gate_rejects_a_restore_after_clear() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (value, generation) = gate.take().expect("installed value must be takeable");
        gate.clear();
        assert!(
            !gate.try_restore(value, generation),
            "restoring after an unload must be rejected"
        );
        assert!(!gate.is_occupied());
    }

    /// A late-returning transcription must not clobber an engine that was
    /// loaded while it was out (e.g. a fresh model loaded after its watchdog
    /// fired).
    #[test]
    fn generation_gate_rejects_a_restore_after_a_new_install() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (stale, generation) = gate.take().expect("installed value must be takeable");
        gate.install(8);
        assert!(
            !gate.try_restore(stale, generation),
            "restoring over a newly installed value must be rejected"
        );
        let (current, _) = gate
            .take()
            .expect("the new value must still be in the slot");
        assert_eq!(current, 8, "the newly installed value must win");
    }
}
