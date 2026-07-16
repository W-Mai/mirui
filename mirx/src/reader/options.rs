/// Structural limits applied while opening a MIRX source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    max_chunks: u16,
}

impl ReadOptions {
    pub const DEFAULT_MAX_CHUNKS: u16 = 4_096;

    pub const fn new() -> Self {
        Self {
            max_chunks: Self::DEFAULT_MAX_CHUNKS,
        }
    }

    pub const fn with_max_chunks(mut self, max_chunks: u16) -> Self {
        self.max_chunks = max_chunks;
        self
    }

    pub const fn max_chunks(&self) -> u16 {
        self.max_chunks
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self::new()
    }
}
