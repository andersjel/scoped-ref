//! [![Crates.io Version](https://img.shields.io/crates/v/scoped-ref.svg)](https://crates.io/crates/scoped-ref)
//! [![Docs.rs](https://docs.rs/scoped-ref/badge.svg)](https://docs.rs/scoped-ref)
//! [![CI Status](https://github.com/andersjel/scoped-ref/actions/workflows/ci.yml/badge.svg)](https://github.com/andersjel/scoped-ref/actions/workflows/ci.yml)
//! [![License](https://img.shields.io/crates/l/scoped-ref.svg)](https://github.com/andersjel/scoped-ref/blob/main/LICENSE-MIT)
//!
//! # Introduction
//!
//! This crate provides safe references with runtime-checked lifetimes. While a
//! reference is in use, attempts to drop the referenced value will block.
//!
//! The main entry-point for this library is the [`Scope`] struct.
//!
//! Additional examples are available in the [`examples`] module.
//!
//! # Features
//!
//! Feature | Default | Description
//! --- | --- | ---
//! docs | no | Used internally when building documentation.
//!
//! # Similar Crates
//! - The [lien] crate provides a similar construction without weak references
//!   but with a more flexible API and `no_std` support.
//! - The [scoped_reference] crate provides runtime checked references that
//!   aborts the program (without first running panic handlers) on violations.
//!   This is preferable to deadlocking, especially in single-threaded use
//!   cases.
//!
//! [lien]: https://crates.io/crates/lien
//! [scoped_reference]: https://crates.io/crates/scoped_reference
//! [smol]: https://crates.io/crates/smol
//! [`smol::Executor<'a>`]: https://docs.rs/smol/2.0.2/smol/struct.Executor.html

#![warn(clippy::pedantic, clippy::allow_attributes)]

pub mod examples;

#[cfg(test)]
mod tests;

use std::{
    fmt::{Debug, Display},
    ops::Deref,
    ptr::NonNull,
    sync::{PoisonError, RwLock, RwLockReadGuard},
};

fn ignore_poison<T>(v: Result<T, PoisonError<T>>) -> T {
    v.unwrap_or_else(PoisonError::into_inner)
}

/// Returned when [`Scope::assign()`] is called un an already assigned
/// [`Scope`].
pub struct AssignedError<F> {
    /// The `body` passed to [`Scope::assign()`].
    pub body: F,
}

impl<F> Debug for AssignedError<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssignError").finish_non_exhaustive()
    }
}

/// The display implementation formats as `"Scope already assigned"`.
impl<F> Display for AssignedError<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Scope already assigned")
    }
}

/// Manages the lifetime of scoped references created with this library.
///
/// A [`Scope<T>`] acts effectively like an `Option<&'a T>` (but without the
/// lifetime parameter). When the [`Scope<T>`] contains a reference, it
/// is said to be *assigned*, when it does not it is *unassigned*.
///
/// An assigned [`Scope<T>`] can be used to produce [`StrongRef<T>`] instances
/// that dereference to the contained `&T`. A [`Scope<T>`] cannot become
/// unassigned while [strong references] exist.
///
/// Both unassigned and assigned [`Scope<T>`] instances can be used to produce
/// [`WeakRef<T>`] instances, that can later be [upgraded] to [`StrongRef<T>`]
/// instances.
///
/// To assign a `&T` to a [`Scope<T>`] you use the [`Scope::assign()`] method.
/// The method takes `&T` to assign and *body* to execute. When the *body*
/// returns, and all [`StrongRef<T>`] instances created from the [`Scope<T>`]
/// are dropped, [`Scope::assign()`] returns and the [`Scope<T>`] is again
/// unassigned. This ensures that no dangling use of the `&T` can take place.
///
/// # Example
///
/// ```
/// # use scoped_ref::{StrongRef, Scope};
/// fn read_value(reference: StrongRef<i32>) -> i32 {
///     *reference
/// }
///
/// let scope = Scope::new();
/// let value = 42;
///
/// let result = scope
///     .assign(&value, || read_value(scope.strong_ref().unwrap()))
///     .unwrap();
///
/// assert_eq!(42, result);
/// ```
///
/// [strong references]: StrongRef
/// [upgraded]: WeakRef::upgrade()
#[derive(Default, Debug)]
pub struct Scope<T: ?Sized> {
    value: RwLock<Option<NonNull<T>>>,
}

// Safety:
// A Scope<T> effectively owns a &T. It is Sync and Send if T is Sync.
unsafe impl<T: Sync + ?Sized> Send for Scope<T> {}
unsafe impl<T: Sync + ?Sized> Sync for Scope<T> {}

/// A weak reference obtained from a [`Scope`].
///
/// A weak reference cannot be dereferenced directly, but must first be upgraded
/// to a [`StrongRef`] with [`WeakRef::upgrade()`].
///
/// Note, that the lifetime parameter `'scope` of a [`WeakRef<'scope, T>`] is
/// that of the associated [`Scope<T>`], _not_ whatever `&T` that the
/// [`Scope<T>`] happens to be assigned.
#[derive(Debug, Clone)]
pub struct WeakRef<'scope, T: ?Sized> {
    source: &'scope Scope<T>,
}

/// A strong reference obtained from a [`Scope`].
///
/// A [`StrongRef<'_, T>`] can be [dereferenced][Deref] to a `&T`.
///
/// A [`StrongRef`] to a [`Scope`] can only be created while [`Scope::assign()`]
/// is being executed, and [`Scope::assign()`] will not return until all
/// existing [`StrongRef`] instances have been dropped.
///
/// Note, that the lifetime parameter `'scope` of a [`StrongRef<'scope, T>`] is
/// that of the associated [`Scope<T>`], _not_ the `&T` that the [`Scope<T>`] is
/// assigned.
#[derive(Debug)]
pub struct StrongRef<'scope, T: ?Sized> {
    source: &'scope Scope<T>,
    guard: RwLockReadGuard<'scope, Option<NonNull<T>>>,
}

impl<'scope, T: ?Sized> StrongRef<'scope, T> {
    /// Downgrade to a [`WeakRef`].
    #[must_use]
    pub fn downgrade(&self) -> WeakRef<'scope, T> {
        self.source.weak_ref()
    }
}

impl<T: ?Sized> Clone for StrongRef<'_, T> {
    fn clone(&self) -> Self {
        self.source.strong_ref().unwrap()
    }
}

unsafe impl<T: Sync + ?Sized> Sync for StrongRef<'_, T> {}

impl<T: ?Sized> Scope<T> {
    /// Construct a new [`Scope`].
    ///
    /// This creates an _unassigned_ [`Scope`]; assign a target using
    /// [`Self::assign()`].
    ///
    /// # Note
    /// [`Scope`] and [`WeakRef`] instances can also be created statically:
    ///
    /// ```
    /// # use scoped_ref::*;
    /// static SCOPE: Scope<i32> = Scope::new();
    /// static WEAK_REF: WeakRef<'static, i32> = SCOPE.weak_ref();
    /// let value = SCOPE.assign(&42, || *WEAK_REF.upgrade().unwrap()).unwrap();
    /// assert_eq!(value, 42)
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: RwLock::new(None),
        }
    }

    /// Run `body` with `target` assigned to `self`.
    ///
    /// If `self` is not already assigned a target when this method is called,
    /// then:
    ///
    /// 1. While `body` is executing, [strong references] to `self` will
    ///    dereference to `target`.
    /// 2. When `body` returns, this method will block until there are no
    ///    [strong references] left to `self` before returning.
    /// 3. The value returned by `body` is returned by this method. When this
    ///    method returns, `self` is again considered unassigned.
    ///
    /// This method has no effect if `self` is already assigned a target when
    /// called. In this case, the given `body` is _not_ executed and is instead
    /// returned back to the caller in an [`AssignedError`].
    ///
    /// # Deadlocks
    /// If any [strong references] are leaked, this method will never return.
    ///
    /// # Errors
    /// Will return [`AssignedError`] if `self` is already assigned a target
    /// when called.
    ///
    /// # Notes
    /// Pre-existing [weak references] to `self` will also be affected by this
    /// method.
    ///
    /// ```
    /// let scope = scoped_ref::Scope::new();
    /// let weak_ref = scope.weak_ref();
    /// let sample = || *weak_ref.upgrade().unwrap();
    ///
    /// let value = scope.assign(&42, sample).unwrap();
    /// assert_eq!(value, 42);
    ///
    /// let value = scope.assign(&43, sample).unwrap();
    /// assert_eq!(value, 43);
    /// ```
    ///
    /// [strong references]: StrongRef
    /// [weak references]: WeakRef
    pub fn assign<R, F: FnOnce() -> R>(&self, target: &T, body: F) -> Result<R, AssignedError<F>> {
        {
            let mut guard = ignore_poison(self.value.write());
            if guard.is_some() {
                return Err(AssignedError { body });
            }
            *guard = Some(NonNull::from_ref(target));
        }
        let _clear = EmptyScopeOnDrop { target: self };
        Ok(body())
    }

    /// Construct a [`WeakRef`] to this [`Scope`].
    ///
    /// The returned [`WeakRef`] can be upgraded to a [`StrongRef`] with
    /// [`WeakRef::upgrade`] but only while a value is
    /// [assigned][Scope::assign] to this [`Scope`].
    #[must_use]
    pub const fn weak_ref(&self) -> WeakRef<'_, T> {
        WeakRef { source: self }
    }

    /// Construct a [`StrongRef`] to this [`Scope`].
    ///
    /// This will return [`None`] if no value is currently
    /// [assigned][Scope::assign] to this [`Scope`].
    #[must_use]
    pub fn strong_ref(&self) -> Option<StrongRef<'_, T>> {
        self.weak_ref().upgrade()
    }
}

impl<'scope, T: ?Sized> WeakRef<'scope, T> {
    /// Upgrade this [`WeakRef`] to a [`StrongRef`].
    ///
    /// This method returns [`None`] unless a value is currently
    /// [assigned][Scope::assign] to the associated [`Scope`].
    #[must_use]
    pub fn upgrade(&self) -> Option<StrongRef<'scope, T>> {
        let guard = ignore_poison(self.source.value.read());

        // Never construct a StrongRef containing None.
        if guard.is_none() {
            None
        } else {
            Some(StrongRef {
                source: self.source,
                guard,
            })
        }
    }
}

impl<T: ?Sized> Deref for StrongRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // Safety:
        // - unwrap_unchecked() is safe because we never construct a StrongRef
        //   containing None (see WeakRef::upgrade()).
        // - as_ref() is safe because we do not return from Scope::assign() before
        //   taking a write lock, and this cannot happen while self.guard exists. This
        //   guarantees that the reference is valid until `self` is dropped.
        unsafe { self.guard.unwrap_unchecked().as_ref() }
    }
}

struct EmptyScopeOnDrop<'a, T: ?Sized> {
    target: &'a Scope<T>,
}

impl<T: ?Sized> Drop for EmptyScopeOnDrop<'_, T> {
    fn drop(&mut self) {
        *ignore_poison(self.target.value.write()) = None;
    }
}
