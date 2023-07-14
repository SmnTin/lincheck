//! Recorders that are used to record the execution of a concurrent program.
//!
//! This is a low-level module. You probably want to use
//! [execute_scenario_with_loom](crate::scenario::execute_scenario_with_loom) instead.
//!
//! The recorder is in fact a type-level state machine. Each state implements the [Recorder] trait.
//! There are three states:
//! - [InitPartRecorder], which records the initial part of the execution
//! - [ParallelPartRecorder], which records the parallel part of the execution
//! - [PostPartRecorder], which records the post part of the execution
//!
//! [ParallelPartRecorder] is split into several [PerThreadRecorder]s, one for each thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::execution::*;

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
            invocations: ParallelHistory::new(),
            current_op: None,
            call_timestamp: 0,
        }
    }

    pub(crate) fn with_capacity(thread_id: ThreadId, capacity: usize) -> Self {
        InternalRecorder {
            thread_id,
            invocations: ParallelHistory::with_capacity(capacity),
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

/// A trait for each state of the recorder.
pub trait Recorder {
    /// The type of the operations that are executed on the data structure.
    type Op;

    /// The type of the return values of the operations.
    type Ret;

    /// The method that records an operation execution.
    /// The record about the operation call is created before the function is called.
    /// The record about the operation return is created after the function is called.
    fn record(&mut self, op: Self::Op, f: impl FnOnce() -> Self::Ret);
}

/// Creates a recorder for the initial part of the execution.
pub fn record_init_part<Op, Ret>() -> InitPartRecorder<Op, Ret> {
    InitPartRecorder {
        init_part: History::new(),
    }
}

/// Creates a recorder for the parallel part of the execution.
pub fn record_parallel_part<Op, Ret>() -> ParallelPartRecorder<Op, Ret> {
    ParallelPartRecorder::new(History::new())
}

/// Creates a recorder for the post part of the execution.
pub fn record_post_part<Op, Ret>() -> PostPartRecorder<Op, Ret> {
    PostPartRecorder {
        init_part: History::new(),
        parallel_part: ParallelHistory::new(),
        post_part: History::new(),
    }
}

/// Same as [record_init_part] but allows to preallocate space for the records.
pub fn record_init_part_with_capacity<Op, Ret>(
    init_part_capacity: usize,
) -> InitPartRecorder<Op, Ret> {
    InitPartRecorder {
        init_part: History::with_capacity(init_part_capacity),
    }
}

/// Same as [record_parallel_part] but allows to preallocate space for the records.
pub fn record_parallel_part_with_capacity<Op, Ret>(
    parallel_part_capacity: usize,
) -> ParallelPartRecorder<Op, Ret> {
    ParallelPartRecorder::with_capacity(History::new(), parallel_part_capacity)
}

/// Same as [record_post_part] but allows to preallocate space for the records.
pub fn record_post_part_with_capacity<Op, Ret>(
    post_part_capacity: usize,
) -> PostPartRecorder<Op, Ret> {
    PostPartRecorder {
        init_part: History::new(),
        parallel_part: ParallelHistory::new(),
        post_part: History::with_capacity(post_part_capacity),
    }
}

/// A recorder for the initial part of the execution.
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
    /// Switches to the parallel part of the execution.
    pub fn record_parallel_part(self) -> ParallelPartRecorder<Op, Ret> {
        ParallelPartRecorder::new(self.init_part)
    }

    /// Switches to the post part of the execution.
    pub fn record_post_part(self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: self.init_part,
            parallel_part: ParallelHistory::new(),
            post_part: History::new(),
        }
    }

    /// Same as [record_parallel_part](InitPartRecorder::record_parallel_part) but allows to preallocate space for the records.
    pub fn record_parallel_part_with_capacity(
        self,
        parallel_part_capacity: usize,
    ) -> ParallelPartRecorder<Op, Ret> {
        ParallelPartRecorder::with_capacity(self.init_part, parallel_part_capacity)
    }

    /// Same as [record_post_part](InitPartRecorder::record_post_part) but allows to preallocate space for the records.
    pub fn record_post_part_with_capacity(
        self,
        post_part_capacity: usize,
    ) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: self.init_part,
            parallel_part: ParallelHistory::new(),
            post_part: History::with_capacity(post_part_capacity),
        }
    }

    /// Finishes recording and returns the execution trace.
    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part,
            parallel_part: ParallelHistory::new(),
            post_part: History::new(),
        }
    }
}

/// A recorder for the parallel part of the execution.
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
            parallel_part: Mutex::new(ParallelHistory::new()),
            next_thread_id: AtomicUsize::new(0),
            timer: AtomicUsize::new(0),
        }
    }

    fn with_capacity(init_part: History<Op, Ret>, parallel_part_capacity: usize) -> Self {
        ParallelPartRecorder {
            init_part: Mutex::new(init_part),
            parallel_part: Mutex::new(ParallelHistory::with_capacity(parallel_part_capacity)),
            next_thread_id: AtomicUsize::new(0),
            timer: AtomicUsize::new(0),
        }
    }

    /// Creates a sub-recorder for a single thread.
    pub fn record_thread(&self) -> PerThreadRecorder<'_, Op, Ret> {
        let thread_id = self.next_thread_id.load(Ordering::Relaxed);
        self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        PerThreadRecorder {
            internal_recorder: InternalRecorder::new(thread_id),
            parent_builder: self,
        }
    }

    /// Same as [record_thread](ParallelPartRecorder::record_thread) but allows to preallocate space for the records.
    pub fn record_thread_with_capacity(
        &self,
        thread_part_capacity: usize,
    ) -> PerThreadRecorder<'_, Op, Ret> {
        let thread_id = self.next_thread_id.load(Ordering::Relaxed);
        self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        PerThreadRecorder {
            internal_recorder: InternalRecorder::with_capacity(thread_id, thread_part_capacity),
            parent_builder: self,
        }
    }

    /// Switches to the post part of the execution.
    pub fn record_post_part(&self) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: std::mem::take(&mut self.init_part.lock().unwrap()),
            parallel_part: std::mem::take(&mut self.parallel_part.lock().unwrap()),
            post_part: History::new(),
        }
    }

    /// Same as [record_post_part](ParallelPartRecorder::record_post_part) but allows to preallocate space for the records.
    pub fn record_post_part_with_capacity(
        &self,
        post_part_capacity: usize,
    ) -> PostPartRecorder<Op, Ret> {
        PostPartRecorder {
            init_part: std::mem::take(&mut self.init_part.lock().unwrap()),
            parallel_part: std::mem::take(&mut self.parallel_part.lock().unwrap()),
            post_part: History::with_capacity(post_part_capacity),
        }
    }

    /// Finishes recording and returns the execution trace.
    pub fn finish(self) -> Execution<Op, Ret> {
        Execution {
            init_part: self.init_part.into_inner().unwrap(),
            parallel_part: self.parallel_part.into_inner().unwrap(),
            post_part: History::new(),
        }
    }
}

/// A recorder for a single thread.
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

/// A recorder for the post part of the execution.
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
    /// Finishes recording and returns the execution trace.
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
    use std::ops::Deref;
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
            execution.init_part.deref(),
            &vec![
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
            execution.parallel_part.deref(),
            &vec![
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
            execution.post_part.deref(),
            &vec![
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
