use alloc::alloc::{alloc, dealloc};
use core::alloc::Layout;
use core::mem::{MaybeUninit, align_of, size_of};
use core::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PayloadError {
    SizeOverflow,
    InvalidLayout,
    AllocationFailed,
    Capacity { required: usize, available: usize },
    Alignment { required: usize, available: usize },
}

/// Byte storage; the owning record queue consumes or drops every stored value.
pub(crate) struct PayloadArena {
    data: NonNull<MaybeUninit<u8>>,
    capacity: usize,
    alignment: usize,
    len: usize,
}

impl Default for PayloadArena {
    fn default() -> Self {
        Self {
            data: NonNull::dangling(),
            capacity: 0,
            alignment: 1,
            len: 0,
        }
    }
}

impl PayloadArena {
    pub(crate) fn required<T>(&self) -> Result<(usize, usize), PayloadError> {
        if size_of::<T>() == 0 {
            return Ok((0, 1));
        }
        let offset = align_up(self.len, align_of::<T>()).ok_or(PayloadError::SizeOverflow)?;
        let end = offset
            .checked_add(size_of::<T>())
            .ok_or(PayloadError::SizeOverflow)?;
        Ok((end, align_of::<T>()))
    }

    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn alignment(&self) -> usize {
        self.alignment
    }

    pub(crate) fn reserve(&mut self, bytes: usize, alignment: usize) -> Result<(), PayloadError> {
        Layout::from_size_align(bytes, alignment).map_err(|_| PayloadError::InvalidLayout)?;
        if bytes <= self.capacity && alignment <= self.alignment {
            return Ok(());
        }
        let capacity = bytes.max(self.capacity);
        let alignment = alignment.max(self.alignment);
        let layout = Layout::from_size_align(capacity, alignment)
            .map_err(|_| PayloadError::InvalidLayout)?;
        if capacity == 0 {
            return Ok(());
        }

        // SAFETY: layout is non-zero and valid.
        let allocation = unsafe { alloc(layout) };
        let data = NonNull::new(allocation.cast::<MaybeUninit<u8>>())
            .ok_or(PayloadError::AllocationFailed)?;
        if self.capacity != 0 {
            // SAFETY: both allocations cover len bytes, do not overlap, and
            // no payload references survive a reservation by the record queue.
            unsafe {
                core::ptr::copy_nonoverlapping(self.data.as_ptr(), data.as_ptr(), self.len);
                dealloc(
                    self.data.as_ptr().cast::<u8>(),
                    Layout::from_size_align_unchecked(self.capacity, self.alignment),
                );
            }
        }
        self.data = data;
        self.capacity = capacity;
        self.alignment = alignment;
        Ok(())
    }

    pub(crate) fn try_store<T: 'static>(&mut self, value: T) -> Result<usize, (PayloadError, T)> {
        let (end, alignment) = match self.required::<T>() {
            Ok(required) => required,
            Err(error) => return Err((error, value)),
        };
        if size_of::<T>() == 0 {
            // SAFETY: aligned dangling storage transfers ownership of a ZST.
            unsafe { NonNull::<T>::dangling().as_ptr().write(value) };
            return Ok(0);
        }
        if alignment > self.alignment {
            return Err((
                PayloadError::Alignment {
                    required: alignment,
                    available: self.alignment,
                },
                value,
            ));
        }
        if end > self.capacity {
            return Err((
                PayloadError::Capacity {
                    required: end,
                    available: self.capacity,
                },
                value,
            ));
        }
        let offset = end - size_of::<T>();
        // SAFETY: capacity and base/offset alignment were checked without
        // relocating the buffer. This slot has no previous live value.
        unsafe { self.data.as_ptr().add(offset).cast::<T>().write(value) };
        self.len = end;
        Ok(offset)
    }

    pub(crate) fn ptr(&mut self, offset: usize) -> *mut u8 {
        self.data.as_ptr().wrapping_add(offset).cast::<u8>()
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }
}

impl Drop for PayloadArena {
    fn drop(&mut self) {
        if self.capacity == 0 {
            return;
        }
        // SAFETY: the record owner already consumed/dropped all values.
        // This allocation was made with the same layout.
        unsafe {
            dealloc(
                self.data.as_ptr().cast::<u8>(),
                Layout::from_size_align_unchecked(self.capacity, self.alignment),
            );
        }
    }
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

/// The pointer must hold one unread T paired with its owning record.
pub(crate) unsafe fn read_stored<T>(payload: *mut u8) -> T {
    if size_of::<T>() == 0 {
        // SAFETY: the caller owns one logical T and dangling is ZST-aligned.
        return unsafe { NonNull::<T>::dangling().as_ptr().read() };
    }
    // SAFETY: the caller supplies a live, aligned T and consumes it once.
    unsafe { payload.cast::<T>().read() }
}

/// The pointer must hold one unconsumed T paired with its owning record.
pub(crate) unsafe fn drop_stored<T>(payload: *mut u8) {
    if size_of::<T>() == 0 {
        // SAFETY: the caller owns one logical T.
        unsafe { NonNull::<T>::dangling().as_ptr().drop_in_place() };
    } else {
        // SAFETY: the caller supplies a live, aligned T and destroys it once.
        unsafe { payload.cast::<T>().drop_in_place() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;
    use alloc::rc::Rc;
    use core::cell::Cell;

    #[derive(Debug)]
    struct Owned(Rc<Cell<usize>>);

    impl Drop for Owned {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    #[repr(align(128))]
    struct Aligned([u8; 128]);

    #[test]
    fn bounded_storage_keeps_existing_values_and_returns_rejected_ownership() {
        let drops = Rc::new(Cell::new(0));
        let mut arena = PayloadArena::default();
        arena
            .reserve(size_of::<Owned>(), align_of::<Owned>())
            .unwrap();
        let first = arena.try_store(Owned(drops.clone())).unwrap();
        let before = arena.len;
        let (error, rejected) = arena.try_store(Owned(drops.clone())).unwrap_err();
        assert_eq!(
            error,
            PayloadError::Capacity {
                required: size_of::<Owned>() * 2,
                available: size_of::<Owned>(),
            }
        );
        assert_eq!(arena.len, before);
        assert_eq!(drops.get(), 0);
        drop(rejected);
        // SAFETY: first is the single unread Owned slot.
        unsafe { drop_stored::<Owned>(arena.ptr(first)) };
        arena.clear();
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn alignment_failure_is_non_mutating_and_reservation_preserves_live_data() {
        let mut arena = PayloadArena::default();
        arena.reserve(256, 1).unwrap();
        let first = arena.try_store(7u8).unwrap();
        let (error, value) = arena.try_store(Aligned([3; 128])).unwrap_err();
        assert_eq!(
            error,
            PayloadError::Alignment {
                required: 128,
                available: 1
            }
        );
        assert_eq!(arena.len, 1);
        arena.reserve(128, 128).unwrap();
        assert_eq!(arena.capacity(), 256);
        let aligned = arena.try_store(value).unwrap();
        assert_eq!(aligned, 128);
        assert_eq!(arena.ptr(aligned) as usize % 128, 0);
        // SAFETY: each read consumes exactly its own stored type and slot.
        unsafe {
            assert_eq!(read_stored::<u8>(arena.ptr(first)), 7);
            assert_eq!(
                read_stored::<Aligned>(arena.ptr(aligned)),
                Aligned([3; 128])
            );
        }
        arena.clear();
    }

    #[test]
    fn bounded_writes_rejection_and_clear_never_allocate() {
        let mut arena = PayloadArena::default();
        arena.reserve(16, align_of::<u32>()).unwrap();
        let allocations = tracked_allocations(|| {
            for _ in 0..32 {
                let offsets =
                    core::array::from_fn::<_, 4, _>(|i| arena.try_store(i as u32).unwrap());
                assert!(arena.try_store(9u32).is_err());
                for (i, offset) in offsets.into_iter().enumerate() {
                    // SAFETY: each slot contains one unread u32.
                    assert_eq!(unsafe { read_stored::<u32>(arena.ptr(offset)) }, i as u32);
                }
                arena.clear();
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(arena.capacity(), 16);
    }

    #[test]
    fn invalid_reservations_keep_allocation_and_values_intact() {
        let mut arena = PayloadArena::default();
        arena.reserve(8, 8).unwrap();
        let offset = arena.try_store(19u64).unwrap();
        let pointer = arena.data;
        assert_eq!(arena.reserve(16, 3), Err(PayloadError::InvalidLayout));
        assert_eq!(
            arena.reserve(usize::MAX, 8),
            Err(PayloadError::InvalidLayout)
        );
        assert_eq!(arena.data, pointer);
        assert_eq!(arena.capacity(), 8);
        assert_eq!(arena.len, 8);
        // SAFETY: offset is the single unread u64.
        assert_eq!(unsafe { read_stored::<u64>(arena.ptr(offset)) }, 19);
        arena.clear();
    }

    #[test]
    fn zero_sized_values_need_no_storage_even_with_large_alignment() {
        #[derive(Debug)]
        #[repr(align(256))]
        struct Empty;
        let mut arena = PayloadArena::default();
        assert_eq!(size_of::<Empty>(), 0);
        let allocations = tracked_allocations(|| {
            let offset = arena.try_store(Empty).unwrap();
            // SAFETY: this is one logical Empty, not an allocated byte slot.
            unsafe { drop_stored::<Empty>(arena.ptr(offset)) };
            arena.clear();
        });
        assert_eq!(allocations, 0);
        assert_eq!(arena.capacity(), 0);
    }
}
