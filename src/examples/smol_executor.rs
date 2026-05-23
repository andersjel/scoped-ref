//! # Spawning In a [`smol::Executor<'a>`]
//!
//! A [`smol::Executor<'a>`] is an async executor that owns tasks with a
//! lifetime of `'a`. To spawn a sub-task from a task in a smol executor, you
//! need something like:
//!
//! ```rust
//! # async fn sub_task() {}
//! async fn main_task<'a>(executor: &'a smol::Executor<'a>) {
//!     executor.spawn(sub_task()).await
//! }
//! ```
//!
//! Unfortunately, it is not possible to create a [`&'a
//! Executor<'a>`](smol::Executor) since [`Executor<'a>`][smol::Executor] is
//! invariant in `'a` and implements [`Drop`]. With this library, you can
//! instead use a [`WeakRef<'a, Executor<'a>>`][crate::WeakRef]. This weak
//! reference can be used to spawn tasks with the following properties:
//!
//! - To spawn a task, the weak reference first has to be upgraded to a
//!   [`StrongRef`][crate::StrongRef]. This [`StrongRef`][crate::StrongRef]
//!   keeps the reference to the executor alive while spawning.
//! - While [dropping][Drop::drop] the executor, and after the executor has been
//!   dropped, the weak reference can not be [upgraded][crate::WeakRef::upgrade]
//!   to a [`StrongRef`][crate::StrongRef], and thus cannot be used to spawn
//!   tasks.
//!
//! ```rust
//! use scoped_ref::{Scope, WeakRef};
//!
//! # async fn sub_task() -> String {
//! #     smol::Timer::after(std::time::Duration::from_millis(1)).await;
//! #     "Hello, World!".to_string()
//! # }
//! async fn main_task<'a>(executor: WeakRef<'a, smol::Executor<'a>>) -> Option<String> {
//!     let task = executor.upgrade()?.spawn(sub_task());
//!     Some(task.await)
//! }
//!
//! let scope = Scope::new();
//! let executor = smol::Executor::new();
//! let result = scope
//!     .assign(&executor, || {
//!         smol::block_on(executor.run(main_task(scope.weak_ref())))
//!     })
//!     .unwrap();
//!
//! assert_eq!(result.unwrap(), "Hello, World!");
//! ```
