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
//! [`Rc`]: std::rc::Rc
//! [`RefCell`]: std::cell::RefCell
//!
//! # Examples
//!
//! ```
//! use refbox::RefBox;
//!
//! fn main() {
//!     let ref_box = RefBox::new(100);
//!     let weak = RefBox::downgrade(&ref_box);
//!     let borrow = weak.try_borrow_mut().unwrap();
//!     assert_eq!(*borrow, 100);
//! }
//! ```

mod internals;

use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use std::error;

use internals::{RefBoxHeap, RefBoxHeapInner, Status};

/// A smart pointer with many reference-counted weak references.
///
/// See the [module](crate) documentation for more information.
///
/// # Accessing the data behind the RefBox
///
/// See [`RefBox::try_borrow`], [`RefBox::try_borrow_mut`] and
/// [`RefBox::try_access_mut`].
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

impl<T: Default> Default for RefBox<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> RefBox<T> {
    /// Creates a new `RefBox` reference counted pointer.
    pub fn new(value: T) -> Self {
        internals::new_ref_box(value)
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
        match self.heap().status() {
            Status::Available => Ok(unsafe { Borrow::new(self.ptr.as_ref()) }),
            Status::Borrowed | Status::Shared(_) => Err(BorrowError::Borrowed),
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
    pub fn downgrade(&self) -> Weak<T> {
        self.heap().increase_weak_count();
        Weak { ptr: self.ptr }
    }

    /// Returns a raw pointer to `T`.
    pub fn as_ptr(&self) -> *const T {
        let ptr = self.ptr.as_ptr();

        unsafe { &raw const (*ptr).data as *const T }
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
/// See [`Weak::try_borrow`], [`Weak::try_borrow_mut`] and
/// [`Weak::try_access_mut`].
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
    /// Borrowing it returns [`BorrowError::Dropped`].
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
        match self.heap().map_or(Status::Dropped, RefBoxHeapInner::status) {
            Status::Available => Ok(unsafe { Borrow::new(self.ptr.as_ref()) }),
            Status::Borrowed | Status::Shared(_) => Err(BorrowError::Borrowed),
            Status::Dropped | Status::DroppedWhileBorrowed | Status::DroppedWhileShared(_) => {
                Err(BorrowError::Dropped)
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

    /// Returns true if the owner of the data is alive.
    pub fn is_alive(&self) -> bool {
        self.heap().is_some_and(RefBoxHeapInner::is_alive)
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
    use std::cell::Cell;
    use std::panic;
    use std::rc::Rc;

    use crate::internals::{RefBoxHeap, Status};
    use crate::{BorrowError, RefBox};

    #[test]
    fn ref_box_new() {
        let ref_box = RefBox::new(123456);
        assert_eq!(*ref_box.try_borrow().unwrap(), 123456);
        drop(ref_box);
    }

    #[test]
    fn ref_box_new_weak_count() {
        let ref_box = RefBox::new(123456);
        assert_eq!(ref_box.heap().weak_count(), 0);
        drop(ref_box);
    }

    #[test]
    fn downgrade_ptr_eq() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.ptr.as_ptr(), weak.ptr.as_ptr());
    }

    #[test]
    fn downgrade_increases_refcount() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        assert_eq!(ref_box.heap().weak_count(), 0);
        let _weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.heap().weak_count(), 1);
    }

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

    #[test]
    fn cloning_weak_increases_weak_count() {
        let val = 123456;
        let ref_box = RefBox::new(val);
        assert_eq!(ref_box.heap().weak_count(), 0);
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.heap().weak_count(), 1);
        let weak2 = weak.clone();
        assert_eq!(ref_box.heap().weak_count(), 2);
        drop(weak2);
        drop(weak);
    }

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
        assert_eq!(ref_box.heap().weak_count(), 1);
        drop(weak);
        assert_eq!(ref_box.heap().weak_count(), 0);
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
        let borrow = ref_box.try_borrow_mut();
        assert!(borrow.is_ok());
        drop(borrow);
        drop(ref_box);
    }

    #[test]
    fn owner_is_not_borrowable_twice() {
        let ref_box = RefBox::new(123456);
        let borrow = ref_box.try_borrow_mut();
        assert!(borrow.is_ok());
        let borrow2 = ref_box.try_borrow_mut();
        assert!(matches!(borrow2, Err(BorrowError::Borrowed)));
        drop(borrow);
        drop(borrow2);
        drop(ref_box);
    }

    #[test]
    fn weak_is_borrowable() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut();
        assert!(borrow.is_ok());
        drop(borrow);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn weak_is_not_borrowable_twice() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut();
        assert!(borrow.is_ok());
        let borrow2 = weak.try_borrow_mut();
        assert!(matches!(borrow2, Err(BorrowError::Borrowed)));
        drop(borrow);
        drop(borrow2);
        drop(weak);
        drop(ref_box);
    }

    #[test]
    fn weak_is_not_borrowable_if_owner_borrowed() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = ref_box.try_borrow_mut();
        assert!(borrow.is_ok());
        let borrow2 = weak.try_borrow_mut();
        assert!(matches!(borrow2, Err(BorrowError::Borrowed)));
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
        let borrow = weak.try_borrow_mut();
        assert!(matches!(borrow, Err(BorrowError::Dropped)));
        drop(borrow);
        drop(weak);
    }

    #[test]
    fn dropping_owner_while_borrowed_delays_drops() {
        let ref_box = RefBox::new(123456);
        let weak = RefBox::downgrade(&ref_box);
        let borrow = weak.try_borrow_mut();
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
        let borrow = ref_box.try_borrow_mut();
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

    struct DropCount(Rc<Cell<usize>>);
    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn counted() -> (RefBox<DropCount>, Rc<Cell<usize>>) {
        let drops = Rc::new(Cell::new(0));
        (RefBox::new(DropCount(drops.clone())), drops)
    }

    #[test]
    fn dropping_owner_while_borrowed_drops_data_once_at_release() {
        let (ref_box, drops) = counted();
        let weak = ref_box.downgrade();
        let borrow = weak.try_borrow_mut().unwrap();
        drop(ref_box);
        assert_eq!(drops.get(), 0);
        drop(borrow);
        assert_eq!(drops.get(), 1);
        drop(weak);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn dropping_owner_while_shared_drops_data_once_at_last_release() {
        let (ref_box, drops) = counted();
        let weak = ref_box.downgrade();
        let first = weak.try_borrow().unwrap();
        let second = weak.try_borrow().unwrap();
        drop(ref_box);
        drop(first);
        assert_eq!(drops.get(), 0);
        drop(second);
        assert_eq!(drops.get(), 1);
        drop(weak);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn weak_is_not_mutably_borrowable_if_owner_dropped_while_shared() {
        let ref_box = RefBox::new(123456);
        let weak = ref_box.downgrade();
        let shared = weak.try_borrow().unwrap();
        drop(ref_box);
        assert_eq!(weak.status(), Status::DroppedWhileShared(1));
        assert!(matches!(weak.try_borrow_mut(), Err(BorrowError::Dropped)));
        assert!(matches!(
            weak.try_access_mut(|_| unreachable!()),
            Err(BorrowError::Dropped)
        ));
        drop(shared);
        assert!(matches!(weak.try_borrow_mut(), Err(BorrowError::Dropped)));
    }

    #[test]
    fn panicking_while_borrowed_releases_the_borrow() {
        let ref_box = RefBox::new(123456);
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let mut borrow = ref_box.try_borrow_mut().unwrap();
            *borrow = 654321;
            panic!("panic while borrowed");
        }));
        assert!(result.is_err());
        assert_eq!(ref_box.status(), Status::Available);
        assert_eq!(*ref_box.try_borrow_mut().unwrap(), 654321);
    }

    #[test]
    fn panicking_while_shared_releases_the_shared_borrow() {
        let ref_box = RefBox::new(123456);
        let weak = ref_box.downgrade();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let _first = ref_box.try_borrow().unwrap();
            let _second = weak.try_borrow().unwrap();
            panic!("panic while shared");
        }));
        assert!(result.is_err());
        assert_eq!(ref_box.status(), Status::Available);
        assert!(weak.try_borrow_mut().is_ok());
    }

    #[test]
    fn panicking_in_access_mut_releases_the_borrow() {
        let ref_box = RefBox::new(123456);
        let weak = ref_box.downgrade();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            weak.try_access_mut(|_| panic!("panic in accessor"))
        }));
        assert!(result.is_err());
        assert_eq!(weak.status(), Status::Available);
        assert_eq!(weak.try_access_mut(|value| *value), Ok(123456));
    }

    #[test]
    fn reborrowing_inside_access_mut_is_rejected() {
        let ref_box = RefBox::new(123456);
        let weak = ref_box.downgrade();
        let reentered = weak.try_access_mut(|value| {
            assert!(matches!(
                ref_box.try_borrow_mut(),
                Err(BorrowError::Borrowed)
            ));
            assert!(matches!(ref_box.try_borrow(), Err(BorrowError::Borrowed)));
            assert!(matches!(weak.try_borrow(), Err(BorrowError::Borrowed)));
            assert_eq!(
                weak.try_access_mut(|_| unreachable!()),
                Err(BorrowError::Borrowed)
            );
            assert_eq!(
                ref_box.try_access_mut(|_| unreachable!()),
                Err(BorrowError::Borrowed)
            );
            *value = 654321;
        });
        assert_eq!(reentered, Ok(()));
        assert_eq!(*ref_box.try_borrow().unwrap(), 654321);
    }

    #[test]
    fn dropping_owner_inside_access_mut_delays_drop_until_return() {
        let (ref_box, drops) = counted();
        let weak = ref_box.downgrade();
        let accessed = weak.try_access_mut(|_| {
            drop(ref_box);
            assert_eq!(drops.get(), 0);
            assert_eq!(weak.status(), Status::DroppedWhileBorrowed);
            assert!(matches!(weak.try_borrow(), Err(BorrowError::Dropped)));
        });
        assert_eq!(accessed, Ok(()));
        assert_eq!(drops.get(), 1);
        assert_eq!(weak.status(), Status::Dropped);
    }

    #[test]
    fn calling_getters_while_having_mutable_ref() {
        let ref_box = RefBox::new(123456);
        let mut borrow = ref_box.try_borrow_mut().unwrap();
        let mut_ref = &mut *borrow;
        assert_eq!(ref_box.heap().weak_count(), 0);
        *mut_ref = 654321;
        let weak = RefBox::downgrade(&ref_box);
        assert_eq!(ref_box.heap().weak_count(), 1);
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

    #[test]
    fn ref_box_as_ptr() {
        let ref_box = RefBox::new(123456);
        let ptr = ref_box.as_ptr();

        let heap = unsafe { ref_box.ptr.as_ref() };
        let real_ptr = heap.data.get();

        assert_eq!(ptr, real_ptr);
        drop(ref_box);
    }

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

    #[test]
    fn heap_overhead() {
        let layout = std::alloc::Layout::new::<RefBoxHeap<()>>();
        assert_eq!(layout.size(), 12);
    }
}
