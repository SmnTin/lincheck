use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};

use loom::sync::atomic::{AtomicBool, Ordering};
use quickcheck::{Arbitrary, Gen};

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
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 4 {
            0 => Op::WriteX,
            1 => Op::WriteY,
            2 => Op::ReadX,
            3 => Op::ReadY,
            _ => unreachable!(),
        }
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
// #[should_panic]
fn two_slots() {
    Lincheck::default().verify::<TwoSlotsParallel, TwoSlotsSequential>();
}
