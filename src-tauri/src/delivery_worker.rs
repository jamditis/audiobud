//! The thread finished transcripts are delivered on.
//!
//! Delivering a transcript blocks for a long time by design: `paste_delay_ms`,
//! the 100 ms keystroke hold in [`crate::input`], the 50 ms clipboard-restore
//! wait, the auto-submit gap, and -- for a pinned target -- a foreground switch,
//! the activation settle, and the hand-back afterwards (#161). Running that on
//! Tauri's main thread stalls every window the app owns, so the overlay freezes
//! for exactly as long as the paste takes.
//!
//! So deliveries run here instead, on one long-lived thread of their own:
//!
//! * **Order.** Jobs run one at a time, in the order they were submitted. The
//!   [`crate::delivery_queue`] already hands them over one at a time, so with a
//!   single worker no two deliveries ever contend for the `EnigoState` mutex --
//!   which matters because `std::sync::Mutex` is not fair, so two deliveries
//!   racing for it could otherwise be typed out in either order (#122).
//! * **Thread affinity.** Every delivery uses the same thread, so anything with
//!   thread-local state stays consistent: enigo's Linux X11 connection, and the
//!   `AttachThreadInput`/`SetForegroundWindow` pair in
//!   [`crate::output_target::backend`], which attaches *the calling thread's*
//!   input queue and detaches it again within the same call. Neither needs the
//!   main thread specifically; both want a thread that does not change under
//!   them.
//! * **Survival.** A panicking delivery is caught, so one bad transcript cannot
//!   take the worker down and leave every later transcript undeliverable.

use log::{error, warn};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long [`DeliveryWorker::shutdown`] waits for the worker thread before
/// giving up on joining it. A real paste is short (milliseconds to low
/// hundreds), so this is generous for the normal case; it is bounded at all
/// so shutdown can never deadlock against work the worker thread itself needs
/// the caller's thread to finish (#161 review round 2, finding 1).
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// How often [`DeliveryWorker::shutdown`] polls [`JoinHandle::is_finished`]
/// while waiting out [`SHUTDOWN_TIMEOUT`].
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// One unit of delivery work: the paste, plus whatever hand-off follows it.
pub type DeliveryJob = Box<dyn FnOnce() + Send + 'static>;

/// Owns the delivery thread and the channel that feeds it.
pub struct DeliveryWorker {
    /// `None` when the thread could not be spawned, which makes [`run`] fall
    /// back to the calling thread. Also taken (left `None`) by [`shutdown`],
    /// which is how the worker thread's receive loop is told to end.
    ///
    /// [`run`]: DeliveryWorker::run
    /// [`shutdown`]: DeliveryWorker::shutdown
    jobs: Mutex<Option<Sender<DeliveryJob>>>,
    /// Joined by [`shutdown`] so quit can wait for whatever delivery is
    /// already running or queued instead of killing the process mid-paste.
    ///
    /// [`shutdown`]: DeliveryWorker::shutdown
    handle: Mutex<Option<JoinHandle<()>>>,
    /// Set by [`shutdown`] before it touches `jobs`, so a [`run`] racing it --
    /// between a caller's `DeliveryQueue::enqueue` returning `Start` and the
    /// `run` call that follows -- can tell shutdown is already underway even
    /// if it observes `jobs` as `None` (#161 review round 4, finding 1). See
    /// [`run`] for why that distinction matters.
    ///
    /// [`run`]: DeliveryWorker::run
    /// [`shutdown`]: DeliveryWorker::shutdown
    shutting_down: AtomicBool,
}

impl Default for DeliveryWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl DeliveryWorker {
    pub fn new() -> Self {
        let (jobs, receiver) = mpsc::channel::<DeliveryJob>();

        let spawned = thread::Builder::new()
            .name("audiobud-delivery".to_string())
            .spawn(move || {
                for job in receiver {
                    // A delivery that panics must not cost every later
                    // transcript its delivery thread. The job's own drop guard
                    // still advances the queue while the panic unwinds.
                    if catch_unwind(AssertUnwindSafe(job)).is_err() {
                        error!("A transcript delivery panicked; the delivery thread continues");
                    }
                }
            });

        match spawned {
            Ok(handle) => Self {
                jobs: Mutex::new(Some(jobs)),
                handle: Mutex::new(Some(handle)),
                shutting_down: AtomicBool::new(false),
            },
            Err(e) => {
                // Without a thread the caller's own thread does the work. It is
                // the transcription thread, never the main thread, so the
                // overlay still moves; only the ordering across overlapping
                // deliveries falls back to the queue's own hand-off.
                error!("Failed to spawn the delivery thread: {}", e);
                Self {
                    jobs: Mutex::new(None),
                    handle: Mutex::new(None),
                    shutting_down: AtomicBool::new(false),
                }
            }
        }
    }

    /// Run `job` on the delivery thread, or on this thread if there is none.
    ///
    /// A transcript is never dropped for want of a worker: the fallback costs
    /// the caller the delivery's blocking time, which is what the old
    /// main-thread dispatch cost every time -- unless [`shutdown`] is already
    /// underway, in which case the fallback is refused instead. Once
    /// `app.exit` is in flight the process can end at any moment, and an
    /// inline paste on some other thread has no guarantee of finishing before
    /// it does; the acceptable outcome for a delivery that loses this race is
    /// "refused and logged", not "silently truncated mid-keystroke" (#161
    /// review round 4, finding 1).
    ///
    /// [`shutdown`]: DeliveryWorker::shutdown
    pub fn run(&self, job: DeliveryJob) {
        let jobs = self.jobs.lock().unwrap();
        let Some(sender) = jobs.as_ref() else {
            drop(jobs);
            if self.shutting_down.load(Ordering::SeqCst) {
                error!(
                    "Delivery worker is shutting down; refusing to deliver on the calling \
                     thread to avoid a paste truncated by the process exiting mid-delivery"
                );
                return;
            }
            job();
            return;
        };

        if let Err(returned) = sender.send(job) {
            drop(jobs);
            if self.shutting_down.load(Ordering::SeqCst) {
                error!(
                    "Delivery worker is shutting down; refusing to deliver on the calling \
                     thread to avoid a paste truncated by the process exiting mid-delivery"
                );
                return;
            }
            warn!("The delivery thread is gone; delivering on the calling thread");
            (returned.0)();
        }
    }

    /// Drain and stop the delivery thread before the process exits (quit).
    ///
    /// Closing the channel lets the worker's `for job in receiver` loop finish
    /// whatever is already running or queued and then end on its own, so a
    /// transcript that is mid-paste when the user quits is still delivered
    /// instead of being cut off by `app.exit` (#161 review, finding 1).
    ///
    /// The wait for that is bounded and non-blocking (polled via
    /// [`JoinHandle::is_finished`]), never a plain `join()`, on purpose: this
    /// runs on the tray callback, which is the platform event-loop thread.
    /// A delivery that loses its target window mid-paste calls back into
    /// [`crate::tray::update_tray_menu`] to clear the stale lock indicator
    /// (`output_target::backend::announce_lock_lost`), and native menu work
    /// needs that same event-loop thread. A plain `join()` here would then
    /// wait on the worker thread while the worker thread waits on this one --
    /// a deadlock that would hang Quit outright (#161 review round 2, finding
    /// 1). Giving up after [`SHUTDOWN_TIMEOUT`] and letting the caller exit
    /// anyway is safe: the process is about to go away either way, and a
    /// stuck delivery blocking real work indefinitely is worse than losing
    /// the last one on the way out.
    ///
    /// Safe to call more than once and safe when no thread was ever spawned.
    pub fn shutdown(&self) {
        // Set before touching `jobs`, so a `run()` that is concurrently
        // between `DeliveryQueue::enqueue` returning `Start` and its own call
        // to `run` -- and so has not yet reached the `self.jobs.lock()` below
        // -- can still tell shutdown has started once it does, even though it
        // will find `jobs` already `None` either way (#161 review round 4,
        // finding 1).
        self.shutting_down.store(true, Ordering::SeqCst);

        // Drop the sender so the worker's receiver iterator ends once it has
        // drained whatever was already sent.
        self.jobs.lock().unwrap().take();

        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            let mut handle_slot = self.handle.lock().unwrap();
            match handle_slot.as_ref() {
                None => return,
                Some(handle) if handle.is_finished() => {
                    // `is_finished()` is true, so this join cannot block.
                    let handle = handle_slot.take().unwrap();
                    drop(handle_slot);
                    if handle.join().is_err() {
                        error!("The delivery thread panicked while shutting down");
                    }
                    return;
                }
                Some(_) => {
                    drop(handle_slot);
                    if Instant::now() >= deadline {
                        warn!(
                            "Delivery thread did not finish within {:?}; exiting without waiting further",
                            SHUTDOWN_TIMEOUT
                        );
                        return;
                    }
                    thread::sleep(SHUTDOWN_POLL_INTERVAL);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeliveryWorker, SHUTDOWN_TIMEOUT};
    use crate::delivery_queue::{DeliveryQueue, EnqueueResult};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread::ThreadId;
    use std::time::{Duration, Instant};

    fn wait_until(mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if done() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the delivery worker did not finish in time");
    }

    #[test]
    fn jobs_run_in_submission_order_even_when_the_first_is_slow() {
        // The ordering guarantee the paste path depends on: a slow delivery
        // holds the thread, and the transcript submitted behind it waits its
        // turn rather than overtaking it.
        let worker = DeliveryWorker::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let slow = Arc::clone(&order);
        worker.run(Box::new(move || {
            std::thread::sleep(Duration::from_millis(80));
            slow.lock().unwrap().push("first");
        }));

        let quick = Arc::clone(&order);
        worker.run(Box::new(move || {
            quick.lock().unwrap().push("second");
        }));

        wait_until(|| order.lock().unwrap().len() == 2);
        assert_eq!(*order.lock().unwrap(), vec!["first", "second"]);
    }

    #[test]
    fn every_delivery_runs_off_the_submitting_thread_and_on_the_same_one() {
        // Off the caller's thread is the whole point (#161); on one unchanging
        // thread is what enigo's X11 connection and the Windows
        // AttachThreadInput pair need.
        let worker = DeliveryWorker::new();
        let threads: Arc<Mutex<Vec<ThreadId>>> = Arc::new(Mutex::new(Vec::new()));

        for _ in 0..3 {
            let seen = Arc::clone(&threads);
            worker.run(Box::new(move || {
                seen.lock().unwrap().push(std::thread::current().id());
            }));
        }

        wait_until(|| threads.lock().unwrap().len() == 3);
        let seen = threads.lock().unwrap();
        assert_ne!(seen[0], std::thread::current().id());
        assert!(seen.iter().all(|id| *id == seen[0]));
    }

    #[test]
    fn a_panicking_delivery_does_not_strand_the_next_one() {
        let worker = DeliveryWorker::new();
        let (sender, receiver) = mpsc::channel();

        worker.run(Box::new(|| panic!("the paste exploded")));
        worker.run(Box::new(move || {
            let _ = sender.send("delivered anyway");
        }));

        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)),
            Ok("delivered anyway")
        );
    }

    #[test]
    fn shutdown_waits_for_a_running_delivery_before_returning() {
        // The quit path (#161 review, finding 1): shutdown must not return --
        // and so must not let `app.exit` run -- until a delivery already in
        // flight has actually finished.
        let worker = DeliveryWorker::new();
        let finished = Arc::new(Mutex::new(false));

        let flag = Arc::clone(&finished);
        worker.run(Box::new(move || {
            std::thread::sleep(Duration::from_millis(80));
            *flag.lock().unwrap() = true;
        }));

        worker.shutdown();
        assert!(
            *finished.lock().unwrap(),
            "shutdown returned before the in-flight delivery finished"
        );
    }

    #[test]
    fn shutdown_drains_jobs_queued_behind_a_running_delivery() {
        let worker = DeliveryWorker::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let first = Arc::clone(&order);
        worker.run(Box::new(move || {
            std::thread::sleep(Duration::from_millis(50));
            first.lock().unwrap().push("first");
        }));
        let second = Arc::clone(&order);
        worker.run(Box::new(move || {
            second.lock().unwrap().push("second");
        }));

        worker.shutdown();
        assert_eq!(*order.lock().unwrap(), vec!["first", "second"]);
    }

    #[test]
    fn shutdown_is_a_harmless_no_op_when_called_twice() {
        let worker = DeliveryWorker::new();
        worker.run(Box::new(|| {}));
        worker.shutdown();
        worker.shutdown();
    }

    #[test]
    fn shutdown_gives_up_after_the_timeout_instead_of_blocking_forever() {
        // The deadlock this guards against (#161 review round 2, finding 1):
        // shutdown runs on the same thread a stuck delivery might be waiting
        // on. A plain `join()` would hang Quit outright; the bounded wait
        // must return control to the caller regardless.
        let worker = DeliveryWorker::new();
        let (_keep_stuck, stay_stuck) = mpsc::channel::<()>();

        worker.run(Box::new(move || {
            // Never finishes on its own; only dropping `_keep_stuck` (which
            // outlives this test) would end it, standing in for a delivery
            // that never returns.
            let _ = stay_stuck.recv();
        }));

        let started = Instant::now();
        worker.shutdown();
        let elapsed = started.elapsed();

        assert!(
            elapsed < SHUTDOWN_TIMEOUT + Duration::from_secs(1),
            "shutdown took {:?}, longer than the bounded wait allows",
            elapsed
        );
    }

    #[test]
    fn run_refuses_to_deliver_inline_once_shutdown_has_started() {
        // Regression for #161 review round 4, finding 1: a `run()` that
        // still reaches this worker after `shutdown()` -- the race between
        // `DeliveryQueue::enqueue` returning `Start` and the `run` call that
        // follows -- must not fall back to delivering inline. The process
        // may already be mid-`app.exit`, and an inline paste on whatever
        // thread called `run` has no guarantee of finishing before it does;
        // refusing is the sound outcome, not a truncated paste.
        let worker = DeliveryWorker::new();
        worker.shutdown();

        let ran = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&ran);
        worker.run(Box::new(move || {
            *flag.lock().unwrap() = true;
        }));
        assert!(
            !*ran.lock().unwrap(),
            "run() must refuse to deliver inline once shutdown has started"
        );
    }

    #[test]
    fn run_still_falls_back_inline_when_the_worker_thread_never_spawned() {
        // Distinct from the shutdown case above: a worker that never got a
        // thread at all (`DeliveryWorker::new`'s `Err` arm, simulated here
        // via `shutting_down` staying false with `jobs` empty) is not
        // shutting down -- it never started -- so `run` must still deliver
        // inline the way it always has, rather than refusing.
        let worker = DeliveryWorker {
            jobs: Mutex::new(None),
            handle: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
        };

        let ran = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&ran);
        let this_thread = std::thread::current().id();
        worker.run(Box::new(move || {
            assert_eq!(std::thread::current().id(), this_thread);
            *flag.lock().unwrap() = true;
        }));
        assert!(*ran.lock().unwrap());
    }

    #[test]
    fn overlapping_dictations_are_delivered_in_the_order_they_finished() {
        // The whole hand-off, as the paste path runs it: the queue starts the
        // first delivery, a second and third arrive while it is still typing,
        // and each finished delivery hands the worker to the next. Nothing here
        // may overtake anything else, whatever the OS scheduler prefers -- the
        // guarantee that makes serialized dictation (#122) possible.
        let worker = Arc::new(DeliveryWorker::new());
        let queue: Arc<DeliveryQueue<&'static str>> = Arc::new(DeliveryQueue::with_capacity(8));
        let delivered = Arc::new(Mutex::new(Vec::new()));

        fn deliver(
            worker: Arc<DeliveryWorker>,
            queue: Arc<DeliveryQueue<&'static str>>,
            delivered: Arc<Mutex<Vec<&'static str>>>,
            text: &'static str,
        ) {
            let run_worker = Arc::clone(&worker);
            run_worker.run(Box::new(move || {
                // The first transcript is the slowest to type, so a later one
                // that ran concurrently would land ahead of it.
                if text == "first" {
                    std::thread::sleep(Duration::from_millis(80));
                }
                delivered.lock().unwrap().push(text);
                if let Some(next) = queue.finish_and_take_next() {
                    deliver(worker, queue, delivered, next);
                }
            }));
        }

        for text in ["first", "second", "third"] {
            match queue.enqueue(text) {
                EnqueueResult::Start(text) => deliver(
                    Arc::clone(&worker),
                    Arc::clone(&queue),
                    Arc::clone(&delivered),
                    text,
                ),
                EnqueueResult::Queued => {}
                EnqueueResult::Full(text) => panic!("the queue refused {}", text),
            }
        }

        wait_until(|| queue.is_idle());
        assert_eq!(*delivered.lock().unwrap(), vec!["first", "second", "third"]);
    }
}
