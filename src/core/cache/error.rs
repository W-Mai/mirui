#[derive(Debug)]
pub enum CacheError<E = core::convert::Infallible> {
    Disabled,
    TooLarge,
    Factory(E),
}

impl<E: core::fmt::Display> core::fmt::Display for CacheError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CacheError::Disabled => f.write_str("cache disabled (max_size = 0)"),
            CacheError::TooLarge => f.write_str("entry exceeds cache max_size"),
            CacheError::Factory(e) => write!(f, "factory error: {e}"),
        }
    }
}
