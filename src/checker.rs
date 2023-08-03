//! The module with the linearizability checker implementation.

use std::collections::HashSet;

use crate::execution::*;
use crate::spec::*;

/// The linearizability checker itself.
///
/// Each linearization of a parallel execution is a topolocial ordering of the happens-before graph.
/// So the checker works by constructing the happens-before graph from the parallel execution and
/// brute-forcing all possible topological orderings.
///
/// The checker rebuilds the sequential specification for each considered linearization. But it does it
/// lazily: it only rebuilds the sequential specification when it needs to backtrack and check the other branch.
pub struct LinearizabilityChecker<'e, Seq: SequentialSpec> {
    execution: &'e Execution<Seq::Op, Seq::Ret>,
    hb: Vec<Vec<InvocationId>>, // for each invocation in the parallel part, a list of invocations which it happens-before
    in_degree: Vec<usize>, // for each invocation in the parallel part, number of invocations that happen-before
    minimal_invocations: HashSet<InvocationId>, // invocations in the parallel part that have in_degree == 0
    linearized: Vec<InvocationId>,              // current linearization of the parallel part
    seq_spec: Seq,
}

impl<'e, Seq> LinearizabilityChecker<'e, Seq>
where
    Seq: SequentialSpec,
    Seq::Op: Clone,
    Seq::Ret: PartialEq,
{
    /// Checks if the given execution is linearizable with respect to the given sequential specification `Seq`.
    pub fn check(execution: &'e Execution<Seq::Op, Seq::Ret>) -> bool {
        let parallel_part = &execution.parallel_part;
        let mut hb_parallel = vec![vec![]; parallel_part.len()];

        for (inv_id_a, inv_a) in parallel_part.iter().enumerate() {
            for (inv_id_b, inv_b) in parallel_part.iter().enumerate() {
                if inv_a.return_timestamp < inv_b.call_timestamp {
                    hb_parallel[inv_id_a].push(inv_id_b);
                }
            }
        }

        let mut in_degree = vec![0; parallel_part.len()];
        for hb_per_inv in hb_parallel.iter() {
            for &inv_id in hb_per_inv {
                in_degree[inv_id] += 1;
            }
        }

        let mut minimal_invocations = HashSet::new();
        for (inv_id, &in_degree) in in_degree.iter().enumerate() {
            if in_degree == 0 {
                minimal_invocations.insert(inv_id);
            }
        }

        let mut checker = LinearizabilityChecker {
            execution,
            hb: hb_parallel,
            in_degree,
            minimal_invocations,
            linearized: Vec::new(),
            seq_spec: Seq::default(),
        };

        checker.check_init_part()
    }

    fn check_init_part(&mut self) -> bool {
        self.execution.init_part.iter().all(|inv| {
            let ret = self.seq_spec.exec(inv.op.clone());
            ret == inv.ret
        }) && self.check_parallel_part()
    }

    fn check_parallel_part(&mut self) -> bool {
        if self.minimal_invocations.is_empty() {
            return self.check_post_part();
        };

        self.minimal_invocations.clone().into_iter().any(|inv_id| {
            self.call(inv_id);

            let inv = &self.execution.parallel_part[inv_id];
            let ret = self.seq_spec.exec(inv.op.clone());
            if ret == inv.ret && self.check_parallel_part() {
                return true;
            }

            self.undo(inv_id);
            self.rebuild_seq_spec();
            false
        })
    }

    fn check_post_part(&mut self) -> bool {
        self.execution.post_part.iter().all(|inv| {
            let ret = self.seq_spec.exec(inv.op.clone());
            ret == inv.ret
        })
    }

    fn call(&mut self, inv_id: usize) {
        self.linearized.push(inv_id);
        self.minimal_invocations.remove(&inv_id);
        for &next_inv_id in self.hb[inv_id].iter() {
            self.in_degree[next_inv_id] -= 1;
            if self.in_degree[next_inv_id] == 0 {
                self.minimal_invocations.insert(next_inv_id);
            }
        }
    }

    fn rebuild_seq_spec(&mut self) {
        self.seq_spec = Seq::default();

        for inv in self.execution.init_part.iter() {
            self.seq_spec.exec(inv.op.clone());
        }
        for &inv_id in self.linearized.iter() {
            let inv = &self.execution.parallel_part[inv_id];
            self.seq_spec.exec(inv.op.clone());
        }
    }

    fn undo(&mut self, inv_id: usize) {
        for &next_inv_id in self.hb[inv_id].iter() {
            if self.in_degree[next_inv_id] == 0 {
                self.minimal_invocations.remove(&next_inv_id);
            }
            self.in_degree[next_inv_id] += 1;
        }
        self.minimal_invocations.insert(inv_id);
        self.linearized.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::*;

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

    #[derive(Debug, Clone)]
    struct SequentialStack<T> {
        stack: Vec<T>,
    }

    impl<T> Default for SequentialStack<T> {
        fn default() -> Self {
            Self { stack: Vec::new() }
        }
    }

    impl<T> SequentialSpec for SequentialStack<T> {
        type Op = Op<T>;
        type Ret = Ret<T>;

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

    #[test]
    fn init_and_post_parts_are_sequentional() {
        let mut recorder = record_init_part();
        recorder.record(Op::Push(1), || Ret::Push);
        recorder.record(Op::Push(2), || Ret::Push);

        let mut recorder = recorder.record_post_part();
        recorder.record(Op::Pop, || Ret::Pop(Some(2)));
        recorder.record(Op::Pop, || Ret::Pop(Some(1)));

        let execution = recorder.finish();

        assert!(LinearizabilityChecker::<SequentialStack<i32>>::check(
            &execution
        ));
    }

    #[test]
    fn parallel_part_overlapping() {
        let mut init_recorder = record_init_part();
        init_recorder.record(Op::Push(1), || Ret::Push);
        init_recorder.record(Op::Push(2), || Ret::Push);
        let init_part = init_recorder.finish().init_part;

        let mut recorder_a = InternalRecorder::new(0);
        let mut recorder_b = InternalRecorder::new(1);
        recorder_a.add_call(Op::Pop, 4);
        recorder_b.add_call(Op::Pop, 5);
        recorder_a.add_return(Ret::Pop(Some(2)), 6);
        recorder_b.add_return(Ret::Pop(Some(1)), 7);

        let execution = Execution {
            init_part,
            parallel_part: [
                recorder_a.history().into_inner(),
                recorder_b.history().into_inner(),
            ]
            .concat()
            .into(),
            post_part: History::new(),
        };

        assert!(LinearizabilityChecker::<SequentialStack<i32>>::check(
            &execution
        ));
    }

    #[test]
    fn parallel_part_does_not_violate_happens_before() {
        let mut recorder_a = InternalRecorder::new(0);
        let mut recorder_b = InternalRecorder::new(1);
        recorder_a.add_call(Op::Pop, 4);
        recorder_b.add_call(Op::Pop, 5);
        recorder_a.add_return(Ret::Pop(Some(1)), 6);

        recorder_a.add_call(Op::Push(1), 7);
        recorder_b.add_return(Ret::Pop(None), 8);
        recorder_a.add_return(Ret::Push, 9);

        let execution = Execution {
            init_part: History::new(),
            parallel_part: [
                recorder_a.history().into_inner(),
                recorder_b.history().into_inner(),
            ]
            .concat()
            .into(),
            post_part: History::new(),
        };

        assert!(!LinearizabilityChecker::<SequentialStack<i32>>::check(
            &execution
        ));
    }
}
