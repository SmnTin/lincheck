use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};

use loom::{
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};
use proptest::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Increment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ret {
    OldValue(usize),
}

impl Arbitrary for Op {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop_oneof![Just(Op::Increment),].boxed()
    }
}

struct ConcurrentCounter {
    x: AtomicUsize,
}

impl ConcurrentSpec for ConcurrentCounter {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self {
            x: AtomicUsize::new(0),
        }
    }

    fn exec(&self, op: Op) -> Ret {
        match op {
            Op::Increment => loop {
                let val = self.x.load(Ordering::Relaxed);
                if self
                    .x
                    .compare_exchange(val, val + 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break Ret::OldValue(val);
                }
                thread::yield_now();
                thread::yield_now(); // strangely, one yield_now is not enough
            },
        }
    }
}

struct SequentialCounter {
    x: usize,
}

impl SequentialSpec for SequentialCounter {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self { x: 0 }
    }

    fn exec(&mut self, op: Op) -> Ret {
        match op {
            Op::Increment => {
                let val = self.x;
                self.x += 1;
                Ret::OldValue(val)
            }
        }
    }
}

#[test]
fn counter() {
    Lincheck {
        num_threads: 1,
        num_ops: 1,
    }
    .verify_or_panic::<ConcurrentCounter, SequentialCounter>()
}
