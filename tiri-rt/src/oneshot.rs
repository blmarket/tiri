//! Single-use local value handoff between two independently owned endpoints.
//!
//! The receiver is a future driven by the runtime's local wakers. Ordinary
//! Waker notifications are inert. Either endpoint may be dropped first: a
//! dropped sender resolves the receiver to `None`, and a dropped receiver
//! makes [`Sender::is_closed`] true so owners can prune dead waiters.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, LocalWaker, Poll};

struct Slot<T> {
    value: Cell<Option<T>>,
    waker: RefCell<Option<LocalWaker>>,
    closed: Cell<bool>,
}

/// Create a connected sender and receiver.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let slot = Rc::new(Slot {
        value: Cell::new(None),
        waker: RefCell::new(None),
        closed: Cell::new(false),
    });
    (Sender { slot: slot.clone() }, Receiver { slot })
}

/// Delivers at most one value. Dropping it unsent resolves the receiver to
/// `None`.
pub struct Sender<T> {
    slot: Rc<Slot<T>>,
}

impl<T> Sender<T> {
    /// Deliver the value and wake the receiver, or return it if the receiver
    /// has been dropped.
    pub fn send(self, value: T) -> Result<(), T> {
        if self.slot.closed.get() {
            return Err(value);
        }
        self.slot.value.set(Some(value));
        // Drop closes the slot and wakes the receiver.
        Ok(())
    }

    /// Whether the receiver has been dropped, so a send would be discarded.
    pub fn is_closed(&self) -> bool {
        self.slot.closed.get()
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.slot.closed.set(true);
        // Release the slot borrow before waking; the wake may reenter.
        let waker = self.slot.waker.take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// Resolves to the sent value, or `None` once the sender is dropped unsent.
/// Dropping it closes the channel.
pub struct Receiver<T> {
    slot: Rc<Slot<T>>,
}

impl<T> Future for Receiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        if let Some(value) = self.slot.value.take() {
            return Poll::Ready(Some(value));
        }
        if self.slot.closed.get() {
            return Poll::Ready(None);
        }
        // Drop the previous waker after releasing the borrow.
        let previous = self.slot.waker.replace(Some(cx.local_waker().clone()));
        drop(previous);
        Poll::Pending
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.slot.closed.set(true);
        // A registered waker must not outlive its waiter.
        let waker = self.slot.waker.take();
        drop(waker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{ContextBuilder, LocalWake, Waker};

    #[derive(Default)]
    struct Count(Cell<usize>);
    impl LocalWake for Count {
        fn wake(self: Rc<Self>) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn poll_with<T>(receiver: &mut Receiver<T>, waker: &LocalWaker) -> Poll<Option<T>> {
        let mut context = ContextBuilder::from_waker(Waker::noop())
            .local_waker(waker)
            .build();
        Pin::new(receiver).poll(&mut context)
    }

    #[test]
    fn send_before_poll_resolves_immediately() {
        let count = Rc::new(Count::default());
        let waker = LocalWaker::from(count.clone());
        let (sender, mut receiver) = channel();
        assert_eq!(sender.send(7), Ok(()));
        assert_eq!(poll_with(&mut receiver, &waker), Poll::Ready(Some(7)));
        assert_eq!(count.0.get(), 0);
    }

    #[test]
    fn send_wakes_registered_waker_after_releasing_the_borrow() {
        struct Reenter {
            receiver: RefCell<Option<Receiver<u32>>>,
            observed: Cell<Option<Poll<Option<u32>>>>,
        }
        impl LocalWake for Reenter {
            fn wake(self: Rc<Self>) {
                // Polling and dropping the receiver both access the slot, so
                // they would panic if the sender still held its borrow.
                let mut receiver = self.receiver.take().unwrap();
                let waker = LocalWaker::from(Rc::new(Count::default()));
                self.observed.set(Some(poll_with(&mut receiver, &waker)));
                drop(receiver);
            }
        }

        let (sender, receiver) = channel();
        let reenter = Rc::new(Reenter {
            receiver: RefCell::new(Some(receiver)),
            observed: Cell::new(None),
        });
        let waker = LocalWaker::from(reenter.clone());
        let pending = poll_with(reenter.receiver.borrow_mut().as_mut().unwrap(), &waker);
        assert_eq!(pending, Poll::Pending);
        drop(waker);
        assert_eq!(reenter.observed.get(), None);
        assert_eq!(sender.send(7), Ok(()));
        assert_eq!(reenter.observed.get(), Some(Poll::Ready(Some(7))));
        assert_eq!(Rc::strong_count(&reenter), 1);
    }

    #[test]
    fn sender_drop_resolves_none() {
        let count = Rc::new(Count::default());
        let waker = LocalWaker::from(count.clone());
        let (sender, mut receiver) = channel::<u32>();
        assert_eq!(poll_with(&mut receiver, &waker), Poll::Pending);
        drop(sender);
        assert_eq!(count.0.get(), 1);
        assert_eq!(poll_with(&mut receiver, &waker), Poll::Ready(None));

        let (sender, mut receiver) = channel::<u32>();
        drop(sender);
        assert_eq!(poll_with(&mut receiver, &waker), Poll::Ready(None));
        assert_eq!(count.0.get(), 1);
    }

    #[test]
    fn receiver_drop_closes_sender() {
        let count = Rc::new(Count::default());
        let waker = LocalWaker::from(count.clone());
        let (sender, mut receiver) = channel::<u32>();
        assert!(!sender.is_closed());
        assert_eq!(poll_with(&mut receiver, &waker), Poll::Pending);
        assert_eq!(Rc::strong_count(&count), 3);
        drop(receiver);
        assert!(sender.is_closed());
        assert_eq!(Rc::strong_count(&count), 2);
        drop(sender);
        assert_eq!(count.0.get(), 0);
    }

    #[test]
    fn send_after_receiver_drop_returns_value() {
        let (sender, receiver) = channel();
        drop(receiver);
        assert_eq!(sender.send(3), Err(3));
    }
}
