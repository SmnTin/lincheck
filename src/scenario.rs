use loom::thread;
use std::fmt::Debug;
use std::panic::{self, UnwindSafe};
use std::rc::Rc;

use crate::checker::*;
use crate::execution::*;
use crate::recorder::{self, *};
use crate::spec::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario<Op> {
    pub init_part: Vec<Op>,
    pub parallel_part: Vec<Vec<Op>>,
    pub post_part: Vec<Op>,
}

pub fn check_scenario_with_loom<Conc, Seq>(
    scenario: Scenario<Conc::Op>,
) -> Result<(), Execution<Conc::Op, Conc::Ret>>
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    Seq: SequentialSpec<Op = Conc::Op, Ret = Conc::Ret> + Send + Sync + 'static,
    Conc::Op: Send + Sync + Clone + Debug + UnwindSafe + 'static,
    Conc::Ret: PartialEq + Clone + Debug + Send,
{
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let result = panic::catch_unwind(|| {
        loom::model(move || {
            let execution = execute_scenario_with_loom::<Conc>(scenario.clone());
            if !LinearizabilityChecker::<Seq>::check(&execution) {
                panic::panic_any(execution);
            }
        });
    });

    panic::set_hook(old_hook);

    result.map_err(|payload| {
        *payload
            .downcast::<Execution<Conc::Op, Conc::Ret>>()
            .unwrap_or_else(|_| panic!("loom::model panicked with unknown payload"))
    })
}

fn execute_scenario_with_loom<Conc>(scenario: Scenario<Conc::Op>) -> Execution<Conc::Op, Conc::Ret>
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    Conc::Op: Send + Sync + Clone + 'static,
    Conc::Ret: PartialEq,
{
    let conc = Rc::new(Conc::new());

    let mut recorder = recorder::record_init_part_with_capacity(scenario.init_part.len());

    // init part
    for op in scenario.init_part {
        recorder.record(op.clone(), || conc.exec(op));
    }

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
                for op in thread_ops {
                    recorder.record(op.clone(), || conc.exec(op));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // post part
    let mut recorder = recorder.record_post_part_with_capacity(scenario.post_part.len());
    for op in scenario.post_part {
        recorder.record(op.clone(), || conc.exec(op));
    }

    recorder.finish()
}
