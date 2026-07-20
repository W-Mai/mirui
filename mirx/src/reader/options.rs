use super::PayloadLimits;

/// Handling for bytes after the MIRX logical file boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrailingBytesPolicy {
    #[default]
    Reject,
    Preserve,
}

/// Limits and preservation policy applied while opening a MIRX source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    max_chunks: u16,
    payload_limits: PayloadLimits,
    trailing_bytes: TrailingBytesPolicy,
}

impl ReadOptions {
    pub const DEFAULT_MAX_CHUNKS: u16 = 4_096;

    pub const fn new() -> Self {
        Self {
            max_chunks: Self::DEFAULT_MAX_CHUNKS,
            payload_limits: PayloadLimits::EMBEDDED,
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

    pub const fn with_payload_limits(mut self, limits: PayloadLimits) -> Self {
        self.payload_limits = limits;
        self
    }

    pub const fn payload_limits(&self) -> PayloadLimits {
        self.payload_limits
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_payload_limits_are_the_default() {
        let options = ReadOptions::default();
        assert_eq!(options.payload_limits(), PayloadLimits::EMBEDDED);
        assert_eq!(options.max_chunks(), ReadOptions::DEFAULT_MAX_CHUNKS);
        assert_eq!(options.trailing_bytes_policy(), TrailingBytesPolicy::Reject);
    }

    #[test]
    fn payload_limits_compose_with_structural_options() {
        let custom = PayloadLimits::HOST.with_max_decoded_bytes(512);
        let options = ReadOptions::new()
            .with_max_chunks(7)
            .with_payload_limits(custom)
            .with_trailing_bytes(TrailingBytesPolicy::Preserve);
        assert_eq!(options.max_chunks(), 7);
        assert_eq!(options.payload_limits(), custom);
        assert_eq!(
            options.trailing_bytes_policy(),
            TrailingBytesPolicy::Preserve
        );
    }
}
