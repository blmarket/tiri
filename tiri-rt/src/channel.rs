use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{LocalWaker, Poll};

struct State<T> {
    values: RefCell<VecDeque<T>>,
    senders: Cell<usize>,
    receiving: Cell<bool>,
    waker: RefCell<Option<LocalWaker>>,
}

impl<T> State<T> {
    fn wake(&self) {
        let waker = self.waker.take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let state = Rc::new(State {
        values: RefCell::new(VecDeque::new()),
        senders: Cell::new(1),
        receiving: Cell::new(true),
        waker: RefCell::new(None),
    });
    (
        Sender {
            state: state.clone(),
        },
        Receiver { state },
    )
}

pub struct Sender<T> {
    state: Rc<State<T>>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.state.senders.set(
            self.state
                .senders
                .get()
                .checked_add(1)
                .expect("channel sender count"),
        );
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        if !self.state.receiving.get() {
            return Err(value);
        }
        self.state.values.borrow_mut().push_back(value);
        self.state.wake();
        Ok(())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let senders = self.state.senders.get() - 1;
        self.state.senders.set(senders);
        if senders == 0 {
            self.state.wake();
        }
    }
}

pub struct Receiver<T> {
    state: Rc<State<T>>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| {
            if let Some(value) = self.state.values.borrow_mut().pop_front() {
                return Poll::Ready(Some(value));
            }
            if self.state.senders.get() == 0 {
                return Poll::Ready(None);
            }
            self.state.waker.replace(Some(cx.local_waker().clone()));
            Poll::Pending
        })
        .await
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.state.receiving.set(false);
        self.state.waker.take();
        let values = self.state.values.take();
        drop(values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{ContextBuilder, LocalWake, Waker};

    #[derive(Default)]
    struct WakeCount(Cell<usize>);

    impl LocalWake for WakeCount {
        fn wake(self: Rc<Self>) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn events_are_ordered_and_drained_before_sender_shutdown() {
        let (sender, mut receiver) = channel();
        let other = sender.clone();
        sender.send(1).unwrap();
        other.send(2).unwrap();
        drop(sender);
        drop(other);
        let mut cx = ContextBuilder::from_waker(Waker::noop()).build();
        assert_eq!(pin!(receiver.recv()).poll(&mut cx), Poll::Ready(Some(1)));
        assert_eq!(pin!(receiver.recv()).poll(&mut cx), Poll::Ready(Some(2)));
        assert_eq!(pin!(receiver.recv()).poll(&mut cx), Poll::Ready(None));
    }

    #[test]
    fn sending_allows_the_consumer_to_receive_and_drop_during_the_wake() {
        struct Consumer {
            receiver: RefCell<Option<Receiver<i32>>>,
            value: Cell<Option<i32>>,
        }
        impl LocalWake for Consumer {
            fn wake(self: Rc<Self>) {
                let mut receiver = self.receiver.take().unwrap();
                let mut cx = ContextBuilder::from_waker(Waker::noop()).build();
                let Poll::Ready(value) = pin!(receiver.recv()).poll(&mut cx) else {
                    panic!("queued event");
                };
                self.value.set(value);
            }
        }
        let (sender, mut receiver) = channel();
        let consumer = Rc::new(Consumer {
            receiver: RefCell::new(None),
            value: Cell::new(None),
        });
        let waker = LocalWaker::from(consumer.clone());
        let mut cx = ContextBuilder::from_waker(Waker::noop())
            .local_waker(&waker)
            .build();
        assert!(pin!(receiver.recv()).poll(&mut cx).is_pending());
        consumer.receiver.replace(Some(receiver));
        sender.send(7).unwrap();
        assert_eq!(consumer.value.get(), Some(7));
        assert_eq!(sender.send(9), Err(9));
    }

    #[test]
    fn cancelling_a_receive_preserves_events_and_receiver_drop_rejects_sends() {
        let (sender, mut receiver) = channel();
        let count = Rc::new(WakeCount::default());
        let waker = LocalWaker::from(count.clone());
        let mut cx = ContextBuilder::from_waker(Waker::noop())
            .local_waker(&waker)
            .build();
        assert!(pin!(receiver.recv()).poll(&mut cx).is_pending());
        sender.send(7).unwrap();
        assert_eq!(count.0.get(), 1);
        assert_eq!(pin!(receiver.recv()).poll(&mut cx), Poll::Ready(Some(7)));
        assert!(pin!(receiver.recv()).poll(&mut cx).is_pending());
        drop(sender);
        assert_eq!(count.0.get(), 2);
        assert_eq!(pin!(receiver.recv()).poll(&mut cx), Poll::Ready(None));
        let (sender, receiver) = channel();
        drop(receiver);
        assert_eq!(sender.send(9), Err(9));
    }
}
