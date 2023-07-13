use loom::thread;
use proptest::{
    prelude::*,
    test_runner::{TestError, TestRunner},
};
use std::sync::{Arc, Mutex};
use std::{fmt::Debug, panic, rc::Rc};

mod checker;
mod execution;
mod fmt;
mod recorder;
mod spec;

pub use checker::*;
pub use execution::*;
pub use recorder::*;
pub use spec::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario<Op> {
    pub init_part: Vec<Op>,
    pub parallel_part: Vec<Vec<Op>>,
    pub post_part: Vec<Op>,
}

pub fn check_scenario<Conc, Seq>(scenario: Scenario<Conc::Op>)
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
    Conc::Op: Send + Sync + Clone + Debug + 'static,
    Conc::Ret: PartialEq + Debug + Send,
{
    loom::model(move || {
        let execution = execute_scenario::<Conc>(scenario.clone());
        if !LinearizabilityChecker::<Seq>::check(&execution) {
            panic::panic_any(execution);
        }
    });
}

fn execute_scenario<Conc>(scenario: Scenario<Conc::Op>) -> Execution<Conc::Op, Conc::Ret>
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    Conc::Op: Send + Sync + Clone + 'static,
    Conc::Ret: PartialEq,
{
    let conc = Rc::new(Conc::new());

    let mut recorder = recorder::record_init_part_with_capacity(scenario.init_part.len());

    // init part
    execute_ops(&*conc, &mut recorder, scenario.init_part);

    let total_parallel_ops = scenario.parallel_part.iter().map(Vec::len).sum();
    let recorder = Rc::new(recorder.record_parallel_part_with_capacity(total_parallel_ops));

    // parallel part
    let handles: Vec<_> = scenario
        .parallel_part
        .into_iter()
        .map(|thread_ops| {
            let conc = conc.clone();
            let recorder = recorder.clone();

            thread::spawn(move || {
                let mut recorder = recorder.record_thread_with_capacity(thread_ops.len());

                execute_ops(&*conc, &mut recorder, thread_ops);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // post part
    let mut recorder = recorder.record_post_part_with_capacity(scenario.post_part.len());
    execute_ops(&*conc, &mut recorder, scenario.post_part);

    recorder.finish()
}

fn execute_ops<Conc, Rec>(conc: &Conc, recorder: &mut Rec, ops: Vec<Conc::Op>)
where
    Conc: ConcurrentSpec,
    Conc::Op: Clone,
    Rec: Recorder<Op = Conc::Op, Ret = Conc::Ret>,
{
    for op in ops {
        recorder.record(op.clone(), || conc.exec(op));
    }
}

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
        let ops_strategy = || prop::collection::vec(any::<Op>(), 1..=args.num_ops);
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
        Conc::Op: Send + Sync + Clone + Arbitrary + Debug + 'static,
        Conc::Ret: PartialEq + Debug + Send + Clone,
    {
        let failed_execution = Arc::new(Mutex::new(Execution::<Conc::Op, Conc::Ret>::default()));

        let old_hook = panic::take_hook();

        panic::set_hook({
            let failed_execution = failed_execution.clone();
            Box::new(move |panic_info| {
                let execution = panic_info
                    .payload()
                    .downcast_ref::<Execution<Conc::Op, Conc::Ret>>();
                if let Some(execution) = execution {
                    *failed_execution.lock().unwrap() = execution.clone();
                }
            })
        });

        let result = TestRunner::default().run(&any::<Scenario<Conc::Op>>(), |scenario| {
            check_scenario::<Conc, Seq>(scenario);
            Ok(())
        });

        panic::set_hook(old_hook);

        match result {
            Ok(_) => Ok(()),
            Err(TestError::Fail(_, _)) => {
                let failed_execution = Arc::into_inner(failed_execution)
                    .unwrap()
                    .into_inner()
                    .unwrap();
                Err(failed_execution)
            }
            Err(failure) => panic!("Unexpected failure: {:?}", failure),
        }
    }

    pub fn verify_or_panic<Conc, Seq>(&self)
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
        Conc::Op: Send + Sync + Clone + Arbitrary + Debug + 'static,
        Conc::Ret: PartialEq + Debug + Send + Clone,
    {
        let result = self.verify::<Conc, Seq>();
        if let Err(execution) = result {
            panic!("Non-linearizable execution: \n\n {}", execution);
        }
    }
}
