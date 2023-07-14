use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};

use loom::sync::atomic::{AtomicBool, Ordering};
use proptest::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    WriteX,
    WriteY,
    ReadX,
    ReadY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ret {
    Write,
    Read(bool),
}

impl Arbitrary for Op {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Op::WriteX),
            Just(Op::WriteY),
            Just(Op::ReadX),
            Just(Op::ReadY),
        ]
        .boxed()
    }
}

struct TwoSlotsParallel {
    x: AtomicBool,
    y: AtomicBool,
}

impl ConcurrentSpec for TwoSlotsParallel {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self {
            x: AtomicBool::new(false),
            y: AtomicBool::new(false),
        }
    }

    fn exec(&self, op: Op) -> Ret {
        match op {
            Op::WriteX => {
                self.x.store(true, Ordering::Relaxed);
                Ret::Write
            }
            Op::WriteY => {
                self.y.store(true, Ordering::Relaxed);
                Ret::Write
            }
            Op::ReadX => Ret::Read(self.x.load(Ordering::Relaxed)),
            Op::ReadY => Ret::Read(self.y.load(Ordering::Relaxed)),
        }
    }
}

struct TwoSlotsSequential {
    x: bool,
    y: bool,
}

impl SequentialSpec for TwoSlotsSequential {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self { x: false, y: false }
    }

    fn exec(&mut self, op: Op) -> Ret {
        match op {
            Op::WriteX => {
                self.x = true;
                Ret::Write
            }
            Op::WriteY => {
                self.y = true;
                Ret::Write
            }
            Op::ReadX => Ret::Read(self.x),
            Op::ReadY => Ret::Read(self.y),
        }
    }
}

#[test]
#[should_panic]
fn two_slots() {
    Lincheck::default().verify_or_panic::<TwoSlotsParallel, TwoSlotsSequential>()
}
