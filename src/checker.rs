use std::collections::HashSet;

use crate::recorder::*;
use crate::spec::*;

pub struct LinearizabilityChecker<Seq: SequentialSpec> {
    execution: Execution<Seq::Op, Seq::Ret>,
    hb_parallel: Vec<Vec<usize>>, // happens-before relation in parallel part
    in_degree: Vec<usize>, // number of invocations that happen-before this invocation in parallel part
    minimal_invocations: HashSet<usize>, // invocations that have in_degree == 0
    linearized_parallel: Vec<usize>,
    seq_spec: Seq,
}

impl<Seq> LinearizabilityChecker<Seq>
where
    Seq: SequentialSpec,
    Seq::Op: Clone,
    Seq::Ret: PartialEq,
{
    pub fn check(execution: Execution<Seq::Op, Seq::Ret>) -> bool {
        let parallel_part = &execution.parallel_part;
        let mut hb_parallel = vec![vec![]; parallel_part.len()];

        for (inv_id_a, inv_a) in parallel_part.iter().enumerate() {
            for (inv_id_b, inv_b) in parallel_part.iter().enumerate() {
                if inv_a.ret_timestamp < inv_b.inv_timestamp {
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
            hb_parallel,
            in_degree,
            minimal_invocations,
            linearized_parallel: Vec::new(),
            seq_spec: Seq::new(),
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
            return false;
        })
    }

    fn check_post_part(&mut self) -> bool {
        self.execution.post_part.iter().all(|inv| {
            let ret = self.seq_spec.exec(inv.op.clone());
            ret == inv.ret
        })
    }

    fn call(&mut self, inv_id: usize) {
        self.linearized_parallel.push(inv_id);
        self.minimal_invocations.remove(&inv_id);
        for &next_inv_id in self.hb_parallel[inv_id].iter() {
            self.in_degree[next_inv_id] -= 1;
            if self.in_degree[next_inv_id] == 0 {
                self.minimal_invocations.insert(next_inv_id);
            }
        }
    }

    fn rebuild_seq_spec(&mut self) {
        self.seq_spec = Seq::new();

        for inv in self.execution.init_part.iter() {
            self.seq_spec.exec(inv.op.clone());
        }
        for &inv_id in self.linearized_parallel.iter() {
            let inv = &self.execution.parallel_part[inv_id];
            self.seq_spec.exec(inv.op.clone());
        }
    }

    fn undo(&mut self, inv_id: usize) {
        for &next_inv_id in self.hb_parallel[inv_id].iter() {
            if self.in_degree[next_inv_id] == 0 {
                self.minimal_invocations.remove(&next_inv_id);
            }
            self.in_degree[next_inv_id] += 1;
        }
        self.minimal_invocations.insert(inv_id);
        self.linearized_parallel.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    impl<T> SequentialSpec for SequentialStack<T> {
        type Op = Op<T>;
        type Ret = Ret<T>;

        fn new() -> Self {
            Self { stack: Vec::new() }
        }

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
            execution
        ));
    }

    #[test]
    fn parallel_part_overlapping() {
        let mut init_recorder = record_init_part();
        init_recorder.record(Op::Push(1), || Ret::Push);
        init_recorder.record(Op::Push(2), || Ret::Push);
        let init_part = init_recorder.finish().init_part;

        let mut recorder_a = InternalRecorder::new(Some(0));
        let mut recorder_b = InternalRecorder::new(Some(1));
        recorder_a.add_invocation(Op::Pop, 4);
        recorder_b.add_invocation(Op::Pop, 5);
        recorder_a.add_completion(Ret::Pop(Some(2)), 6);
        recorder_b.add_completion(Ret::Pop(Some(1)), 7);

        let execution = Execution {
            init_part,
            parallel_part: [recorder_a.invocations(), recorder_b.invocations()].concat(),
            post_part: Vec::new(),
        };

        assert!(LinearizabilityChecker::<SequentialStack<i32>>::check(
            execution
        ));
    }

    #[test]
    fn parallel_part_does_not_violate_happens_before() {
        let mut recorder_a = InternalRecorder::new(Some(0));
        let mut recorder_b = InternalRecorder::new(Some(1));
        recorder_a.add_invocation(Op::Pop, 4);
        recorder_b.add_invocation(Op::Pop, 5);
        recorder_a.add_completion(Ret::Pop(Some(1)), 6);

        recorder_a.add_invocation(Op::Push(1), 7);
        recorder_b.add_completion(Ret::Pop(None), 8);
        recorder_a.add_completion(Ret::Push, 9);

        let execution = Execution {
            init_part: Vec::new(),
            parallel_part: [recorder_a.invocations(), recorder_b.invocations()].concat(),
            post_part: Vec::new(),
        };

        assert!(!LinearizabilityChecker::<SequentialStack<i32>>::check(
            execution
        ));
    }
}
