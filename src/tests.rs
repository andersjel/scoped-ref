use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::Barrier,
    thread::{self, yield_now},
    time::Duration,
};

use crate::{Scope, WeakRef};

#[derive(Default)]
struct InvariantWithDrop<'a> {
    _data: PhantomData<UnsafeCell<&'a mut ()>>,
}

impl InvariantWithDrop<'_> {
    #[expect(clippy::unused_self)]
    fn foo(&self) {}
}

impl Drop for InvariantWithDrop<'_> {
    fn drop(&mut self) {}
}

#[test]
fn simple() {
    let scope = Scope::new();
    let reference = scope.weak_ref();
    let v = scope.assign(&42, || *reference.upgrade().unwrap()).unwrap();
    assert_eq!(v, 42);
}

#[test]
fn static_simple() {
    static SCOPE: Scope<isize> = Scope::new();
    static REFERENCE: WeakRef<'static, isize> = SCOPE.weak_ref();
    let v = SCOPE.assign(&42, || *REFERENCE.upgrade().unwrap()).unwrap();
    assert_eq!(v, 42);
}

#[test]
fn ref_to_invariant() {
    #[expect(clippy::needless_pass_by_value)]
    fn g<'a>(mock: WeakRef<'a, InvariantWithDrop<'a>>) {
        mock.upgrade().unwrap().foo();
    }

    let scope = Scope::new();
    let mock: InvariantWithDrop = InvariantWithDrop::default();

    scope.assign(&mock, || g(scope.weak_ref())).unwrap();
}

#[test]
fn in_another_thread() {
    let barrier = Barrier::new(2);
    let scope = Scope::new();
    let reference = scope.weak_ref();
    thread::scope(|thread_scope| {
        assert!(reference.upgrade().is_none());
        let thread = thread_scope.spawn(|| scope.assign(&42, || barrier.wait()).unwrap());
        while reference.upgrade().is_none() {
            yield_now();
        }
        barrier.wait();
        thread.join().unwrap();
        while reference.upgrade().is_some() {
            yield_now();
        }
    });
}

async fn sub_task() -> String {
    smol::Timer::after(Duration::from_millis(1)).await;
    "Hello, World!".to_string()
}

async fn main_task<'a>(executor: WeakRef<'a, smol::Executor<'a>>) -> Option<String> {
    let task = executor.upgrade()?.spawn(sub_task());
    Some(task.await)
}

#[test]
fn smol_executor() {
    let scope = Scope::new();
    let executor = smol::Executor::new();
    let result = scope
        .assign(&executor, || {
            smol::block_on(executor.run(main_task(scope.weak_ref())))
        })
        .unwrap();

    assert_eq!(result.unwrap(), "Hello, World!");
}
