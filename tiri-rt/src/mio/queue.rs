use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll};

use crate::Handle as _;

use super::runtime::{Handle, Task};

type Job = Pin<Box<dyn Future<Output = ()>>>;

struct QueueState {
    handle: Handle,
    jobs: RefCell<VecDeque<Job>>,
    task: RefCell<Option<Task>>,
    active: Cell<bool>,
}

/// Owns a sequential queue of local futures. Dropping it cancels queued or
/// waiting work immediately; an executing future is released when its current
/// poll returns.
#[must_use = "dropping the queue cancels its futures"]
pub struct Queue {
    state: Rc<QueueState>,
}

impl Queue {
    pub(super) fn new(handle: Handle) -> io::Result<Self> {
        handle.core.check()?;
        Ok(Self {
            state: Rc::new(QueueState {
                handle,
                jobs: RefCell::new(VecDeque::new()),
                task: RefCell::new(None),
                active: Cell::new(false),
            }),
        })
    }
}

impl crate::Queue for Queue {
    fn append<F>(&self, future: F) -> io::Result<()>
    where
        F: Future<Output = ()> + 'static,
    {
        self.state.handle.core.check()?;
        if !self.state.active.get() {
            let drain = Drain {
                state: Rc::downgrade(&self.state),
                current: None,
            };
            let task = self.state.handle.spawn(drain)?;
            let previous = self.state.task.replace(Some(task));
            drop(previous);
            self.state.active.set(true);
        }
        self.state.jobs.borrow_mut().push_back(Box::pin(future));
        Ok(())
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        let jobs = std::mem::take(&mut *self.state.jobs.borrow_mut());
        let task = self.state.task.borrow_mut().take();
        drop(task);
        drop(jobs);
    }
}

struct Drain {
    state: Weak<QueueState>,
    current: Option<Job>,
}

impl Future for Drain {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        loop {
            let Some(state) = this.state.upgrade() else {
                this.current = None;
                return Poll::Ready(());
            };
            if this.current.is_none() {
                this.current = state.jobs.borrow_mut().pop_front();
                if this.current.is_none() {
                    state.active.set(false);
                    let finished = state.task.borrow_mut().take();
                    drop(state);
                    drop(finished);
                    return Poll::Ready(());
                }
            }
            drop(state);
            if this
                .current
                .as_mut()
                .unwrap()
                .as_mut()
                .poll(cx)
                .is_pending()
            {
                if this.state.strong_count() == 0 {
                    this.current = None;
                    return Poll::Ready(());
                }
                return Poll::Pending;
            }
            this.current = None;
        }
    }
}
