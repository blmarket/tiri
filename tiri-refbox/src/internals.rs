//! Contains the internal state machine and heap part of a `RefBox`.

use crate::RefBox;
use core::cell::{Cell, UnsafeCell};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::alloc;

#[cfg(feature = "cyclic_stable")]
use std::alloc::Layout;

/// The reference counter.
pub(crate) type WeakCount = u32;

/// The current status of a `RefBox`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Available,
    Borrowed,
    Dropped,
    DroppedWhileBorrowed,
}

/// The status part of the RefBoxHeap.
#[derive(Debug)]
pub(crate) struct RefBoxHeapInner {
    /// The current status of the data.
    status: Cell<Status>,
    /// The number of weak references to the data.
    weak_count: Cell<WeakCount>,
    /// The layout of the `RefBoxHeap<T>`.
    ///
    /// Necessary to deallocate the memory of the heap part when feature `cyclic_stable` is enabled.
    #[cfg(feature = "cyclic_stable")]
    layout: Layout,
}

/// The part of a `RefBox` that is stored on the heap.
#[repr(C)]
#[derive(Debug)]
pub struct RefBoxHeap<T: ?Sized> {
    pub(crate) inner: RefBoxHeapInner,
    pub(crate) data: UnsafeCell<T>,
}

impl RefBoxHeapInner {
    /// Returns the current status.
    #[inline]
    pub(crate) fn status(&self) -> Status {
        self.status.get()
    }

    /// Returns the current weak reference count.
    #[inline]
    pub(crate) fn weak_count(&self) -> WeakCount {
        self.weak_count.get()
    }

    /// The layout of the entire `RefBoxHeap<T>`.
    ///
    /// Necessary to deallocate the memory of the heap part when feature `cyclic_stable` is enabled.
    #[cfg(feature = "cyclic_stable")]
    #[inline]
    fn layout(&self) -> Layout {
        self.layout
    }

    /// Sets the weak reference count. Used in tests.
    #[inline]
    #[cfg(test)]
    pub(crate) fn set_weak_count(&self, count: WeakCount) {
        self.weak_count.set(count);
    }

    /// Increases the reference counter by 1.
    ///
    /// # Panics
    ///
    /// Panics if the number of `Weak`s overflows `RefCount::MAX`.
    #[inline]
    pub(crate) fn increase_weak_count(&self) {
        let refcount = self.weak_count();

        if refcount == WeakCount::MAX {
            cold_panic();
        } else {
            self.weak_count.set(refcount + 1);
        }
    }

    /// Increases the reference counter by 1.
    ///
    /// Returns false if the refcount overflowed.
    #[inline]
    pub(crate) fn try_increase_weak_count(&self) -> bool {
        let refcount = self.weak_count();

        if refcount == WeakCount::MAX {
            cold_false()
        } else {
            self.weak_count.set(refcount + 1);
            true
        }
    }

    /// Decreases the reference counter by 1.
    #[inline]
    fn decrease_refcount(&self) -> WeakCount {
        let refcount = self.weak_count() - 1;
        self.weak_count.set(refcount);
        refcount
    }

    /// Returns true if the owner is alive.
    #[inline]
    pub(crate) fn is_alive(&self) -> bool {
        matches!(self.status(), Status::Available | Status::Borrowed)
    }

    /// Returns true if the data is currently borrowed.
    #[inline]
    pub(crate) fn is_borrowed(&self) -> bool {
        matches!(self.status(), Status::Borrowed)
    }

    /// Sets the status to `Borrowed`.
    #[inline]
    pub(crate) fn start_borrow(&self) {
        self.status.set(Status::Borrowed);
    }
}

impl<T: ?Sized> RefBoxHeap<T> {
    /// Returns a shared reference to the data.
    ///
    /// # Safety
    ///
    /// 1. Ensure there are no mutable references to `T`.
    /// 2. Ensure `T` is initialized.
    /// 3. Ensure `T` is not dropped.
    #[inline]
    pub(crate) unsafe fn data_ref(&self) -> &T {
        unsafe { &*self.data.get() }
    }

    /// Runs the destructor of the data.
    ///
    /// # Safety
    ///
    /// 1. Ensure there are no references to `T`.
    /// 2. Ensure `T` is initialized.
    /// 3. Ensure `T` is not already dropped.
    pub(crate) unsafe fn drop_data(&self) {
        unsafe { ptr::drop_in_place(self.data.get()) };
        self.inner.status.set(Status::Dropped);
    }
}

/// Panics.
///
/// Is unlikely to be called, so it has a 'cold' attribute for optimization.
#[cold]
#[inline(never)]
fn cold_panic() {
    panic!()
}

/// Returns false.
///
/// Is unlikely to be called, so it has a 'cold' attribute for optimization.
#[cold]
#[inline(never)]
fn cold_false() -> bool {
    false
}

/// Creates a new pointer.
#[inline]
pub(crate) fn new_ref_box<T>(value: T) -> RefBox<T> {
    let heap = Box::into_raw(Box::new(RefBoxHeap {
        inner: RefBoxHeapInner {
            status: Cell::new(Status::Available),
            weak_count: Cell::new(0),
            #[cfg(feature = "cyclic_stable")]
            layout: Layout::new::<RefBoxHeap<T>>(),
        },
        data: UnsafeCell::new(value),
    }));

    let ptr = unsafe { NonNull::new_unchecked(heap) };

    RefBox {
        ptr,
        _p: PhantomData,
    }
}

/// Creates a new `RefBox` through a closure which receives a
/// `RefBoxRef` to the same data.
#[cfg(any(feature = "cyclic", feature = "cyclic_stable"))]
#[inline]
pub(crate) fn new_cyclic_ref_box<T, F: FnOnce(&crate::Weak<T>) -> T>(op: F) -> RefBox<T> {
    let heap = Box::into_raw(Box::new(RefBoxHeap {
        inner: RefBoxHeapInner {
            status: Cell::new(Status::Dropped),
            weak_count: Cell::new(1),
            #[cfg(feature = "cyclic_stable")]
            layout: Layout::new::<RefBoxHeap<T>>(),
        },
        data: UnsafeCell::new(core::mem::MaybeUninit::<T>::uninit()),
    }));

    let weak = crate::Weak {
        ptr: unsafe { NonNull::new_unchecked(heap.cast()) },
    };

    let data = op(&weak);

    unsafe { (*heap).data.get_mut().write(data) };

    unsafe { (*heap).inner.status.set(Status::Available) };

    drop(weak);

    RefBox {
        ptr: unsafe { NonNull::new_unchecked(heap.cast()) },
        _p: PhantomData,
    }
}

/// Deallocates the heap part of the `RefBox`.
unsafe fn dealloc_heap<T: ?Sized>(heap: NonNull<RefBoxHeap<T>>) {
    #[cfg(feature = "cyclic_stable")]
    let layout = unsafe { &(*heap.as_ptr()).inner }.layout();

    #[cfg(all(feature = "cyclic", not(feature = "cyclic_stable")))]
    let layout = unsafe { alloc::Layout::for_value_raw(heap.as_ptr()) };

    #[cfg(all(not(feature = "cyclic"), not(feature = "cyclic_stable")))]
    let layout = alloc::Layout::for_value(unsafe { heap.as_ref() });

    unsafe {
        alloc::dealloc(heap.as_ptr().cast(), layout);
    }
}

/// Called when the owning RefBox is dropped.
#[inline]
pub(crate) unsafe fn drop_ref_box<T: ?Sized>(heap: NonNull<RefBoxHeap<T>>) {
    match unsafe { heap.as_ref() }.inner.status() {
        Status::Available => {
            unsafe { heap.as_ref().drop_data() };

            if unsafe { heap.as_ref() }.inner.weak_count() == 0 {
                unsafe { dealloc_heap(heap) };
            }
        }
        Status::Borrowed => {
            unsafe { heap.as_ref() }
                .inner
                .status
                .set(Status::DroppedWhileBorrowed);
        }
        Status::DroppedWhileBorrowed | Status::Dropped => {
            unsafe { std::hint::unreachable_unchecked() };
        }
    }
}

/// Called when a weak pointer is dropped.
#[inline]
pub(crate) unsafe fn drop_weak<T: ?Sized>(heap: NonNull<RefBoxHeap<T>>) {
    let refcount = unsafe { &(*heap.as_ptr()).inner }.decrease_refcount();

    if refcount == 0 && unsafe { &(*heap.as_ptr()).inner }.status() == Status::Dropped {
        unsafe { dealloc_heap(heap) };
    }
}

/// Called when a Borrow is dropped.
#[inline]
pub(crate) unsafe fn drop_borrow<T: ?Sized>(heap: &RefBoxHeap<T>) {
    match heap.inner.status() {
        Status::Borrowed => {
            heap.inner.status.set(Status::Available);
        }
        Status::DroppedWhileBorrowed => {
            unsafe { heap.drop_data() };
        }
        Status::Available | Status::Dropped => {
            unsafe { std::hint::unreachable_unchecked() };
        }
    }
}
