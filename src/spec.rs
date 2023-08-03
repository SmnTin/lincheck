/// The sequential implementation of a data structure.
pub trait SequentialSpec: Default {
    /// The type of operations.
    type Op;

    /// The type of return values.
    type Ret;

    /// Executes an operation on the data structure.
    fn exec(&mut self, op: Self::Op) -> Self::Ret;
}

/// The concurrent implementation of a data structure.
pub trait ConcurrentSpec: Default {
    /// The sequential specification for the data structure.
    type Seq: SequentialSpec;

    /// Executes an operation on the data structure.
    fn exec(&self, op: ConcOp<Self>) -> ConcRet<Self>;
}

/// Type alias not to have always write down FQP.
pub type ConcOp<T> = <<T as ConcurrentSpec>::Seq as SequentialSpec>::Op;

/// Type alias not to have always write down FQP.
pub type ConcRet<T> = <<T as ConcurrentSpec>::Seq as SequentialSpec>::Ret;
