#![warn(missing_docs)]

//! Lincheck is a Rust library for testing concurrent data structures for [linearizability](https://en.wikipedia.org/wiki/Linearizability). Simply put, it checks whether a concurrent data structure behaves similarly to a simpler sequential implementation. It is inspired by [Lincheck for Kotlin](https://github.com/JetBrains/lincheck) and is built on top of [loom](https://github.com/tokio-rs/loom), a model-checker for concurrency.
//!
//! # Features
//!
//! - Lincheck uses [proptest](https://docs.rs/proptest/latest/proptest/) to generate random concurrent scenarios and automatically shrink them to a minimal failing scenario.
//! - Lincheck runs every scenario inside [loom](https://github.com/tokio-rs/loom) model-checker to check every possible interleaving of operations.
//! - Lincheck provides a simple API for defining concurrent data structures and their sequential counterparts.
//! - Recording of execution traces is made to introduce as little additional synchronization between threads as possible.
//!
//! # Tutorial
//!
//! ```rust
//! use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};
//! use loom::sync::atomic::{AtomicBool, Ordering};
//! use proptest::prelude::*;
//!
//! // Let's implement a simple concurrent data structure:
//! // a pair of boolean flags `x` and `y`
//! // that can be read and written by multiple threads.
//! // The flags are initialized to `false` and can be switched to `true`.
//!
//! // We start by defining the operations:
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! enum Op {
//!     WriteX, // set x to true
//!     WriteY, // set y to true
//!     ReadX, // get the value of x
//!     ReadY, // get the value of y
//! }
//!
//! // ... and their results:
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! enum Ret {
//!     Write, // the result of a write operation
//!     Read(bool), // the result of a read operation
//! }
//!
//! // We need to implement the `Arbitrary` trait for our operations
//! // to be able to generate them randomly:
//! impl Arbitrary for Op {
//!     type Parameters = ();
//!     type Strategy = BoxedStrategy<Self>;
//!
//!     fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
//!         prop_oneof![
//!             Just(Op::WriteX),
//!             Just(Op::WriteY),
//!             Just(Op::ReadX),
//!             Just(Op::ReadY),
//!         ]
//!         .boxed()
//!     }
//! }
//!
//! // We then define the sequential implementation which we test against.
//! // This type must be default-constructible:
//! #[derive(Default)]
//! struct TwoSlotsSequential {
//!     x: bool,
//!     y: bool,
//! }
//!
//! // We implement the `SequentialSpec` trait for our implementation:
//! impl SequentialSpec for TwoSlotsSequential {
//!     type Op = Op;
//!     type Ret = Ret;
//!
//!     fn exec(&mut self, op: Op) -> Ret {
//!         match op {
//!             Op::WriteX => {
//!                 self.x = true;
//!                 Ret::Write
//!             }
//!             Op::WriteY => {
//!                 self.y = true;
//!                 Ret::Write
//!             }
//!             Op::ReadX => Ret::Read(self.x),
//!             Op::ReadY => Ret::Read(self.y),
//!         }
//!     }
//! }
//!
//! // We then define the concurrent implementation that we want to test.
//! // This type must be default-constructible too:
//! #[derive(Default)]
//! struct TwoSlotsParallel {
//!     x: AtomicBool,
//!     y: AtomicBool,
//! }
//!
//! // We implement the `ConcurrentSpec` trait for our implementation:
//! impl ConcurrentSpec for TwoSlotsParallel {
//!     // We declare which sequential specification
//!     // this data structure implements
//!     type Seq = TwoSlotsSequential;
//!
//!     // We must be able to execute an operation on our implementation.
//!     // Note that we reuse `Op` and `Ret` types from the sequential spec.
//!     fn exec(&self, op: Op) -> Ret {
//!         match op {
//!             Op::WriteX => {
//!                 self.x.store(true, Ordering::Relaxed);
//!                 Ret::Write
//!             }
//!             Op::WriteY => {
//!                 self.y.store(true, Ordering::Relaxed);
//!                 Ret::Write
//!             }
//!             Op::ReadX => Ret::Read(self.x.load(Ordering::Relaxed)),
//!             Op::ReadY => Ret::Read(self.y.load(Ordering::Relaxed)),
//!         }
//!     }
//! }
//!
//!
//! // Notice that the concurrent specification receives a shared reference to itself (`&self`)
//! // while the sequential specification receives an exclusive reference to itself (`&mut self`).
//! // This is because the concurrent specification is shared between threads
//! // while the sequential specification is not.
//!
//! // We are now ready to write our test:
//! #[test]
//! fn two_slots() {
//!     Lincheck {
//!         num_threads: 2,
//!         num_ops: 5,
//!     }.verify::<TwoSlotsParallel>();
//! }
//! ```
//!
//! If we run the test, we get a failure along with a trace of the execution:
//! ```text
//! running 1 test
//! test two_slots ... FAILED
//!
//! failures:
//!
//! ---- two_slots stdout ----
//! thread 'two_slots' panicked at 'Non-linearizable execution:
//!
//!  INIT PART:
//! |================|
//! |  MAIN THREAD   |
//! |================|
//! |                |
//! | WriteX : Write |
//! |                |
//! |----------------|
//!
//! PARALLEL PART:
//! |=====================|================|
//! |      THREAD 0       |    THREAD 1    |
//! |=====================|================|
//! |                     |                |
//! |                     |----------------|
//! |                     |                |
//! | ReadY : Read(false) | WriteY : Write |
//! |                     |                |
//! |                     |----------------|
//! |                     |                |
//! |---------------------|                |
//! |                     |                |
//! | ReadY : Read(false) |                |
//! |                     |                |
//! |---------------------|----------------|
//!
//! POST PART:
//! |================|
//! |  MAIN THREAD   |
//! |================|
//! |                |
//! | WriteX : Write |
//! |                |
//! |----------------|
//! ```
//!
//! # Limitations
//!
//! - Lincheck runner sets its own panic hook. This doesn't play well with parallel test execution. To fix this, you can run your tests with the `--test-threads=1` flag like this:
//! ```bash
//! $ cargo test -- --test-threads=1
//! ```
//! - [loom](https://github.com/tokio-rs/loom) can't model all weak memory models effects. This means that some executions that may arise on the real hardware may not be explored by loom. This is why the concurrent data structures should be additionally fuzzed on the real hardware. The support for fuzzing in Lincheck is planned.
//! - [proptest](https://docs.rs/proptest/latest/proptest/) only explores a random sample of all possible scenarios. This means that some failing executions may not be explored.

use proptest::{
    prelude::*,
    test_runner::{TestError, TestRunner},
};
use std::panic::UnwindSafe;
use std::{fmt::Debug, panic};

pub mod checker;
mod execution;
mod fmt;
pub mod recorder;
pub mod scenario;
mod spec;

pub use execution::*;
use scenario::*;
pub use spec::*;

/// A test runner for Lincheck.
/// It is used to configure the test.
#[derive(Clone, Debug)]
pub struct Lincheck {
    /// The maximum number of threads to use in the test.
    pub num_threads: usize,
    /// The maximum number of operations to run per thread.
    pub num_ops: usize,
}

impl Default for Lincheck {
    fn default() -> Self {
        Self {
            num_threads: 2,
            num_ops: 5,
        }
    }
}

impl<Op: Arbitrary + 'static> Arbitrary for Scenario<Op> {
    type Parameters = Lincheck;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        let ops_strategy = || prop::collection::vec(any::<Op>(), 0..=args.num_ops);
        let init_strategy = ops_strategy();
        let post_strategy = ops_strategy();

        let parallel_strategy = prop::collection::vec(ops_strategy(), 1..=args.num_threads);

        (init_strategy, parallel_strategy, post_strategy)
            .prop_map(|(init_part, parallel_part, post_part)| Self {
                init_part,
                parallel_part,
                post_part,
            })
            .boxed()
    }
}

impl Lincheck {
    /// Verifies that the concurrent implementation `Conc` is linearizable with respect to the sequential implementation `Seq`.
    /// They must use the same operations and return the same results.
    ///
    /// The test runner will generate random scenarios and check that regardless of the interleaving of the operations,
    /// the parallel execution is linearizable to some sequential execution.
    ///
    /// It returns a non-linearizable execution if the test fails.
    pub fn verify<Conc>(&self) -> Result<(), Execution<ConcOp<Conc>, ConcRet<Conc>>>
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Conc::Seq: Send + Sync + 'static,
        ConcOp<Conc>: Send + Sync + Clone + Arbitrary + Debug + UnwindSafe + 'static,
        ConcRet<Conc>: PartialEq + Debug + Send + Clone,
    {
        let result = TestRunner::default().run(&any::<Scenario<ConcOp<Conc>>>(), |scenario| {
            check_scenario_with_loom::<Conc>(scenario)
                .map_err(|_| TestCaseError::Fail("Non-linearizable execution".into()))
        });

        match result {
            Ok(_) => Ok(()),
            Err(TestError::Fail(_, scenario)) => {
                // rerun the scenario to get the failing execution
                Err(check_scenario_with_loom::<Conc>(scenario).unwrap_err())
            }
            Err(failure) => panic!("Unexpected failure: {:?}", failure),
        }
    }

    /// The same as [verify](Lincheck::verify) but automatically panics and pretty-prints the execution if the test fails.
    pub fn verify_or_panic<Conc>(&self)
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Conc::Seq: Send + Sync + 'static,
        <Conc::Seq as SequentialSpec>::Op:
            Send + Sync + UnwindSafe + Clone + Arbitrary + Debug + 'static,
        <Conc::Seq as SequentialSpec>::Ret: PartialEq + Debug + Send + Clone,
    {
        let result = self.verify::<Conc>();
        if let Err(execution) = result {
            panic!("Non-linearizable execution: \n\n {}", execution);
        }
    }
}
