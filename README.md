[![Rust CI](https://github.com/SmnTin/lincheck/actions/workflows/general.yml/badge.svg)](https://github.com/SmnTin/lincheck/actions/workflows/general.yml)

# Lincheck

Lincheck is a Rust library for testing concurrent data structures for [linearizability](https://en.wikipedia.org/wiki/Linearizability). Simply put, it checks whether a concurrent data structure behaves similarly to a simpler sequential implementation. It is inspired by [Lincheck for Kotlin](https://github.com/JetBrains/lincheck) and is built on top of [loom](https://github.com/tokio-rs/loom), a model-checker for concurrency.

## Features

- Lincheck uses [proptest](https://docs.rs/proptest/latest/proptest/) to generate random concurrent scenarios and automatically shrink them to a minimal failing scenario.
- Lincheck runs every scenario inside [loom](https://github.com/tokio-rs/loom) model-checker to check every possible interleaving of operations.
- Lincheck provides a simple API for defining concurrent data structures and their sequential counterparts.
- Recording of execution traces is made to introduce as little additional synchronization between threads as possible.

## Tutorial

For this tutorial we will use the following:
```rust
use lincheck::{ConcurrentSpec, Lincheck, SequentialSpec};
use loom::sync::atomic::{AtomicBool, Ordering};
use proptest::prelude::*;
```

Let's implement a simple concurrent data structure: a pair of boolean flags `x` and `y` that can be read and written by multiple threads. The flags are initialized to `false` and can be switched to `true`.

We start by defining the operations and their results:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    WriteX, // set x to true
    WriteY, // set y to true
    ReadX, // get the value of x
    ReadY, // get the value of y
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ret {
    Write, // the result of a write operation
    Read(bool), // the result of a read operation
}
```

We need to implement the `Arbitrary` trait for our operations to be able to generate them randomly:
```rust
impl Arbitrary for Op {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Op::WriteX),
            Just(Op::WriteY),
            Just(Op::ReadX),
            Just(Op::ReadY),
        ]
        .boxed()
    }
}
```

We then define the concurrent implementation that we want to test:
```rust
struct TwoSlotsParallel {
    x: AtomicBool,
    y: AtomicBool,
}
```

We implement the `ConcurrentSpec` trait for our implementation:
```rust
impl ConcurrentSpec for TwoSlotsParallel {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self {
            x: AtomicBool::new(false),
            y: AtomicBool::new(false),
        }
    }

    fn exec(&self, op: Op) -> Ret {
        match op {
            Op::WriteX => {
                self.x.store(true, Ordering::Relaxed);
                Ret::Write
            }
            Op::WriteY => {
                self.y.store(true, Ordering::Relaxed);
                Ret::Write
            }
            Op::ReadX => Ret::Read(self.x.load(Ordering::Relaxed)),
            Op::ReadY => Ret::Read(self.y.load(Ordering::Relaxed)),
        }
    }
}
```
We must be able to create a new instance of our implementation and execute an operation on it. The `exec` method should not panic.

We then define the sequential implementation which we test against:
```rust
struct TwoSlotsSequential {
    x: bool,
    y: bool,
}
```

We implement the `SequentialSpec` trait for our implementation:
```rust
impl SequentialSpec for TwoSlotsSequential {
    type Op = Op;
    type Ret = Ret;

    fn new() -> Self {
        Self { x: false, y: false }
    }

    fn exec(&mut self, op: Op) -> Ret {
        match op {
            Op::WriteX => {
                self.x = true;
                Ret::Write
            }
            Op::WriteY => {
                self.y = true;
                Ret::Write
            }
            Op::ReadX => Ret::Read(self.x),
            Op::ReadY => Ret::Read(self.y),
        }
    }
}
```

Notice that the concurrent specification receives a shared reference to itself (`&self`) while the sequential specification receives an exclusive reference to itself (`&mut self`). This is because the concurrent specification is shared between threads while the sequential specification is not.

We are now ready to write our test:
```rust
#[test]
fn two_slots() {
    Lincheck {
        num_threads: 2,
        num_ops: 5,
    }.verify::<TwoSlotsParallel, TwoSlotsSequential>();
}
```

If we run the test, we get a failure along with a trace of the execution:
```
running 1 test
test two_slots ... FAILED

failures:

---- two_slots stdout ----
thread 'two_slots' panicked at 'Non-linearizable execution: 

 INIT PART:
|================|
|  MAIN THREAD   |
|================|
|                |
| WriteX : Write |
|                |
|----------------|

PARALLEL PART:
|=====================|================|
|      THREAD 0       |    THREAD 1    |
|=====================|================|
|                     |                |
|                     |----------------|
|                     |                |
| ReadY : Read(false) | WriteY : Write |
|                     |                |
|                     |----------------|
|                     |                |
|---------------------|                |
|                     |                |
| ReadY : Read(false) |                |
|                     |                |
|---------------------|----------------|

POST PART:
|================|
|  MAIN THREAD   |
|================|
|                |
| WriteX : Write |
|                |
|----------------|
```

## Limitations

- Lincheck runner sets its own panic hook. This doesn't play well with parallel test execution. To fix this, you can run your tests with the `--test-threads=1` flag like this:
```bash
$ cargo test -- --test-threads=1
```
- [loom](https://github.com/tokio-rs/loom) can't model all weak memory models effects. This means that some executions that may arise on the real hardware may not be explored by loom. This is why the concurrent data structures should be additionally fuzzed on the real hardware. The support for fuzzing in Lincheck is planned.
- [proptest](https://docs.rs/proptest/latest/proptest/) only explores a random sample of all possible scenarios. This means that some failing executions may not be explored.

## License

Lincheck is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Lincheck by you, shall be licensed as MIT, without any additional terms or conditions.