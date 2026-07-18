/// Handling for bytes after the MIRX logical file boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrailingBytesPolicy {
    #[default]
    Reject,
    Preserve,
}

/// Structural limits applied while opening a MIRX source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    max_chunks: u16,
    trailing_bytes: TrailingBytesPolicy,
}

impl ReadOptions {
    pub const DEFAULT_MAX_CHUNKS: u16 = 4_096;

    pub const fn new() -> Self {
        Self {
            max_chunks: Self::DEFAULT_MAX_CHUNKS,
            trailing_bytes: TrailingBytesPolicy::Reject,
        }
    }

    pub const fn with_max_chunks(mut self, max_chunks: u16) -> Self {
        self.max_chunks = max_chunks;
        self
    }

    pub const fn max_chunks(&self) -> u16 {
        self.max_chunks
    }

    pub const fn with_trailing_bytes(mut self, policy: TrailingBytesPolicy) -> Self {
        self.trailing_bytes = policy;
        self
    }

    pub const fn trailing_bytes_policy(&self) -> TrailingBytesPolicy {
        self.trailing_bytes
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self::new()
    }
}
