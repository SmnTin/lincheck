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

#[derive(Clone, Debug)]
pub struct Lincheck {
    pub num_threads: usize,
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
    pub fn verify<Conc, Seq>(&self) -> Result<(), Execution<Conc::Op, Conc::Ret>>
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
        Conc::Op: Send + Sync + Clone + Arbitrary + Debug + UnwindSafe + 'static,
        Conc::Ret: PartialEq + Debug + Send + Clone,
    {
        let result = TestRunner::default().run(&any::<Scenario<Conc::Op>>(), |scenario| {
            check_scenario_with_loom::<Conc, Seq>(scenario)
                .map_err(|_| TestCaseError::Fail("Non-linearizable execution".into()))
        });

        match result {
            Ok(_) => Ok(()),
            Err(TestError::Fail(_, scenario)) => {
                Err(check_scenario_with_loom::<Conc, Seq>(scenario).unwrap_err())
            }
            Err(failure) => panic!("Unexpected failure: {:?}", failure),
        }
    }

    pub fn verify_or_panic<Conc, Seq>(&self)
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
        Conc::Op: Send + Sync + UnwindSafe + Clone + Arbitrary + Debug + 'static,
        Conc::Ret: PartialEq + Debug + Send + Clone,
    {
        let result = self.verify::<Conc, Seq>();
        if let Err(execution) = result {
            panic!("Non-linearizable execution: \n\n {}", execution);
        }
    }
}
