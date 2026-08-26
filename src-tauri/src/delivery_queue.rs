//! Bounded FIFO coordination for finished transcript delivery.
//!
//! The queue owns only hand-off order. The active worker owns the current item,
//! which lets the Windows output-target backend retry that same item without
//! releasing the next one when target activation lands.
//!
//! An item is a [`TranscriptDelivery`]: the text plus the
//! [`DictationContext`] captured when that dictation started. The context has to
//! travel with the text rather than be looked up at delivery time, because
//! queued transcripts are pasted after later dictations have already changed the
//! live state they would otherwise be resolved from (#160). The queue is generic
//! over its item so its own tests can exercise ordering with plain strings.

use crate::dictation_context::DictationContext;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

pub const DEFAULT_DELIVERY_CAPACITY: usize = 8;

/// One finished transcript and the dictation intent it must be delivered under.
pub struct TranscriptDelivery {
    pub text: String,
    pub context: DictationContext,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueResult<T> {
    /// No worker was active, so the caller must start delivery of this item.
    Start(T),
    /// A worker is active and will take this item in FIFO order.
    Queued,
    /// The bounded queue is full. Return the item instead of dropping it.
    Full(T),
}

struct QueueState<T> {
    pending: VecDeque<T>,
    in_flight: bool,
}

// Derived Default would demand `T: Default`, which a delivery is not.
impl<T> Default for QueueState<T> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            in_flight: false,
        }
    }
}

pub struct DeliveryQueue<T = TranscriptDelivery> {
    capacity: usize,
    state: Mutex<QueueState<T>>,
}

impl<T> Default for DeliveryQueue<T> {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_DELIVERY_CAPACITY)
    }
}

impl<T> DeliveryQueue<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(QueueState::default()),
        }
    }

    pub fn enqueue(&self, text: T) -> EnqueueResult<T> {
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
    pub fn finish_and_take_next(&self) -> Option<T> {
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

    fn guard(&self) -> MutexGuard<'_, QueueState<T>> {
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
