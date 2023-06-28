use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(crate) type Timestamp = usize;
pub(crate) type ThreadId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invocation<Op, Ret> {
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) inv_timestamp: Timestamp,
    pub(crate) ret_timestamp: Timestamp,

    pub(crate) op: Op,
    pub(crate) ret: Ret,
}

pub(crate) type History<Op, Ret> = Vec<Invocation<Op, Ret>>;

pub(crate) struct InternalRecorder<Op, Ret> {
    thread_id: Option<ThreadId>,
    invocations: History<Op, Ret>,
    current_op: Option<Op>,
    inv_timestamp: usize,
}

impl<Op, Ret> InternalRecorder<Op, Ret> {
    pub(crate) fn new(thread_id: Option<ThreadId>) -> Self {
        InternalRecorder {
            thread_id,
            invocations: Vec::new(),
            current_op: None,
            inv_timestamp: 0,
        }
    }

    pub(crate) fn add_invocation(&mut self, op: Op, inv_timestamp: usize) {
        assert!(self.current_op.is_none());
        self.inv_timestamp = inv_timestamp;
        self.current_op = Some(op);
    }

    pub(crate) fn add_completion(&mut self, ret: Ret, ret_timestamp: usize) {
        self.invocations.push(Invocation {
            thread_id: self.thread_id,
            inv_timestamp: self.inv_timestamp,
            ret_timestamp,
            op: self.current_op.take().unwrap(),
            ret,
        })
    }

    pub(crate) fn invocations(self) -> Vec<Invocation<Op, Ret>> {
        // assert!(self.current_op.is_none());
        self.invocations
    }
}

pub trait Recorder {
    type Op;
    type Ret;

    fn record(&mut self, op: Self::Op, f: impl FnOnce() -> Self::Ret);
}

pub fn record_init_part<Op, Ret>() -> InitPartRecorder<Op, Ret> {
    InitPartRecorder {
        internal_recorder: InternalRecorder::new(None),
        timer: 0,
    }
}

pub struct InitPartRecorder<Op, Ret> {
    internal_recorder: InternalRecorder<Op, Ret>,
    timer: Timestamp,
}

impl<Op, Ret> Recorder for InitPartRecorder<Op, Ret> {
    type Op = Op;
    type Ret = Ret;

    fn record(&mut self, op: Op, f: impl FnOnce() -> Ret) {
        self.internal_recorder.add_invocation(op, self.timer);
        self.timer += 1;

        let ret = f();

        self.internal_recorder.add_completion(ret, self.timer);
        self.timer += 1;
    }
}

impl<Op, Ret> InitPartRecorder<Op, Ret> {
    pub fn record_parallel_part(self) -> ParallelPartRecorder<Op, Ret> {
        ParallelPartRecorder {
            init_part: Mutex::new(self.internal_recorder.invocations()),
            invocations: Mutex::new(Vec::new()),
            next_thread_id: AtomicUsize::new(0),
            timer: AtomicUsize::new(self.timer),
        }
    }

    pub fn record_post_part(self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: self.internal_recorder.invocations(),
            parallel_part: Vec::new(),
            internal_builder: InternalRecorder::new(None),
            timer: self.timer,
        }
    }

    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.internal_recorder.invocations(),
            parallel_part: Vec::new(),
            post_part: Vec::new(),
        }
    }
}

pub struct ParallelPartRecorder<Op, Ret> {
    init_part: Mutex<History<Op, Ret>>,
    invocations: Mutex<History<Op, Ret>>,
    next_thread_id: AtomicUsize,
    timer: AtomicUsize,
}

impl<Op, Ret> ParallelPartRecorder<Op, Ret> {
    pub fn record_thread(&self) -> PerThreadRecorder<'_, Op, Ret> {
        let thread_id = self.next_thread_id.load(Ordering::Relaxed);
        self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        PerThreadRecorder {
            internal_recorder: InternalRecorder::new(Some(thread_id)),
            parent_builder: self,
        }
    }

    pub fn record_post_part(&self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: std::mem::replace(&mut self.init_part.lock().unwrap(), Vec::new()),
            parallel_part: std::mem::replace(&mut self.invocations.lock().unwrap(), Vec::new()),
            internal_builder: InternalRecorder::new(None),
            timer: self.timer.load(Ordering::Relaxed),
        }
    }

    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part.into_inner().unwrap(),
            parallel_part: self.invocations.into_inner().unwrap(),
            post_part: Vec::new(),
        }
    }
}

pub struct PerThreadRecorder<'a, Op, Ret> {
    internal_recorder: InternalRecorder<Op, Ret>,
    parent_builder: &'a ParallelPartRecorder<Op, Ret>,
}

impl<'a, Op, Ret> Recorder for PerThreadRecorder<'a, Op, Ret> {
    type Op = Op;
    type Ret = Ret;

    fn record(&mut self, op: Op, f: impl FnOnce() -> Ret) {
        self.internal_recorder
            .add_invocation(op, self.parent_builder.timer.load(Ordering::Relaxed));
        self.parent_builder.timer.fetch_add(1, Ordering::Relaxed);

        let ret = f();

        self.internal_recorder
            .add_completion(ret, self.parent_builder.timer.load(Ordering::Relaxed));
        self.parent_builder.timer.fetch_add(1, Ordering::Relaxed);
    }
}

impl<'a, Op, Ret> Drop for PerThreadRecorder<'a, Op, Ret> {
    fn drop(&mut self) {
        let invocations = std::mem::replace(&mut self.internal_recorder.invocations, Vec::new());

        self.parent_builder
            .invocations
            .lock()
            .unwrap()
            .extend(invocations);
    }
}

pub struct PostPartRecorder<Op, Ret> {
    init_part: History<Op, Ret>,
    parallel_part: History<Op, Ret>,
    internal_builder: InternalRecorder<Op, Ret>,
    timer: usize,
}

impl<Op, Ret> Recorder for PostPartRecorder<Op, Ret> {
    type Op = Op;
    type Ret = Ret;

    fn record(&mut self, op: Op, f: impl FnOnce() -> Ret) {
        self.internal_builder.add_invocation(op, self.timer);
        self.timer += 1;

        let ret = f();

        self.internal_builder.add_completion(ret, self.timer);
        self.timer += 1;
    }
}

impl<Op, Ret> PostPartRecorder<Op, Ret> {
    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part,
            parallel_part: self.parallel_part,
            post_part: self.internal_builder.invocations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution<Op, Ret> {
    pub(crate) init_part: History<Op, Ret>,
    pub(crate) parallel_part: History<Op, Ret>,
    pub(crate) post_part: History<Op, Ret>,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::thread;

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Op {
        A,
        B,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Ret {
        A,
        B,
    }

    #[test]
    fn test_record_init() {
        let mut recorder = record_init_part();

        recorder.record(Op::A, || Ret::A);
        recorder.record(Op::B, || Ret::B);

        let execution = recorder.finish();

        assert_eq!(
            execution.init_part,
            vec![
                Invocation {
                    thread_id: None,
                    inv_timestamp: 0,
                    ret_timestamp: 1,
                    op: Op::A,
                    ret: Ret::A,
                },
                Invocation {
                    thread_id: None,
                    inv_timestamp: 2,
                    ret_timestamp: 3,
                    op: Op::B,
                    ret: Ret::B,
                },
            ]
        );
    }

    #[test]
    fn test_record_parallel_part() {
        let mut recorder = record_init_part();

        // to shift the timer
        recorder.record(Op::A, || Ret::A);

        let recorder = recorder.record_parallel_part();

        let x = AtomicU32::new(0);

        thread::scope(|s| {
            {
                let x = &x;
                let mut recorder = recorder.record_thread();
                s.spawn(move || {
                    recorder.record(Op::A, || {
                        x.store(1, Ordering::Release);
                        while x.load(Ordering::Acquire) != 2 {
                            std::hint::spin_loop();
                        }
                        Ret::A
                    });
                });
            }
            {
                let x = &x;
                let mut recorder = recorder.record_thread();
                s.spawn(move || {
                    while x.load(Ordering::Acquire) != 1 {
                        std::hint::spin_loop();
                    }
                    recorder.record(Op::B, || Ret::B);
                    x.store(2, Ordering::Release);
                });
            }
        });

        let mut execution = recorder.finish();

        // histories are merged in the order of thread joining which is arbitrary
        execution.parallel_part.sort_by_key(|inv| inv.thread_id);

        assert_eq!(
            execution.parallel_part,
            vec![
                Invocation {
                    thread_id: Some(0),
                    inv_timestamp: 2,
                    ret_timestamp: 5,
                    op: Op::A,
                    ret: Ret::A,
                },
                Invocation {
                    thread_id: Some(1),
                    inv_timestamp: 3,
                    ret_timestamp: 4,
                    op: Op::B,
                    ret: Ret::B,
                },
            ]
        )
    }

    #[test]
    fn test_record_post() {
        let mut recorder = record_init_part();

        // to shift the timer
        recorder.record(Op::A, || Ret::A);

        let mut recorder = recorder.record_post_part();

        recorder.record(Op::A, || Ret::A);
        recorder.record(Op::B, || Ret::B);

        let execution = recorder.finish();

        assert_eq!(
            execution.post_part,
            vec![
                Invocation {
                    thread_id: None,
                    inv_timestamp: 2,
                    ret_timestamp: 3,
                    op: Op::A,
                    ret: Ret::A,
                },
                Invocation {
                    thread_id: None,
                    inv_timestamp: 4,
                    ret_timestamp: 5,
                    op: Op::B,
                    ret: Ret::B,
                },
            ]
        );
    }
}
