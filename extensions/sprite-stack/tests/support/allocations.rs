use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct TrackingAllocator;

std::thread_local! {
    static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<AllocationStats> = const { Cell::new(AllocationStats { count: 0, allocated: 0, freed: 0 }) };
}

#[derive(Clone, Copy, Default)]
pub struct AllocationStats {
    pub count: usize,
    pub allocated: usize,
    pub freed: usize,
}

fn count(allocated: usize, freed: usize) {
    TRACK_ALLOCATIONS.with(|tracking| {
        if tracking.get() {
            let mut stats = ALLOCATIONS.get();
            stats.count += usize::from(allocated != 0);
            stats.allocated += allocated;
            stats.freed += freed;
            ALLOCATIONS.set(stats);
        }
    });
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size(), 0);
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(0, layout.size());
        // SAFETY: `ptr` and `layout` came from this allocator's system allocation.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size, layout.size());
        // SAFETY: the allocation remains owned by the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

pub fn tracked_allocations(run: impl FnOnce()) -> usize {
    tracked_storage(run).count
}

pub fn tracked_storage(run: impl FnOnce()) -> AllocationStats {
    ALLOCATIONS.set(AllocationStats::default());
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(true));
    run();
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(false));
    ALLOCATIONS.get()
}
