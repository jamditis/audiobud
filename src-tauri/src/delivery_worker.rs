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
use std::sync::mpsc::{self, Sender};
use std::thread;

/// One unit of delivery work: the paste, plus whatever hand-off follows it.
pub type DeliveryJob = Box<dyn FnOnce() + Send + 'static>;

/// Owns the delivery thread and the channel that feeds it.
pub struct DeliveryWorker {
    /// `None` when the thread could not be spawned, which makes [`run`] fall
    /// back to the calling thread.
    ///
    /// [`run`]: DeliveryWorker::run
    jobs: Option<Sender<DeliveryJob>>,
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
            Ok(_handle) => Self { jobs: Some(jobs) },
            Err(e) => {
                // Without a thread the caller's own thread does the work. It is
                // the transcription thread, never the main thread, so the
                // overlay still moves; only the ordering across overlapping
                // deliveries falls back to the queue's own hand-off.
                error!("Failed to spawn the delivery thread: {}", e);
                Self { jobs: None }
            }
        }
    }

    /// Run `job` on the delivery thread, or on this thread if there is none.
    ///
    /// A transcript is never dropped for want of a worker: the fallback costs
    /// the caller the delivery's blocking time, which is what the old
    /// main-thread dispatch cost every time.
    pub fn run(&self, job: DeliveryJob) {
        let Some(jobs) = self.jobs.as_ref() else {
            job();
            return;
        };

        if let Err(returned) = jobs.send(job) {
            warn!("The delivery thread is gone; delivering on the calling thread");
            (returned.0)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DeliveryWorker;
    use crate::delivery_queue::{DeliveryQueue, EnqueueResult};
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
