use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(crate) type Timestamp = usize;
pub(crate) type ThreadId = usize;
pub(crate) type InvocationId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invocation<Op, Ret> {
    pub(crate) op: Op,
    pub(crate) ret: Ret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParallelInvocation<Op, Ret> {
    pub(crate) thread_id: ThreadId,
    pub(crate) call_timestamp: Timestamp,
    pub(crate) return_timestamp: Timestamp,

    pub(crate) op: Op,
    pub(crate) ret: Ret,
}

pub(crate) type History<Op, Ret> = Vec<Invocation<Op, Ret>>;
pub(crate) type ParallelHistory<Op, Ret> = Vec<ParallelInvocation<Op, Ret>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution<Op, Ret> {
    pub(crate) init_part: History<Op, Ret>,
    pub(crate) parallel_part: ParallelHistory<Op, Ret>,
    pub(crate) post_part: History<Op, Ret>,
}

pub(crate) struct InternalRecorder<Op, Ret> {
    thread_id: ThreadId,
    invocations: ParallelHistory<Op, Ret>,
    current_op: Option<Op>,
    call_timestamp: usize,
}

impl<Op, Ret> InternalRecorder<Op, Ret> {
    pub(crate) fn new(thread_id: ThreadId) -> Self {
        InternalRecorder {
            thread_id,
            invocations: Vec::new(),
            current_op: None,
            call_timestamp: 0,
        }
    }

    pub(crate) fn add_call(&mut self, op: Op, timestamp: usize) {
        assert!(self.current_op.is_none());
        self.call_timestamp = timestamp;
        self.current_op = Some(op);
    }

    pub(crate) fn add_return(&mut self, ret: Ret, timestamp: usize) {
        self.invocations.push(ParallelInvocation {
            thread_id: self.thread_id,
            call_timestamp: self.call_timestamp,
            return_timestamp: timestamp,
            op: self.current_op.take().unwrap(),
            ret,
        })
    }

    #[allow(dead_code)] // seems to be a bug because the method is used
    pub(crate) fn history(self) -> ParallelHistory<Op, Ret> {
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
        init_part: Vec::new(),
    }
}

pub fn record_parallel_part<Op, Ret>() -> ParallelPartRecorder<Op, Ret> {
    ParallelPartRecorder::new(Vec::new())
}

pub fn record_post_part<Op, Ret>() -> PostPartRecorder<Op, Ret> {
    PostPartRecorder {
        init_part: Vec::new(),
        parallel_part: Vec::new(),
        post_part: Vec::new(),
    }
}

pub struct InitPartRecorder<Op, Ret> {
    init_part: History<Op, Ret>,
}

impl<Op, Ret> Recorder for InitPartRecorder<Op, Ret> {
    type Op = Op;
    type Ret = Ret;

    fn record(&mut self, op: Op, f: impl FnOnce() -> Ret) {
        let ret = f();
        self.init_part.push(Invocation { op, ret })
    }
}

impl<Op, Ret> InitPartRecorder<Op, Ret> {
    pub fn record_parallel_part(self) -> ParallelPartRecorder<Op, Ret> {
        ParallelPartRecorder::new(self.init_part)
    }

    pub fn record_post_part(self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: self.init_part,
            parallel_part: Vec::new(),
            post_part: Vec::new(),
        }
    }

    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part,
            parallel_part: Vec::new(),
            post_part: Vec::new(),
        }
    }
}

pub struct ParallelPartRecorder<Op, Ret> {
    init_part: Mutex<History<Op, Ret>>,
    parallel_part: Mutex<ParallelHistory<Op, Ret>>,
    next_thread_id: AtomicUsize,
    timer: AtomicUsize,
}

impl<Op, Ret> ParallelPartRecorder<Op, Ret> {
    fn new(init_part: History<Op, Ret>) -> Self {
        ParallelPartRecorder {
            init_part: Mutex::new(init_part),
            parallel_part: Mutex::new(Vec::new()),
            next_thread_id: AtomicUsize::new(0),
            timer: AtomicUsize::new(0),
        }
    }

    pub fn record_thread(&self) -> PerThreadRecorder<'_, Op, Ret> {
        let thread_id = self.next_thread_id.load(Ordering::Relaxed);
        self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        PerThreadRecorder {
            internal_recorder: InternalRecorder::new(thread_id),
            parent_builder: self,
        }
    }

    pub fn record_post_part(&self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: std::mem::take(&mut self.init_part.lock().unwrap()),
            parallel_part: std::mem::take(&mut self.parallel_part.lock().unwrap()),
            post_part: Vec::new(),
        }
    }

    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part.into_inner().unwrap(),
            parallel_part: self.parallel_part.into_inner().unwrap(),
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
        let call_timestamp = self.parent_builder.timer.fetch_add(1, Ordering::Relaxed);
        self.internal_recorder.add_call(op, call_timestamp);

        let ret = f();

        let return_timestamp = self.parent_builder.timer.fetch_add(1, Ordering::Relaxed);
        self.internal_recorder.add_return(ret, return_timestamp);
    }
}

impl<'a, Op, Ret> Drop for PerThreadRecorder<'a, Op, Ret> {
    fn drop(&mut self) {
        let invocations = std::mem::take(&mut self.internal_recorder.invocations);

        self.parent_builder
            .parallel_part
            .lock()
            .unwrap()
            .extend(invocations);
    }
}

pub struct PostPartRecorder<Op, Ret> {
    init_part: History<Op, Ret>,
    parallel_part: ParallelHistory<Op, Ret>,
    post_part: History<Op, Ret>,
}

impl<Op, Ret> Recorder for PostPartRecorder<Op, Ret> {
    type Op = Op;
    type Ret = Ret;

    fn record(&mut self, op: Op, f: impl FnOnce() -> Ret) {
        let ret = f();
        self.post_part.push(Invocation { op, ret });
    }
}

impl<Op, Ret> PostPartRecorder<Op, Ret> {
    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part,
            parallel_part: self.parallel_part,
            post_part: self.post_part,
        }
    }
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
                    op: Op::A,
                    ret: Ret::A,
                },
                Invocation {
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
                ParallelInvocation {
                    thread_id: 0,
                    call_timestamp: 0,
                    return_timestamp: 3,
                    op: Op::A,
                    ret: Ret::A,
                },
                ParallelInvocation {
                    thread_id: 1,
                    call_timestamp: 1,
                    return_timestamp: 2,
                    op: Op::B,
                    ret: Ret::B,
                },
            ]
        )
    }

    #[test]
    fn test_record_post() {
        let mut recorder = record_init_part().record_post_part();

        recorder.record(Op::A, || Ret::A);
        recorder.record(Op::B, || Ret::B);

        let execution = recorder.finish();

        assert_eq!(
            execution.post_part,
            vec![
                Invocation {
                    op: Op::A,
                    ret: Ret::A,
                },
                Invocation {
                    op: Op::B,
                    ret: Ret::B,
                },
            ]
        );
    }
}
