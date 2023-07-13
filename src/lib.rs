use loom::thread;
use quickcheck::{Arbitrary, Gen}; // TODO: use some other crate for this
use std::{fmt::Debug, rc::Rc};

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
    Conc::Ret: PartialEq + Debug,
{
    loom::model(move || {
        let execution = execute_scenario::<Conc>(scenario.clone());
        if !LinearizabilityChecker::<Seq>::check(&execution) {
            panic!("Non-linearizable execution: \n\n{}", execution);
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

    let total_parallel_ops = scenario.parallel_part.iter().map(|ops| ops.len()).sum();
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

pub struct Lincheck {
    pub num_threads: usize,
    pub num_iterations: usize,
    pub num_ops: usize,
}

impl Default for Lincheck {
    fn default() -> Self {
        Self {
            num_threads: 2,
            num_iterations: 100,
            num_ops: 4,
        }
    }
}

impl Lincheck {
    fn generate_scenario<Op: Arbitrary>(&self) -> Scenario<Op> {
        let mut scenario = Scenario {
            init_part: Vec::new(),
            parallel_part: Vec::new(),
            post_part: Vec::new(),
        };

        let mut gen = Gen::new(50);

        for _ in 0..self.num_ops {
            scenario.init_part.push(Op::arbitrary(&mut gen));
        }

        for _ in 0..self.num_threads {
            let mut ops = Vec::new();

            for _ in 0..self.num_ops {
                ops.push(Op::arbitrary(&mut gen));
            }

            scenario.parallel_part.push(ops);
        }

        for _ in 0..self.num_ops {
            scenario.post_part.push(Op::arbitrary(&mut gen));
        }

        scenario
    }

    pub fn verify<Conc, Seq>(&self)
    where
        Conc: ConcurrentSpec + Send + Sync + 'static,
        Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
        Conc::Op: Send + Sync + Clone + Arbitrary + Debug + 'static,
        Conc::Ret: PartialEq + Debug,
    {
        for _ in 0..self.num_iterations {
            check_scenario::<Conc, Seq>(self.generate_scenario::<Conc::Op>());
        }
    }
}
