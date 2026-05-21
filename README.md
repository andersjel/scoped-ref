# Introduction

This crate provides safe references with runtime-checked lifetimes. While a reference is in use, the owner cannot drop the referenced value.

The main entry-point for this library is the [`Scope`] struct.

# Features

This crate has no feature flags.

# Example Use Cases

## Spawning In a [`smol::Executor<'a>`]

A [`smol::Executor<'a>`] is an async executor that owns tasks with a lifetime of `'a`. To spawn a sub-task from a task in a smol executor, you need something like:

```rust
async fn sub_task() {}
async fn main_task<'a>(executor: &'a smol::Executor<'a>) {
    executor.spawn(sub_task()).await
}
```

Unfortunately, it is not possible to create a `&'a Executor<'a>` since `Executor<'a>` is invariant in `'a` and implements [`Drop`]. With this library, you can instead use `WeakRef<'a, Executor<'a>`:

```rust
use scoped_ref::{Scope, WeakRef};

async fn sub_task() -> String {
    smol::Timer::after(std::time::Duration::from_millis(1)).await;
    "Hello, World!".to_string()
}

async fn main_task<'a>(executor: WeakRef<'a, smol::Executor<'a>>) -> Option<String> {
    let task = executor.upgrade()?.spawn(sub_task());
    Some(task.await)
}

let scope = Scope::new();
let executor = smol::Executor::new();
let result = scope
    .assign(&executor, || {
        smol::block_on(executor.run(main_task(scope.weak_ref())))
    })
    .unwrap();

assert_eq!(result.unwrap(), "Hello, World!");
```

# Similar Crates
- The [lien] crate provides a similar construction without weak references but with a more flexible API and `no_std` support.
- The [scoped_reference] crate provides runtime checked references that aborts the program (without first running panic handlers) on violations. This is preferable to deadlocking, especially in single-threaded use cases.

[lien]: https://crates.io/crates/lien
[scoped_reference]: https://crates.io/crates/scoped_reference
[smol]: https://crates.io/crates/smol
[`smol::Executor<'a>`]: https://docs.rs/smol/2.0.2/smol/struct.Executor.html
