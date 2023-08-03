//! The [Scenario] and how to execute and check it.

use loom::thread;
use std::fmt::Debug;
use std::panic::{self, UnwindSafe};
use std::rc::Rc;

use crate::checker::*;
use crate::execution::*;
use crate::recorder::{self, *};
use crate::spec::*;

/// A scenario tells which operations to run in which order.
/// It consists of three parts: [init_part](Scenario::init_part), [parallel_part](Scenario::parallel_part) and [post_part](Scenario::post_part).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario<Op> {
    /// The initial part of the scenario, which is executed sequentially before the parallel part.
    pub init_part: Vec<Op>,

    /// The parallel part of the scenario, which is executed concurrently.
    /// Each vector of operations is executed in a separate thread.
    pub parallel_part: Vec<Vec<Op>>,

    /// The post part of the scenario, which is executed sequentially after the parallel part.
    pub post_part: Vec<Op>,
}

/// Executes the given scenario and checks the resulting execution for linearizability inside [loom] model-checker.
///
/// It works by panicking inside using the failing execution as the message and catching the panic outside.
/// This is the only way to return a value from loom model-checker.
pub fn check_scenario_with_loom<Conc>(
    scenario: Scenario<ConcOp<Conc>>,
) -> Result<(), Execution<ConcOp<Conc>, ConcRet<Conc>>>
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    Conc::Seq: Send + Sync + 'static,
    ConcOp<Conc>: Send + Sync + Clone + Debug + UnwindSafe + 'static,
    ConcRet<Conc>: PartialEq + Clone + Debug + Send,
{
    // temporarily disable the panic hook to avoid printing the panic message
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    // catch the panic and return the panic payload
    let result = panic::catch_unwind(|| {
        loom::model(move || {
            let execution = execute_scenario_with_loom::<Conc>(scenario.clone());
            if !LinearizabilityChecker::<Conc::Seq>::check(&execution) {
                // panic with the failing execution as the payload
                panic::panic_any(execution);
            }
        });
    });

    // restore the panic hook
    panic::set_hook(old_hook);

    result.map_err(|payload| {
        // recover the failing execution from the panic payload
        *payload
            .downcast::<Execution<ConcOp<Conc>, ConcRet<Conc>>>()
            .unwrap_or_else(|_| panic!("loom::model panicked with unknown payload"))
    })
}

/// Executes the given scenario with [loom] mock threads and returns the resulting execution.
pub fn execute_scenario_with_loom<Conc>(
    scenario: Scenario<ConcOp<Conc>>,
) -> Execution<ConcOp<Conc>, ConcRet<Conc>>
where
    Conc: ConcurrentSpec + Send + Sync + 'static,
    ConcOp<Conc>: Send + Sync + Clone + 'static,
    ConcRet<Conc>: PartialEq,
{
    let conc = Rc::new(Conc::default());

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

            // spawning threads creates a happens-before relation between the threads and the main thread
            thread::spawn(move || {
                let mut recorder = recorder.record_thread_with_capacity(thread_ops.len());
                for op in thread_ops {
                    recorder.record(op.clone(), || conc.exec(op));
                }
            })
        })
        .collect();

    // wait for all threads to finish before executing the post part
    for handle in handles {
        handle.join().unwrap();
    }
    // joining threads creates a happens-before relation between the threads and the main thread

    // post part
    let mut recorder = recorder.record_post_part_with_capacity(scenario.post_part.len());
    for op in scenario.post_part {
        recorder.record(op.clone(), || conc.exec(op));
    }

    recorder.finish() // retrieve the recorded execution
}
