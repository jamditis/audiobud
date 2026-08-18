//! Bounded FIFO coordination for finished transcript delivery.
//!
//! The queue owns only hand-off order. The active worker owns the current text,
//! which lets the Windows output-target backend retry that same item without
//! releasing the next one when target activation lands.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

pub const DEFAULT_DELIVERY_CAPACITY: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueResult {
    /// No worker was active, so the caller must start delivery of this text.
    Start(String),
    /// A worker is active and will take this text in FIFO order.
    Queued,
    /// The bounded queue is full. Return the text instead of dropping it.
    Full(String),
}

#[derive(Default)]
struct QueueState {
    pending: VecDeque<String>,
    in_flight: bool,
}

pub struct DeliveryQueue {
    capacity: usize,
    state: Mutex<QueueState>,
}

impl Default for DeliveryQueue {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_DELIVERY_CAPACITY)
    }
}

impl DeliveryQueue {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(QueueState::default()),
        }
    }

    pub fn enqueue(&self, text: String) -> EnqueueResult {
        let mut state = self.guard();
        let delivery_count = state.pending.len() + usize::from(state.in_flight);

        if delivery_count >= self.capacity {
            return EnqueueResult::Full(text);
        }

        if state.in_flight {
            state.pending.push_back(text);
            EnqueueResult::Queued
        } else {
            state.in_flight = true;
            EnqueueResult::Start(text)
        }
    }

    /// Finish the active item and transfer its worker to the next queued item.
    /// `None` means the worker is released and a future enqueue must start one.
    pub fn finish_and_take_next(&self) -> Option<String> {
        let mut state = self.guard();
        match state.pending.pop_front() {
            Some(next) => Some(next),
            None => {
                state.in_flight = false;
                None
            }
        }
    }

    pub fn is_idle(&self) -> bool {
        let state = self.guard();
        !state.in_flight && state.pending.is_empty()
    }

    fn guard(&self) -> MutexGuard<'_, QueueState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{DeliveryQueue, EnqueueResult};

    #[test]
    fn second_delivery_waits_until_first_finishes() {
        let queue = DeliveryQueue::with_capacity(3);

        assert_eq!(
            queue.enqueue("first".to_owned()),
            EnqueueResult::Start("first".to_owned())
        );
        assert_eq!(queue.enqueue("second".to_owned()), EnqueueResult::Queued);

        assert_eq!(queue.finish_and_take_next(), Some("second".to_owned()));
        assert_eq!(queue.finish_and_take_next(), None);
    }

    #[test]
    fn capacity_counts_the_active_delivery_and_returns_overflow_text() {
        let queue = DeliveryQueue::with_capacity(2);

        assert!(matches!(
            queue.enqueue("first".to_owned()),
            EnqueueResult::Start(_)
        ));
        assert_eq!(queue.enqueue("second".to_owned()), EnqueueResult::Queued);
        assert_eq!(
            queue.enqueue("recover me".to_owned()),
            EnqueueResult::Full("recover me".to_owned())
        );

        assert_eq!(queue.finish_and_take_next(), Some("second".to_owned()));
        assert_eq!(queue.finish_and_take_next(), None);
    }

    #[test]
    fn drained_queue_starts_a_new_delivery_worker() {
        let queue = DeliveryQueue::with_capacity(2);

        assert!(matches!(
            queue.enqueue("first".to_owned()),
            EnqueueResult::Start(_)
        ));
        assert_eq!(queue.finish_and_take_next(), None);
        assert_eq!(
            queue.enqueue("later".to_owned()),
            EnqueueResult::Start("later".to_owned())
        );
    }

    #[test]
    fn idle_state_tracks_delivery_lifecycle() {
        let queue = DeliveryQueue::with_capacity(2);

        assert!(queue.is_idle());
        assert!(matches!(
            queue.enqueue("first".to_owned()),
            EnqueueResult::Start(_)
        ));
        assert!(!queue.is_idle());
        assert_eq!(queue.finish_and_take_next(), None);
        assert!(queue.is_idle());
    }
}
