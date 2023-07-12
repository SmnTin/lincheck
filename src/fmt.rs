use std::fmt::{self, Debug, Display, Formatter};

use crate::recorder::*;

impl<Op, Ret> Display for Invocation<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} : {:?}", self.op, self.ret)
    }
}

impl<Op, Ret> Display for ParallelInvocation<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} : {:?}", self.op, self.ret)
    }
}

impl<Op, Ret> Display for Execution<Op, Ret>
where
    Op: Debug,
    Ret: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "init_part:")?;
        write!(f, "[")?;
        for inv in &self.init_part {
            write!(f, "{}, ", inv)?;
        }
        writeln!(f, "]")?;
        writeln!(f, "parallel_part:")?;

        let thread_parts = self.get_thread_parts();
        for (thread_id, thread_part) in thread_parts.into_iter().enumerate() {
            write!(f, "thread {}: ", thread_id)?;
            write!(f, "[")?;
            for inv in thread_part {
                write!(f, "{}, ", inv)?;
            }
            writeln!(f, "]")?;
        }

        writeln!(f, "post_part:")?;
        write!(f, "[")?;
        for inv in &self.post_part {
            write!(f, "{}, ", inv)?;
        }
        writeln!(f, "]")?;
        Ok(())
    }
}
