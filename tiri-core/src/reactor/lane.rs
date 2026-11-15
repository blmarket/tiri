use crate::server_client::Client as _;
use crate::shared::client::{ClientRef, ClientWeak};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{ContextBuilder, LocalWake, LocalWaker, Poll, Waker};

type Job = Pin<Box<dyn Future<Output = ()>>>;

enum DrainSlot {
    Idle,
    Parked {
        job: Job,
        counted: bool,
    },
    Polling {
        from_wake: bool,
        rewoken: bool,
        counted: bool,
    },
}

#[derive(Clone)]
pub enum LaneTarget {
    Global,
    Client(ClientWeak),
}

impl LaneTarget {
    pub fn for_client(owner: Option<&ClientRef>) -> Self {
        owner.map_or(Self::Global, |owner| Self::Client(Rc::downgrade(owner)))
    }

    pub fn try_with<R>(&self, operation: impl FnOnce(&mut Lane) -> R) -> Option<R> {
        thread_local! {
            static GLOBAL_LANE: RefCell<Lane> = RefCell::new(Lane::new());
        }
        match self {
            Self::Global => Some(GLOBAL_LANE.with_borrow_mut(operation)),
            Self::Client(observer) => {
                let owner = observer.upgrade()?;
                let mut lane = owner.borrow_lane_mut();
                Some(operation(&mut lane))
            }
        }
    }

    pub fn is_polling(&self) -> bool {
        self.try_with(|lane| matches!(lane.slot, DrainSlot::Polling { .. }))
            .unwrap_or(false)
    }

    pub fn is_polling_from_wake(&self) -> Option<bool> {
        self.try_with(|lane| match lane.slot {
            DrainSlot::Polling { from_wake, .. } => from_wake,
            _ => false,
        })
    }

    pub fn step(&self) {
        let _ = self.try_with(|lane| {
            lane.ran = lane.ran.wrapping_add(1);
            if let DrainSlot::Polling { counted, .. } | DrainSlot::Parked { counted, .. } =
                &mut lane.slot
            {
                *counted = true;
            }
        });
    }

    pub fn yield_to_drive(&self) -> impl Future<Output = ()> + use<> {
        let target = self.clone();
        std::future::poll_fn(move |_| match target.is_polling_from_wake() {
            Some(true) => Poll::Pending,
            Some(false) => Poll::Ready(()),
            None => Poll::Pending,
        })
    }
}

struct LaneWake(LaneTarget);

impl LocalWake for LaneWake {
    fn wake(self: Rc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Rc<Self>) {
        let taken = self.0.try_with(|lane| {
            match std::mem::replace(
                &mut lane.slot,
                DrainSlot::Polling {
                    from_wake: true,
                    rewoken: false,
                    counted: false,
                },
            ) {
                DrainSlot::Parked { job, counted } => {
                    lane.slot = DrainSlot::Polling {
                        from_wake: true,
                        rewoken: false,
                        counted,
                    };
                    Some(job)
                }
                DrainSlot::Polling {
                    from_wake,
                    counted,
                    ..
                } => {
                    lane.slot = DrainSlot::Polling {
                        from_wake,
                        rewoken: true,
                        counted,
                    };
                    None
                }
                DrainSlot::Idle => {
                    lane.slot = DrainSlot::Idle;
                    None
                }
            }
        });
        if let Some(Some(job)) = taken {
            poll_wake(&self.0, job);
        }
    }
}

fn poll_wake(target: &LaneTarget, mut job: Job) {
    let waker = LocalWaker::from(Rc::new(LaneWake(target.clone())));
    let mut cx = ContextBuilder::from_waker(Waker::noop())
        .local_waker(&waker)
        .build();
    loop {
        match job.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                drop(job);
                let _ = target.try_with(|lane| {
                    let counted = match lane.slot {
                        DrainSlot::Polling { counted, .. } => counted,
                        _ => unreachable!("polled lane stays Polling until poll returns"),
                    };
                    if !counted {
                        lane.ran = lane.ran.wrapping_add(1);
                    }
                    lane.slot = DrainSlot::Idle;
                });
                return;
            }
            Poll::Pending => {
                let state = target.try_with(|lane| match &mut lane.slot {
                    DrainSlot::Polling {
                        rewoken, counted, ..
                    } => {
                        let again = std::mem::replace(rewoken, false);
                        (again, *counted)
                    }
                    _ => unreachable!("polled lane stays Polling until poll returns"),
                });
                let Some((rewoken, counted)) = state else {
                    drop(job);
                    return;
                };
                if !rewoken {
                    let mut parked = Some(job);
                    let _ = target.try_with(|lane| {
                        lane.slot = DrainSlot::Parked {
                            job: parked.take().unwrap(),
                            counted,
                        };
                    });
                    drop(parked);
                    return;
                }
            }
        }
    }
}

pub struct Lane {
    slot: DrainSlot,
    jobs: VecDeque<Job>,
    ran: u32,
}

impl Default for Lane {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Lane {
    pub fn new() -> Self {
        Self {
            slot: DrainSlot::Idle,
            jobs: VecDeque::new(),
            ran: 0,
        }
    }

    pub fn clear(&mut self) {
        let jobs = std::mem::take(&mut self.jobs);
        let slot = std::mem::replace(&mut self.slot, DrainSlot::Idle);
        self.ran = 0;
        drop(slot);
        drop(jobs);
    }

    pub fn push(owner: Option<&ClientRef>, work: impl Future<Output = ()> + 'static) {
        let target = LaneTarget::for_client(owner);
        let mut boxed: Option<Job> = Some(Box::pin(work));
        target
            .try_with(|lane| lane.jobs.push_back(boxed.take().unwrap()))
            .expect("live command lane owner");
    }

    pub fn drive(owner: Option<&ClientRef>) -> u32 {
        let target = LaneTarget::for_client(owner);
        let start = target.try_with(|lane| {
            match std::mem::replace(
                &mut lane.slot,
                DrainSlot::Polling {
                    from_wake: false,
                    rewoken: false,
                    counted: false,
                },
            ) {
                DrainSlot::Polling {
                    from_wake,
                    rewoken,
                    counted,
                } => {
                    lane.slot = DrainSlot::Polling {
                        from_wake,
                        rewoken,
                        counted,
                    };
                    None
                }
                DrainSlot::Parked { job, counted } => {
                    lane.slot = DrainSlot::Polling {
                        from_wake: false,
                        rewoken: false,
                        counted,
                    };
                    Some(Some(job))
                }
                DrainSlot::Idle => match lane.jobs.pop_front() {
                    Some(job) => Some(Some(job)),
                    None => {
                        lane.slot = DrainSlot::Idle;
                        Some(None)
                    }
                },
            }
        });
        let Some(Some(first)) = start else {
            return 0;
        };
        let Some(mut current) = first else {
            return target
                .try_with(|lane| std::mem::take(&mut lane.ran))
                .unwrap_or(0);
        };
        let waker = LocalWaker::from(Rc::new(LaneWake(target.clone())));
        let mut cx = ContextBuilder::from_waker(Waker::noop())
            .local_waker(&waker)
            .build();
        loop {
            match current.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {
                    drop(current);
                    let next = target.try_with(|lane| {
                        let counted = match lane.slot {
                            DrainSlot::Polling { counted, .. } => counted,
                            _ => unreachable!("polled lane stays Polling until poll returns"),
                        };
                        if !counted {
                            lane.ran = lane.ran.wrapping_add(1);
                        }
                        match lane.jobs.pop_front() {
                            Some(job) => {
                                lane.slot = DrainSlot::Polling {
                                    from_wake: false,
                                    rewoken: false,
                                    counted: false,
                                };
                                Some(job)
                            }
                            None => {
                                lane.slot = DrainSlot::Idle;
                                None
                            }
                        }
                    });
                    let Some(next) = next else {
                        return 0;
                    };
                    match next {
                        Some(job) => current = job,
                        None => {
                            return target
                                .try_with(|lane| std::mem::take(&mut lane.ran))
                                .unwrap_or(0);
                        }
                    }
                }
                Poll::Pending => {
                    let state = target.try_with(|lane| match &mut lane.slot {
                        DrainSlot::Polling {
                            rewoken, counted, ..
                        } => {
                            let again = std::mem::replace(rewoken, false);
                            (again, *counted)
                        }
                        _ => unreachable!("polled lane stays Polling until poll returns"),
                    });
                    let Some((rewoken, counted)) = state else {
                        drop(current);
                        return 0;
                    };
                    if !rewoken {
                        let mut parked = Some(current);
                        let _ = target.try_with(|lane| {
                            lane.slot = DrainSlot::Parked {
                                job: parked.take().unwrap(),
                                counted,
                            };
                        });
                        drop(parked);
                        return target
                            .try_with(|lane| std::mem::take(&mut lane.ran))
                            .unwrap_or(0);
                    }
                }
            }
        }
    }
}
