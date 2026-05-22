#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MaxSize {
    Count(usize),
    Bytes(usize),
    #[default]
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
}
