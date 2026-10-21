//! A `Box` with weak references.
//!
//! A [`RefBox`] is a smart pointer that owns the data, just like a standard
//! [`Box`]. Similarly, a RefBox cannot be cloned cheaply, and when it is
//! dropped, the data it points to is dropped as well. However, a RefBox may
//! have many [`Weak`] pointers to the same data. These pointers don't own the
//! data and are reference counted, comparable to the standard library's
//! [`std::rc::Weak`]. As long as the RefBox is alive, Weak pointers can be used to
//! access the data from multiple places without lifetime parameters.
//!
//! A RefBox could be seen as a lighter alternative to the standard library's
//! [`Rc`], [`std::rc::Weak`] and [`RefCell`] combination, in cases where there is one
//! Rc with many Weak pointers to the same data.
//!
//! Note: this crate is currently **experimental**.
//!
//! [`Rc`]: std::rc::Rc
//! [`RefCell`]: std::cell::RefCell
//!
//! ## Tradeoffs
//!
//! A RefBox does not differentiate between strong and weak pointers. There is
//! always a *single* strong pointer and zero, one or many weak pointers.
//! Like a `RefCell`, the data can either be borrowed mutably once or
//! immutably many times. In return, RefBox uses less memory, is faster to
//! borrow from, and a Weak does not need to be upgraded to access the data.
//!
//! # Rc + Refcell vs. RefBox
//!
//! |                  | `Rc<RefCell<T>>`                                               | `RefBox<T>`                                     |
//! |------------------|----------------------------------------------------------------|-------------------------------------------------|
//! | Pointer kinds    | Many `Rc` pointers and many `Weak` pointers                    | One `RefBox` pointer and many `Weak` pointers   |
//! | Clonable         | Both `Rc` and `Weak` are cheap to clone                        | Only `Weak` is cheap to clone                   |
//! | Up-/Downgrading  | `Rc` is downgradable, `Weak` is upgradable                     | `RefBox` is downgradable                        |
//! | Data access through strong pointer | `RefCell::try_borrow_mut`                    | `RefBox::try_borrow_mut`                        |
//! | Data access through weak pointer | 1. `Weak::upgrade`<br>2. `RefCell::try_borrow_mut`<br>3. Drop temporary `Rc` | `Weak::try_borrow_mut` |
//! | Simultaneous borrows | One mutable OR multiple immutable                          | One mutable OR multiple immutable               |
//! | `T::drop` happens when | When all `Rc`s are dropped                               | When the single `RefBox` is dropped             |
//! | Max number of `Weak` pointers | `usize::MAX`                                      | `u32::MAX`                                      |
//! | Heap overhead | 64-bit: 24 bytes<br>32-bit: 12 bytes | 12 bytes<br>With cyclic_stable enabled on 64-bit: 32 bytes<br>With cyclic_stable enabled on 32-bit: 20 bytes |
//! | Performance      | Cloning is fast, mutating is slow                        | Cloning is a tiny bit slower, mutating is much faster |
//!
//! # Examples
//!
//! ```
//! use refbox::RefBox;
//!
//! fn main() {
//!     // Create a RefBox.
//!     let ref_box = RefBox::new(100);
//!
//!     // Create a weak reference.
//!     let weak = RefBox::downgrade(&ref_box);
//!
//!     // Access the data.
//!     let borrow = weak.try_borrow_mut().unwrap();
//!     assert_eq!(*borrow, 100);
//! }
//! ```
//!
//! # Optional Features
//!
//! * **cyclic_stable**: Enables the `RefBox::new_cyclic()` method on the stable release channel
//!   of Rust. This allows you to create data structures that contain weak references to (parts of)
//!   themselves in one go. To make it work, the memory layout of the type `T` is saved in the heap
//!   part of the `RefBox`. This increases the memory size of the heap part with `2 * usize`.
//! * **cyclic**: Enables the `RefBox::new_cyclic()` method on the nightly release channel without
//!   increasing the memory size of the heap part. This allows you to create data structures that
//!   contain weak references to (parts of) themselves in one go. Requires the nightly feature
//!   [layout_for_ptr].
//!
//! [layout_for_ptr]: https://github.com/rust-lang/rust/issues/69835

#![cfg_attr(feature = "cyclic", feature(layout_for_ptr))]

mod internals;

use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use std::error;

use internals::{RefBoxHeap, RefBoxHeapInner, Status, WeakCount};

/// Coerces a `RefBox<T>` into a `RefBox<dyn Trait>` on stable Rust.
///
/// Normally, performing custom coercions requires the [`CoerceUnsized`] trait
/// which is only available on Nightly Rust. This macro bypasses this trait by
/// performing the actual coercion in the raw pointer domain, which is
/// perfectly possible on Stable Rust.
///
/// # Examples
///
/// ```
/// use std::any::Any;
/// use refbox::{coerce, RefBox};
///
/// struct SomeStruct;
/// trait SomeTrait {}
/// impl SomeTrait for SomeStruct {}
///
/// let ref_box = RefBox::new(SomeStruct);
///
/// let dyn_ref_box = coerce!(ref_box => dyn SomeTrait);
///
/// ```
///
/// [`CoerceUnsized`]: std::ops::CoerceUnsized
#[macro_export]
macro_rules! coerce {
    ($ref_box:expr => $into_type:ty) => {{
        let __raw = $crate::RefBox::into_raw($ref_box);
        let __out: $crate::RefBox<$into_type> = unsafe { $crate::RefBox::from_raw(__raw) };
        __out
    }};
}

/// Coerces a `Weak<T>` into a `Weak<dyn Trait>` on stable Rust.
///
/// Normally, performing custom coercions requires the [`CoerceUnsized`] trait
/// which is only available on Nightly Rust. This macro bypasses this trait by
/// performing the actual coercion in the raw pointer domain, which is
/// perfectly possible on Stable Rust.
///
/// # Examples
///
/// ```
/// use std::any::Any;
/// use refbox::{coerce_weak, RefBox, Weak};
///
/// struct SomeStruct;
/// trait SomeTrait {}
/// impl SomeTrait for SomeStruct {}
///
/// let ref_box = RefBox::new(SomeStruct);
/// let weak_box = RefBox::downgrade(&ref_box);
///
/// let dyn_weak = coerce_weak!(weak_box => dyn SomeTrait);
///
/// ```
///
/// [`CoerceUnsized`]: std::ops::CoerceUnsized
#[macro_export]
macro_rules! coerce_weak {
    ($weak:expr => $into_type:ty) => {{
        let __raw = $crate::Weak::into_raw($weak);
        let __out: $crate::Weak<$into_type> = unsafe { $crate::Weak::from_raw(__raw) };
        __out
    }};
}

/// A smart pointer with many reference-counted weak references.
///
/// See the [module](crate) documentation for more information.
///
/// # Accessing the data behind the RefBox
///
/// See [`RefBox::try_borrow`], [`RefBox::try_borrow_mut`],
/// [`RefBox::try_borrow_mut_or_else`] and [`RefBox::try_access_mut`].
///
/// Note: like a `RefCell`, either one mutable borrow or many immutable borrows
/// can be active at a time.
pub struct RefBox<T: ?Sized> {
    ptr: NonNull<RefBoxHeap<T>>,
    /// A RefBox owns the data and may call `T::drop`.
    _p: PhantomData<RefBoxHeap<T>>,
}

impl<T: ?Sized> Drop for RefBox<T> {
    fn drop(&mut self) {
        unsafe { internals::drop_ref_box(self.ptr) };
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RefBox<T> {
    /// Tries to access the data and write a debug representation of it.
    /// If accessing fails, it only writes the pointer value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.try_borrow() {
            Ok(this) => f
                .debug_tuple("RefBox")
                .field(&self.ptr)
                .field(&&*this)
                .finish(),
            Err(_) => f.debug_tuple("RefBox").field(&self.ptr).finish(),
        }
    }
}

impl<T: ?Sized> PartialEq<Weak<T>> for RefBox<T> {
    fn eq(&self, other: &Weak<T>) -> bool {
        other.is(self)
    }
}

impl<T: Default> Default for RefBox<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for RefBox<T> {
    #[inline]
    fn from(val: T) -> Self {
        Self::new(val)
    }
}

impl<T> RefBox<T> {
    /// Creates a new `RefBox` reference counted pointer.
    pub fn new(value: T) -> Self {
        internals::new_ref_box(value)
    }

    /// Creates a new `RefBox` pointer through a closure which receives a
    /// [`Weak`] pointer to the same data. Use this to create data structures
    /// that contain weak references to themselves.
    ///
    /// Note: if you try to borrow the data in the closure, you will get a
    /// [`BorrowError::Dropped`] error.
    ///
    /// Note: only available if either the `cyclic` or `cyclic_stable` feature is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use refbox::{Weak, RefBox};
    ///
    /// struct Node {
    ///     parent: Weak<Node>,
    ///     child: Option<RefBox<Node>>,
    /// }
    ///
    /// let node = RefBox::new_cyclic(|me| {
    ///     let child = RefBox::new(Node {
    ///         parent: me.clone(),
    ///         child: None,
    ///     });
    ///
    ///     Node {
    ///         parent: Weak::new(),
    ///         child: Some(child),
    ///     }
    /// });
    /// ```
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    pub fn new_cyclic<F: FnOnce(&Weak<T>) -> T>(op: F) -> Self {
        internals::new_cyclic_ref_box(op)
    }
}

impl<T: ?Sized> RefBox<T> {
    /// Returns a read-only reference to the heap part of the `RefBox`.
    #[inline(always)]
    fn heap(&self) -> &RefBoxHeapInner {
        let ptr = self.ptr.as_ptr();
        unsafe { &(*ptr).inner }
    }

    /// Tries to borrow the data mutably.
    ///
    /// # Returns
    ///
    /// * `Ok(Borrow)` if the borrow was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already borrowed
    #[inline]
    pub fn try_borrow_mut(&self) -> Result<Borrow<'_, T>, BorrowError> {
        self.try_borrow_mut_or_else(|| BorrowError::Borrowed)
    }

    /// Tries to borrow the data mutably and returns a custom error if
    /// borrowing fails.
    pub fn try_borrow_mut_or_else<E>(
        &self,
        err_borrowed: impl FnOnce() -> E,
    ) -> Result<Borrow<'_, T>, E> {
        match self.heap().status() {
            Status::Available => Ok(unsafe { Borrow::new(self.ptr.as_ref()) }),
            Status::Borrowed | Status::Shared(_) => Err(err_borrowed()),
            Status::Dropped | Status::DroppedWhileBorrowed | Status::DroppedWhileShared(_) => {
                unreachable!()
            }
        }
    }

    /// Tries to borrow the data immutably.
    ///
    /// Any number of immutable borrows may be active at the same time, but
    /// not together with a mutable borrow.
    ///
    /// # Returns
    ///
    /// * `Ok(Ref)` if the borrow was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already mutably borrowed
    pub fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError> {
        match self.heap().status() {
            Status::Available | Status::Shared(_) => Ok(unsafe { Ref::new(self.ptr.as_ref()) }),
            Status::Borrowed => Err(BorrowError::Borrowed),
            Status::Dropped | Status::DroppedWhileBorrowed | Status::DroppedWhileShared(_) => {
                unreachable!()
            }
        }
    }

    /// Provides access to the data through a closure.
    ///
    /// If the data is already borrowed, the closure is not executed and an
    /// error is returned. Otherwise, the closure is executed and the output of
    /// the closure is returned.
    ///
    /// # Returns
    ///
    /// * `Ok(R)` if the access was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already borrowed
    pub fn try_access_mut<R, F: FnOnce(&mut T) -> R>(&self, op: F) -> Result<R, BorrowError> {
        let mut borrow = self.try_borrow_mut()?;
        Ok(op(&mut *borrow))
    }

    /// Creates a [`Weak`] reference to the data.
    ///
    /// # Panics
    ///
    /// Panics if the number of Refs overflows `u32::MAX`.
    ///
    /// See [`Self::try_downgrade`] for a version that does not panic.
    pub fn downgrade(&self) -> Weak<T> {
        self.heap().increase_weak_count();
        Weak { ptr: self.ptr }
    }

    /// Tries to create a [`Weak`] pointer to the data.
    ///
    /// # Returns
    ///
    /// * `Some(Weak)` if it was successful.
    /// * `None` if the number of weak pointers overflowed `u32::MAX`.
    pub fn try_downgrade(&self) -> Option<Weak<T>> {
        if self.heap().try_increase_weak_count() {
            Some(Weak { ptr: self.ptr })
        } else {
            None
        }
    }

    /// Returns the number of [`Weak`]s pointing to this RefBox.
    pub fn weak_count(&self) -> WeakCount {
        self.heap().weak_count()
    }

    /// Returns a raw pointer to `T`.
    pub fn as_ptr(&self) -> *const T {
        let ptr = self.ptr.as_ptr();

        unsafe { &raw const (*ptr).data as *const T }
    }

    /// Turns the `RefBox` into a raw pointer.
    pub fn into_raw(self) -> *mut RefBoxHeap<T> {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
    }

    /// Creates a `RefBox` from a raw pointer.
    ///
    /// # Safety
    ///
    /// Ensure `ptr` is a valid pointer to a `RefBoxHeap<T>` instance.
    pub unsafe fn from_raw(ptr: *mut RefBoxHeap<T>) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _p: PhantomData,
        }
    }

    /// Casts a `RefBox<T>` to a `RefBox<U>`.
    ///
    /// # Safety
    ///
    /// Ensure `T` can be safely cast to `U`.
    pub unsafe fn cast<U>(self) -> RefBox<U> {
        let raw_ptr = self.into_raw();

        unsafe { RefBox::from_raw(raw_ptr as _) }
    }

    /// Returns the current status of the heap part for testing purposes.
    #[cfg(test)]
    fn status(&self) -> Status {
        self.heap().status()
    }
}

/// A weak reference-counted reference to a [`RefBox`].
///
/// [`Weak::new`] and [`Default`] create empty handles without allocating.
/// Empty handles can be cloned and dropped and report [`BorrowError::Dropped`]
/// when borrowed. Use [`Weak::is_empty`] to distinguish them from expired handles.
///
/// See the [module](crate) documentation for more information.
///
/// # Accessing the data behind the Weak
///
/// See [`Weak::try_borrow`], [`Weak::try_borrow_mut`],
/// [`Weak::try_borrow_mut_or_else`] and [`Weak::try_access_mut`].
///
/// Note: like a `RefCell`, either one mutable borrow or many immutable borrows
/// can be active at a time.
pub struct Weak<T: ?Sized> {
    ptr: NonNull<RefBoxHeap<T>>,
}

impl<T: ?Sized> Drop for Weak<T> {
    fn drop(&mut self) {
        if !self.is_empty() {
            unsafe { internals::drop_weak(self.ptr) };
        }
    }
}

impl<T: ?Sized> Clone for Weak<T> {
    /// Copies the reference and increases the reference counter.
    ///
    /// # Panics
    ///
    /// Panics if the number of Refs overflows `u32::MAX`.
    fn clone(&self) -> Self {
        if let Some(heap) = self.heap() {
            heap.increase_weak_count();
        }
        Weak { ptr: self.ptr }
    }
}

impl<T: ?Sized> fmt::Debug for Weak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Weak").field(&self.ptr).finish()
    }
}

impl<T: ?Sized> PartialEq for Weak<T> {
    /// Returns true if both `Weak` pointers point to the same instance.
    ///
    /// This compares the pointer addresses, not the actual objects themselves,
    /// which makes it very fast. Two empty weak references compare equal.
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(self.ptr.as_ptr(), other.ptr.as_ptr())
    }
}

impl<T: ?Sized> PartialEq<RefBox<T>> for Weak<T> {
    fn eq(&self, other: &RefBox<T>) -> bool {
        self.is(other)
    }
}

impl<T: ?Sized> Eq for Weak<T> {}

impl<T> Default for Weak<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Weak<T> {
    /// Creates an empty weak reference without allocating memory.
    ///
    /// Borrowing it returns [`BorrowError::Dropped`]. Like [`std::rc::Weak::new`],
    /// this constructor requires a sized `T`; [`coerce_weak!`] can then unsize it.
    ///
    /// ```
    /// use refbox::{BorrowError, Weak};
    /// let weak = Weak::<String>::new();
    /// assert!(!weak.is_alive());
    /// assert!(matches!(weak.try_borrow_mut(), Err(BorrowError::Dropped)));
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::without_provenance(core::num::NonZeroUsize::MAX),
        }
    }
}

impl<T: ?Sized> Weak<T> {
    /// Returns true if this weak reference has no allocation.
    ///
    /// A weak reference whose owner was dropped is not empty: it retains
    /// its allocation and identity until the last weak reference is dropped.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ptr.as_ptr().cast::<()>().addr() == usize::MAX
    }

    /// Returns a read-only reference to the heap part of the `Weak`.
    #[inline(always)]
    fn heap(&self) -> Option<&RefBoxHeapInner> {
        if self.is_empty() {
            return None;
        }
        let ptr = self.ptr.as_ptr();
        Some(unsafe { &(*ptr).inner })
    }

    /// Tries to borrow the data mutably.
    ///
    /// # Returns
    ///
    /// * `Ok(Borrow)` if the borrow was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already borrowed
    /// * `Err(BorrowError::Dropped)` if empty or the owning [`RefBox`] was dropped
    #[inline]
    pub fn try_borrow_mut(&self) -> Result<Borrow<'_, T>, BorrowError> {
        self.try_borrow_mut_or_else(|| BorrowError::Borrowed, || BorrowError::Dropped)
    }

    /// Tries to borrow the data mutably and returns a custom error if
    /// borrowing fails.
    pub fn try_borrow_mut_or_else<E>(
        &self,
        err_borrowed: impl FnOnce() -> E,
        err_dropped: impl FnOnce() -> E,
    ) -> Result<Borrow<'_, T>, E> {
        match self.heap().map_or(Status::Dropped, RefBoxHeapInner::status) {
            Status::Available => Ok(unsafe { Borrow::new(self.ptr.as_ref()) }),
            Status::Borrowed | Status::Shared(_) => Err(err_borrowed()),
            Status::Dropped | Status::DroppedWhileBorrowed | Status::DroppedWhileShared(_) => {
                Err(err_dropped())
            }
        }
    }

    /// Tries to borrow the data immutably.
    ///
    /// Any number of immutable borrows may be active at the same time, but
    /// not together with a mutable borrow.
    ///
    /// # Returns
    ///
    /// * `Ok(Ref)` if the borrow was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already mutably borrowed
    /// * `Err(BorrowError::Dropped)` if empty or the owning [`RefBox`] was dropped
    pub fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError> {
        match self.heap().map_or(Status::Dropped, RefBoxHeapInner::status) {
            Status::Available | Status::Shared(_) => Ok(unsafe { Ref::new(self.ptr.as_ref()) }),
            Status::Borrowed => Err(BorrowError::Borrowed),
            Status::Dropped | Status::DroppedWhileBorrowed | Status::DroppedWhileShared(_) => {
                Err(BorrowError::Dropped)
            }
        }
    }

    /// Provides access to the data through a closure.
    ///
    /// If the data is already borrowed or the owning [`RefBox`] is dropped,
    /// the closure is not executed and an error is returned. Otherwise, the
    /// closure is executed and the output of the closure is returned.
    ///
    /// # Returns
    ///
    /// * `Ok(R)` if the access was successful
    /// * `Err(BorrowError::Borrowed)` if the data was already borrowed
    /// * `Err(BorrowError::Dropped)` if empty or the owning [`RefBox`] was dropped
    pub fn try_access_mut<R, F: FnOnce(&mut T) -> R>(&self, op: F) -> Result<R, BorrowError> {
        let mut borrow = self.try_borrow_mut()?;
        Ok(op(&mut *borrow))
    }

    /// Tries to clone the weak reference to the data.
    ///
    /// # Returns
    ///
    /// * `Some(Weak)` if it was successful.
    /// * `None` if the number of weak pointers overflowed `u32::MAX`.
    pub fn try_clone(&self) -> Option<Weak<T>> {
        if self
            .heap()
            .is_none_or(RefBoxHeapInner::try_increase_weak_count)
        {
            Some(Weak { ptr: self.ptr })
        } else {
            None
        }
    }

    /// Returns the total number of [`Weak`] pointers that point to the same instance as this one.
    /// Returns zero for an empty weak reference.
    pub fn weak_count(&self) -> WeakCount {
        self.heap().map_or(0, RefBoxHeapInner::weak_count)
    }

    /// Returns true if the owner of the data is alive.
    pub fn is_alive(&self) -> bool {
        self.heap().is_some_and(RefBoxHeapInner::is_alive)
    }

    /// Returns true if the data is currently mutably borrowed.
    pub fn is_borrowed(&self) -> bool {
        self.heap().is_some_and(RefBoxHeapInner::is_borrowed)
    }

    /// Returns true if this `Weak` and the supplied [`RefBox`] point to the same instance.
    pub fn is(&self, owner: &RefBox<T>) -> bool {
        std::ptr::addr_eq(self.ptr.as_ptr(), owner.ptr.as_ptr())
    }

    /// Returns a raw pointer to `T`.
    ///
    /// For an empty weak reference, returns a dangling, non-null sentinel.
    /// The pointer must not be dereferenced unless the owner is alive and
    /// the borrowing rules are upheld.
    pub fn as_ptr(&self) -> *const T {
        let ptr = self.ptr.as_ptr();
        if self.is_empty() {
            return ptr as *const T;
        }

        unsafe { &raw const (*ptr).data as *const T }
    }

    /// Turns the `Weak` into a raw pointer to its control block.
    ///
    /// Empty pointers can also be round-tripped through [`Self::from_raw`].
    /// This differs from [`Self::as_ptr`], which points to the payload.
    pub fn into_raw(self) -> *mut RefBoxHeap<T> {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
    }

    /// Creates a `Weak` from a raw pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must come from [`Self::into_raw`] (possibly through a valid
    /// unsizing coercion), and its weak reference must not have been reclaimed.
    /// An empty sentinel returned by `into_raw` is also accepted. A null pointer
    /// is not valid.
    pub unsafe fn from_raw(ptr: *mut RefBoxHeap<T>) -> Self {
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        Self { ptr }
    }

    /// Casts a `Weak<T>` to a `Weak<U>`.
    ///
    /// # Safety
    ///
    /// Ensure `T` can be safely cast to `U`.
    pub unsafe fn cast<U>(self) -> Weak<U> {
        let raw_ptr = self.into_raw();

        unsafe { Weak::from_raw(raw_ptr as _) }
    }

    /// Returns the current status of the heap part for testing purposes.
    #[cfg(test)]
    fn status(&self) -> Status {
        self.heap().map_or(Status::Dropped, RefBoxHeapInner::status)
    }
}

/// A mutable borrow as a RAII-guard of a [`RefBox`] or [`Weak`].
///
/// See the [module](crate) documentation for more information.
pub struct Borrow<'ptr, T: ?Sized> {
    pub(crate) heap: &'ptr RefBoxHeap<T>,
    /// A borrow is a mutable reference to the data.
    pub(crate) _p: PhantomData<&'ptr mut T>,
}

impl<'ptr, T: ?Sized> Borrow<'ptr, T> {
    /// Creates a new borrow.
    #[inline]
    fn new(heap: &'ptr RefBoxHeap<T>) -> Self {
        heap.inner.start_borrow();
        Self {
            heap,
            _p: PhantomData,
        }
    }
}

impl<'ptr, T: ?Sized> Drop for Borrow<'ptr, T> {
    fn drop(&mut self) {
        unsafe { internals::drop_borrow(self.heap) };
    }
}

impl<'ptr, T: ?Sized> Deref for Borrow<'ptr, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.heap.data_ref() }
    }
}

impl<'ptr, T: ?Sized> DerefMut for Borrow<'ptr, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.heap.data.get() }
    }
}

impl<'ptr, T: ?Sized + fmt::Debug> fmt::Debug for Borrow<'ptr, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Borrow").field(&self.deref()).finish()
    }
}

/// An immutable borrow as a RAII-guard of a [`RefBox`] or [`Weak`].
///
/// See the [module](crate) documentation for more information.
pub struct Ref<'ptr, T: ?Sized> {
    pub(crate) heap: &'ptr RefBoxHeap<T>,
    /// A ref is a shared reference to the data.
    pub(crate) _p: PhantomData<&'ptr T>,
}

impl<'ptr, T: ?Sized> Ref<'ptr, T> {
    /// Creates a new shared borrow.
    #[inline]
    fn new(heap: &'ptr RefBoxHeap<T>) -> Self {
        heap.inner.start_shared_borrow();
        Self {
            heap,
            _p: PhantomData,
        }
    }
}

impl<'ptr, T: ?Sized> Drop for Ref<'ptr, T> {
    fn drop(&mut self) {
        unsafe { internals::drop_shared_borrow(self.heap) };
    }
}

impl<'ptr, T: ?Sized> Deref for Ref<'ptr, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.heap.data_ref() }
    }
}

impl<'ptr, T: ?Sized + fmt::Debug> fmt::Debug for Ref<'ptr, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Ref").field(&self.deref()).finish()
    }
}

/// An error that may occur during borrowing of [`RefBox`] or [`Weak`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorrowError {
    Borrowed,
    Dropped,
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BorrowError::Borrowed => write!(f, "already borrowed"),
            BorrowError::Dropped => write!(f, "owner dropped"),
        }
    }
}

impl error::Error for BorrowError {}

#[cfg(test)]
mod tests {
    use std::panic;

    use crate::internals::{RefBoxHeap, Status};
    use crate::{BorrowError, RefBox};

    #[test]
    fn ref_box_new() {
        let ref_box = RefBox::new(123456);
        assert_eq!(*ref_box.try_borrow().unwrap(), 123456);
        drop(ref_box);
    }

    /// The weak count after creation should be 0.
    #[test]
    fn ref_box_new_weak_count() {
        let ref_box = RefBox::new(123456);
        assert_eq!(ref_box.weak_count(), 0);
        drop(ref_box);
    }

    #[test]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn ref_box_new_cyclic() {
        let ref_box = RefBox::new_cyclic(|_weak| 13579);
        assert_eq!(*ref_box.try_borrow().unwrap(), 13579);
        drop(ref_box);
    }

    /// The weak count in the closure should be 1, and the weak count after creation should be 0.
    #[test]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn ref_box_new_cyclic_weak_count() {
        let ref_box = RefBox::new_cyclic(|weak| {
            assert_eq!(weak.weak_count(), 1);
        });
        assert_eq!(ref_box.weak_count(), 0);
        drop(ref_box);
    }

    /// The Weak in the closure should point to the same instance as the returned RefBox.
    #[test]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn ref_box_new_cyclic_ptr_eq() {
        let mut out_weak = None;
        let ref_box = RefBox::new_cyclic(|weak| out_weak = Some(weak.clone()));
        assert_eq!(ref_box.ptr.as_ptr(), out_weak.unwrap().ptr.as_ptr());
        drop(ref_box);
    }

    /// MIRI: A panic in the closure of [`RefBox::new_cyclic`] should not leak memory.
    #[test]
    #[should_panic]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn ref_box_new_cyclic_does_not_leak() {
        let ref_box = RefBox::new_cyclic(|_weak| {
            panic!("panic in closure!");
        });
        drop(ref_box);
    }

    /// A weak pointer returned from [`RefBox::downgrade`] should point to the same
    /// instance as the RefBox.
    #[test]
    fn downgrade_ptr_eq() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.ptr.as_ptr(), weak.ptr.as_ptr());
    }

    /// ['RefBox::downgrade'] should increase the weak reference count.
    #[test]
    fn downgrade_increases_refcount() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        assert_eq!(ref_box.weak_count(), 0);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.weak_count(), 1);
        assert_eq!(weak.weak_count(), 1);
    }

    /// ['RefBox::downgrade'] should panic if the weak count overflows u32::MAX.
    #[test]
    fn downgrade_panics_on_max() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        ref_box.heap().set_weak_count(u32::MAX);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let weak = RefBox::downgrade(&ref_box);
            drop(weak);
        }));
        assert!(result.is_err());

        ref_box.heap().set_weak_count(0);
        drop(ref_box);
    }

    /// ['RefBox::try_downgrade'] should return None if the weak count overflows
    /// u32::MAX.
    #[test]
    fn try_downgrade_returns_none_on_max() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        ref_box.heap().set_weak_count(u32::MAX);
        let weak = RefBox::try_downgrade(&ref_box);
        assert!(weak.is_none());
        drop(weak);
        ref_box.heap().set_weak_count(0);
        drop(ref_box);
    }

    /// ['Weak::clone'] should increase the weak reference count.
    #[test]
    fn cloning_weak_increases_weak_count() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        assert_eq!(ref_box.weak_count(), 0);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.weak_count(), 1);
        let weak2 = weak.clone();
        assert_eq!(ref_box.weak_count(), 2);
        drop(weak2);
        drop(weak);
    }

    /// ['Weak::clone'] should panic if the weak count overflows u32::MAX.
    #[test]
    fn cloning_weak_panics_on_max() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        weak.heap().unwrap().set_weak_count(u32::MAX);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let weak2 = weak.clone();
            drop(weak2);
        }));
        assert!(result.is_err());

        drop(weak);
        ref_box.heap().set_weak_count(0);
        drop(ref_box);
    }

    #[test]
    fn dropping_weak_decreases_weak_count() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.weak_count(), 1);
        drop(weak);
        assert_eq!(ref_box.weak_count(), 0);
    }

    #[test]
    fn owner_is_alive() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        let weak = RefBox::downgrade(&ref_box);
        assert!(weak.is_alive());
        drop(ref_box);
    }

    #[test]
    fn owner_is_not_alive_after_dropped() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        let weak = RefBox::downgrade(&ref_box);
        assert!(weak.is_alive());
        drop(ref_box);
        assert!(!weak.is_alive());
    }

    #[test]
    fn weak_borrows_reject_owner_dropped_during_borrow() {
        let owner = RefBox::new(7);
        let borrowed_weak = owner.downgrade();
        let weak = owner.downgrade();
        let borrow = borrowed_weak.try_borrow_mut().unwrap();
        drop(owner);
        assert!(matches!(weak.try_borrow(), Err(BorrowError::Dropped)));
        assert!(matches!(weak.try_borrow_mut(), Err(BorrowError::Dropped)));
        assert_eq!(*borrow, 7);
    }

    #[test]
    fn owner_is_borrowable() {
        let ref_box = RefBox::new(123456);
        let borrow = ref_box.try_borrow_mut_or_else(|| "was borrowed");
        assert!(borrow.is_ok());
        drop(borrow);
        drop(ref_box);
    }

    #[test]
    fn owner_is_not_borrowable_twice() {
        let ref_box = RefBox::new(123456);
        let borrow = ref_box.try_borrow_mut_or_else(|| "was borrowed");
        assert!(borrow.is_ok());
        let borrow2 = ref_box.try_borrow_mut_or_else(|| "was borrowed");
        assert!(matches!(borrow2, Err("was borrowed")));
        drop(borrow);
        drop(borrow2);
        drop(ref_box);
    }

    #[test]
    fn weak_is_borrowable() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert!(borrow.is_ok());
        drop(borrow);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn weak_is_not_borrowable_twice() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert!(borrow.is_ok());
        let borrow2 = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert!(matches!(borrow2, Err("was borrowed")));
        drop(borrow);
        drop(borrow2);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn weak_is_not_borrowable_if_owner_borrowed() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = ref_box.try_borrow_mut_or_else(|| "was borrowed");
        assert!(borrow.is_ok());
        let borrow2 = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert!(matches!(borrow2, Err("was borrowed")));
        drop(borrow);
        drop(borrow2);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn weak_is_not_borrowable_if_owner_dropped() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        drop(ref_box);
        let borrow = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert!(matches!(borrow, Err("was dropped")));
        drop(borrow);
        drop(weak);
    }

    #[test]
    fn dropping_owner_while_borrowed_delays_drops() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
        assert_eq!(weak.status(), Status::Borrowed);
        drop(ref_box);
        assert_eq!(weak.status(), Status::DroppedWhileBorrowed);
        drop(borrow);
        assert_eq!(weak.status(), Status::Dropped);
        drop(weak);
    }

    #[test]
    fn borrowing_changes_status() {
        let ref_box = RefBox::new(123456);
        assert_eq!(ref_box.status(), Status::Available);
        let borrow = ref_box.try_borrow_mut_or_else(|| "was borrowed");
        assert_eq!(ref_box.status(), Status::Borrowed);
        drop(borrow);
        assert_eq!(ref_box.status(), Status::Available);
        drop(ref_box);
    }

    #[test]
    fn shared_borrows_coexist_and_exclude_mutable_borrow() {
        let ref_box = RefBox::new(123456);
        let weak = ref_box.downgrade();
        let first = ref_box.try_borrow().unwrap();
        let second = weak.try_borrow().unwrap();
        assert_eq!(ref_box.status(), Status::Shared(2));
        assert_eq!(*first + *second, 246912);
        assert!(!weak.is_borrowed());
        assert!(matches!(weak.try_borrow_mut(), Err(BorrowError::Borrowed)));
        assert!(matches!(
            ref_box.try_borrow_mut(),
            Err(BorrowError::Borrowed)
        ));
        drop(first);
        assert_eq!(ref_box.status(), Status::Shared(1));
        drop(second);
        assert_eq!(ref_box.status(), Status::Available);

        let borrow = weak.try_borrow_mut().unwrap();
        assert!(matches!(weak.try_borrow(), Err(BorrowError::Borrowed)));
        assert!(matches!(ref_box.try_borrow(), Err(BorrowError::Borrowed)));
        drop(borrow);
        assert_eq!(*ref_box.try_borrow().unwrap(), 123456);
    }

    #[test]
    fn dropping_owner_while_shared_borrowed_delays_drops() {
        let ref_box = RefBox::new(123456);
        let first_weak = ref_box.downgrade();
        let second_weak = ref_box.downgrade();
        let first = first_weak.try_borrow().unwrap();
        let second = second_weak.try_borrow().unwrap();
        drop(ref_box);
        assert_eq!(first_weak.status(), Status::DroppedWhileShared(2));
        assert!(!first_weak.is_alive());
        assert!(matches!(first_weak.try_borrow(), Err(BorrowError::Dropped)));
        drop(first);
        assert_eq!(first_weak.status(), Status::DroppedWhileShared(1));
        assert_eq!(*second, 123456);
        drop(second);
        assert_eq!(first_weak.status(), Status::Dropped);
    }

    #[test]
    fn empty_weak_is_not_shared_borrowable() {
        let weak = crate::Weak::<i32>::new();
        assert!(matches!(weak.try_borrow(), Err(BorrowError::Dropped)));
    }

    #[test]
    fn dropping_owner_changes_status() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(weak.status(), Status::Available);
        drop(ref_box);
        assert_eq!(weak.status(), Status::Dropped);
        drop(weak);
    }

    #[test]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn borrowing_in_cyclic_fails() {
        let ref_box = RefBox::new_cyclic(|weak| {
            let borrow = weak.try_borrow_mut_or_else(|| "was borrowed", || "was dropped");
            assert!(borrow.is_err());
        });
        drop(ref_box);
    }

    #[test]
    #[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
    fn cloning_weak_in_cyclic_increases_refcount() {
        let ref_box = RefBox::new_cyclic(|weak| {
            let weak2 = weak.clone();
            assert_eq!(weak.weak_count(), 2);
            drop(weak2);
        });
        drop(ref_box);
    }

    #[test]
    fn calling_getters_while_having_mutable_ref() {
        let ref_box = RefBox::new(123456);
        let mut borrow = ref_box.try_borrow_mut_or_else(|| "was borrowed").unwrap();
        let mut_ref = &mut *borrow;
        assert_eq!(ref_box.weak_count(), 0);
        *mut_ref = 654321;
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.weak_count(), 1);
        *mut_ref = 13579;
        drop(borrow);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn single_ref_boxes_are_distinct() {
        let ref_box_1 = RefBox::new(123456);
        let ref_box_2 = RefBox::new(654321);

        let borrow1 = ref_box_1.try_borrow_mut().unwrap();
        let borrow2 = ref_box_2.try_borrow_mut().unwrap();

        assert_ne!(*borrow1, *borrow2);
        assert_eq!(*borrow1, 123456);
        assert_eq!(*borrow2, 654321);

        drop(borrow1);
        drop(borrow2);
        drop(ref_box_1);
        drop(ref_box_2);
    }

    /// Test if [`RefBox::as_ptr`] returns the right pointer. Run with MIRI.
    #[test]
    fn ref_box_as_ptr() {
        let ref_box = RefBox::new(123456);
        let ptr = ref_box.as_ptr();

        let heap = unsafe { ref_box.ptr.as_ref() };
        let real_ptr = heap.data.get();

        assert_eq!(ptr, real_ptr);
        drop(ref_box);
    }

    /// Test if [`Weak::as_ptr`] returns the right pointer. Run with MIRI.
    #[test]
    fn weak_as_ptr() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let ptr = weak.as_ptr();

        let heap = unsafe { weak.ptr.as_ref() };
        let real_ptr = heap.data.get();

        assert_eq!(ptr, real_ptr);
        drop(weak);
        drop(ref_box);
    }

    /// Test if the overhead of the heap part is 12 bytes when 'cyclic_stable' feature is not enabled.
    #[test]
    #[cfg(not(feature = "cyclic_stable"))]
    fn heap_overhead() {
        let layout = std::alloc::Layout::new::<RefBoxHeap<()>>();
        assert_eq!(layout.size(), 12);
    }

    /// Test if the overhead of the heap part is 16 bytes when 'cyclic_stable' feature is enabled
    /// and the pointer size is 16 bits.
    #[test]
    #[cfg(feature = "cyclic_stable")]
    #[cfg(target_pointer_width = "16")]
    fn heap_overhead_cyclic_stable_16bit() {
        let layout = std::alloc::Layout::new::<RefBoxHeap<()>>();
        assert_eq!(layout.size(), 16);
    }

    /// Test if the overhead of the heap part is 20 bytes when 'cyclic_stable' feature is enabled
    /// and the pointer size is 32 bits.
    #[test]
    #[cfg(feature = "cyclic_stable")]
    #[cfg(target_pointer_width = "32")]
    fn heap_overhead_cyclic_stable_32bit() {
        let layout = std::alloc::Layout::new::<RefBoxHeap<()>>();
        assert_eq!(layout.size(), 20);
    }

    /// Test if the overhead of the heap part is 32 bytes when 'cyclic_stable' feature is enabled
    /// and the pointer size is 64 bits.
    #[test]
    #[cfg(feature = "cyclic_stable")]
    #[cfg(target_pointer_width = "64")]
    fn heap_overhead_cyclic_stable_64bit() {
        let layout = std::alloc::Layout::new::<RefBoxHeap<()>>();
        assert_eq!(layout.size(), 32);
    }
}
