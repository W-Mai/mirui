extern crate std;

#[path = "../tests/support/source_cells.rs"]
pub(crate) mod source_cells;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

std::thread_local! {
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
    static TRACK: Cell<bool> = const { Cell::new(false) };
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
    static FAIL_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
}

fn allocation_allowed() -> bool {
    FAIL_AFTER
        .try_with(|remaining| match remaining.get() {
            None => true,
            Some(0) => false,
            Some(count) => {
                remaining.set(Some(count - 1));
                true
            }
        })
        .unwrap_or(true)
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !allocation_allowed() {
            return core::ptr::null_mut();
        }
        let _ = TRACK.try_with(|track| {
            if track.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: Forward the unchanged layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The pointer and layout came from the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if !allocation_allowed() {
            return core::ptr::null_mut();
        }
        let _ = TRACK.try_with(|track| {
            if track.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: Forward the owned allocation and new size unchanged.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

pub(crate) fn failing_allocations_after<R>(successful: usize, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<usize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAIL_AFTER.with(|remaining| remaining.set(self.0));
        }
    }
    let restore = Restore(FAIL_AFTER.with(|remaining| remaining.replace(Some(successful))));
    let result = run();
    drop(restore);
    result
}

pub(crate) fn tracked_allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    struct StopTracking;
    impl Drop for StopTracking {
        fn drop(&mut self) {
            TRACK.with(|track| track.set(false));
        }
    }
    let stop = StopTracking;
    run();
    drop(stop);
    ALLOCATIONS.with(Cell::get)
}

#[cfg(feature = "gallery")]
pub(crate) fn physical_key(
    position: crate::input::event::input::PhysicalKey,
) -> crate::input::event::input::Key {
    crate::input::event::input::Key {
        physical: Some(position),
        code: crate::input::event::input::KEY_UNKNOWN,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    }
}
