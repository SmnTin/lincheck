
pub trait SequentialSpec {
    type Op;
    type Ret;

    fn new() -> Self;
    fn exec(&mut self, op: Self::Op) -> Self::Ret;
}

pub trait ConcurrentSpec {
    type Op;
    type Ret;

    fn new() -> Self;
    fn exec(&self, op: Self::Op) -> Self::Ret;
}