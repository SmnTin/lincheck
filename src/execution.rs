use std::ops::{Deref, DerefMut};

pub(crate) type Timestamp = usize;
pub(crate) type ThreadId = usize;
pub(crate) type InvocationId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invocation<Op, Ret> {
    pub(crate) op: Op,
    pub(crate) ret: Ret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParallelInvocation<Op, Ret> {
    pub(crate) thread_id: ThreadId,
    pub(crate) call_timestamp: Timestamp,
    pub(crate) return_timestamp: Timestamp,

    pub(crate) op: Op,
    pub(crate) ret: Ret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct History<Op, Ret> {
    inner: Vec<Invocation<Op, Ret>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParallelHistory<Op, Ret> {
    inner: Vec<ParallelInvocation<Op, Ret>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution<Op, Ret> {
    pub(crate) init_part: History<Op, Ret>,
    pub(crate) parallel_part: ParallelHistory<Op, Ret>,
    pub(crate) post_part: History<Op, Ret>,
}

impl<Op, Ret> ParallelHistory<Op, Ret> {
    pub fn get_thread_parts(&self) -> Vec<Vec<&ParallelInvocation<Op, Ret>>> {
        let mut thread_parts = Vec::new();
        for inv in &self.inner {
            if thread_parts.len() <= inv.thread_id {
                thread_parts.resize_with(inv.thread_id + 1, Vec::new);
            }
            thread_parts[inv.thread_id].push(inv);
        }
        thread_parts
    }
}

// The rest of the file consists of boilerplate trait implementations

impl<Op, Ret> History<Op, Ret> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Vec::with_capacity(cap),
        }
    }

    pub fn into_inner(self) -> Vec<Invocation<Op, Ret>> {
        self.inner
    }
}

impl<Op, Ret> ParallelHistory<Op, Ret> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Vec::with_capacity(cap),
        }
    }

    pub fn into_inner(self) -> Vec<ParallelInvocation<Op, Ret>> {
        self.inner
    }
}

impl<Op, Ret> Deref for History<Op, Ret> {
    type Target = Vec<Invocation<Op, Ret>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Op, Ret> DerefMut for History<Op, Ret> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<Op, Ret> Deref for ParallelHistory<Op, Ret> {
    type Target = Vec<ParallelInvocation<Op, Ret>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Op, Ret> DerefMut for ParallelHistory<Op, Ret> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<Op, Ret> IntoIterator for History<Op, Ret> {
    type Item = Invocation<Op, Ret>;
    type IntoIter = <Vec<Invocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, Op, Ret> IntoIterator for &'a History<Op, Ret> {
    type Item = &'a Invocation<Op, Ret>;
    type IntoIter = <&'a Vec<Invocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, Op, Ret> IntoIterator for &'a mut History<Op, Ret> {
    type Item = &'a mut Invocation<Op, Ret>;
    type IntoIter = <&'a mut Vec<Invocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

impl<Op, Ret> IntoIterator for ParallelHistory<Op, Ret> {
    type Item = ParallelInvocation<Op, Ret>;
    type IntoIter = <Vec<ParallelInvocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, Op, Ret> IntoIterator for &'a ParallelHistory<Op, Ret> {
    type Item = &'a ParallelInvocation<Op, Ret>;
    type IntoIter = <&'a Vec<ParallelInvocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, Op, Ret> IntoIterator for &'a mut ParallelHistory<Op, Ret> {
    type Item = &'a mut ParallelInvocation<Op, Ret>;
    type IntoIter = <&'a mut Vec<ParallelInvocation<Op, Ret>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

impl<Op, Ret> Default for History<Op, Ret> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Op, Ret> Default for ParallelHistory<Op, Ret> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Op, Ret> From<Vec<Invocation<Op, Ret>>> for History<Op, Ret> {
    fn from(inner: Vec<Invocation<Op, Ret>>) -> Self {
        Self { inner }
    }
}

impl<Op, Ret> From<Vec<ParallelInvocation<Op, Ret>>> for ParallelHistory<Op, Ret> {
    fn from(inner: Vec<ParallelInvocation<Op, Ret>>) -> Self {
        Self { inner }
    }
}

impl<Op, Ret> From<History<Op, Ret>> for Vec<Invocation<Op, Ret>> {
    fn from(history: History<Op, Ret>) -> Self {
        history.inner
    }
}

impl<Op, Ret> From<ParallelHistory<Op, Ret>> for Vec<ParallelInvocation<Op, Ret>> {
    fn from(history: ParallelHistory<Op, Ret>) -> Self {
        history.inner
    }
}

impl<Op, Ret> Default for Execution<Op, Ret> {
    fn default() -> Self {
        Self {
            init_part: History::default(),
            parallel_part: ParallelHistory::default(),
            post_part: History::default(),
        }
    }
}
