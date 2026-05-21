#![warn(clippy::pedantic, clippy::allow_attributes)]
#![doc = include_str!("../README.md")]

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
    /// [`Scope`] and [`WeakRef`] instances can be created statically:
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
    /// This will return [`None`][Option::None] if no value is currently
    /// [assigned][Scope::assign] to this [`Scope`].
    #[must_use]
    pub fn strong_ref(&self) -> Option<StrongRef<'_, T>> {
        self.weak_ref().upgrade()
    }
}

impl<'scope, T: ?Sized> WeakRef<'scope, T> {
    /// Upgrade this [`WeakRef`] to a [`StrongRef`].
    ///
    /// This method returns [`None`][Option::None] unless a value is
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
