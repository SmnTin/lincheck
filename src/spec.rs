/// The sequential implementation of a data structure.
pub trait SequentialSpec {
    /// The type of operations.
    type Op;

    /// The type of return values.
    type Ret;

    /// Creates a new instance of the data structure.
    fn new() -> Self;

    /// Executes an operation on the data structure.
    fn exec(&mut self, op: Self::Op) -> Self::Ret;
}

/// The concurrent implementation of a data structure.
pub trait ConcurrentSpec {
    /// The type of operations.
    type Op;

    /// The type of return values.
    type Ret;

    /// Creates a new instance of the data structure.
    fn new() -> Self;

    /// Executes an operation on the data structure.
    fn exec(&self, op: Self::Op) -> Self::Ret;
}
