use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};

use loom::sync::Mutex;
use quickcheck::{Arbitrary, Gen};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op<T> {
    Push(T),
    Pop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ret<T> {
    Push,
    Pop(Option<T>),
}

impl<T: Arbitrary> Arbitrary for Op<T> {
    fn arbitrary(g: &mut Gen) -> Self {
        match bool::arbitrary(g) {
            true => Op::Push(T::arbitrary(g)),
            false => Op::Pop,
        }
    }
}

impl<T: Arbitrary> Arbitrary for Ret<T> {
    fn arbitrary(g: &mut Gen) -> Self {
        match bool::arbitrary(g) {
            true => Ret::Push,
            false => Ret::Pop(Option::arbitrary(g)),
        }
    }
}

struct SequentialStack<T> {
    stack: Vec<T>,
}

impl<T> SequentialSpec for SequentialStack<T> {
    type Op = Op<T>;
    type Ret = Ret<T>;

    fn new() -> Self {
        Self { stack: Vec::new() }
    }

    fn exec(&mut self, op: Self::Op) -> Self::Ret {
        match op {
            Op::Push(value) => {
                self.stack.push(value);
                Ret::Push
            }
            Op::Pop => Ret::Pop(self.stack.pop()),
        }
    }
}

struct ConcurrentStack<T> {
    stack: Mutex<Vec<T>>,
}

impl<T> ConcurrentSpec for ConcurrentStack<T> {
    type Op = Op<T>;
    type Ret = Ret<T>;

    fn new() -> Self {
        Self {
            stack: Mutex::new(Vec::new()),
        }
    }

    fn exec(&self, op: Self::Op) -> Self::Ret {
        let mut stack = self.stack.lock().unwrap();
        match op {
            Op::Push(value) => {
                stack.push(value);
                Ret::Push
            }
            Op::Pop => Ret::Pop(stack.pop()),
        }
    }
}

#[test]
fn models_stack() {
    Lincheck::default().verify::<ConcurrentStack<u8>, SequentialStack<u8>>();
}
