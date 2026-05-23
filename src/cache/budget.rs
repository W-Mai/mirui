#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxSize {
    Count(usize),
    Bytes(usize),
    Disabled,
}

impl MaxSize {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, MaxSize::Disabled)
    }

    pub fn limit(&self) -> usize {
        match self {
            MaxSize::Count(n) | MaxSize::Bytes(n) => *n,
            MaxSize::Disabled => 0,
        }
    }
}

pub trait HasSize {
    fn cache_size(&self) -> usize;
}

impl<T> HasSize for alloc::vec::Vec<T> {
    fn cache_size(&self) -> usize {
        self.len() * core::mem::size_of::<T>()
    }
}

impl HasSize for alloc::string::String {
    fn cache_size(&self) -> usize {
        self.len()
    }
}

// Plain copy types and primitives report `size_of`. The cache's Bytes
// budget mode is not really meant for these (too coarse to be useful),
// but the `HasSize` bound on Cache<K, V> still has to be satisfied.
macro_rules! has_size_via_size_of {
    ($($ty:ty),* $(,)?) => {
        $(
            impl HasSize for $ty {
                fn cache_size(&self) -> usize {
                    core::mem::size_of::<Self>()
                }
            }
        )*
    };
}

has_size_via_size_of!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, bool, char
);

// RefCell<T> wraps cached resources that the consumer borrows mutably
// at use time (e.g. an offscreen Texture rendered into during a frame).
// `cache_size` is read by the cache during insert / evict, when no
// consumer should be holding a borrow — `borrow()` panicking here is
// the correct signal that someone reads the cache mid-render.
impl<T: HasSize> HasSize for core::cell::RefCell<T> {
    fn cache_size(&self) -> usize {
        self.borrow().cache_size()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveCond {
    Ok,
    TooLarge,
    NeedVictim,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn vec_cache_size_matches_byte_count() {
        let v: alloc::vec::Vec<u32> = vec![1, 2, 3];
        assert_eq!(v.cache_size(), 12);
    }

    #[test]
    fn refcell_delegates_to_inner() {
        let c = core::cell::RefCell::new(alloc::string::String::from("hello"));
        assert_eq!(c.cache_size(), 5);
    }
}
