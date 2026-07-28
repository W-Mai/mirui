use super::RawChunkPolicy;
use crate::{ChunkType, PayloadLimits};

/// Type-wide raw capability policy applied while opening a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawTypePolicy {
    pub chunk_type: ChunkType,
    pub policy: RawChunkPolicy,
}

/// Limits and per-type capability grants used while opening a document.
///
/// Raw type policies are searched from the end, so a later duplicate takes
/// precedence. The policy slice is not retained after opening; evaluated
/// capabilities are copied into the corresponding document nodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenOptions<'p> {
    max_chunks: u16,
    payload_limits: PayloadLimits,
    raw_type_policies: &'p [RawTypePolicy],
}

impl<'p> OpenOptions<'p> {
    pub const DEFAULT_MAX_CHUNKS: u16 = 256;
    pub const HOST_MAX_CHUNKS: u16 = 4_096;

    pub const fn new() -> Self {
        Self {
            max_chunks: Self::DEFAULT_MAX_CHUNKS,
            payload_limits: PayloadLimits::EMBEDDED,
            raw_type_policies: &[],
        }
    }

    /// Creates an explicit larger profile for host-side tools.
    pub const fn host_tools() -> Self {
        Self {
            max_chunks: Self::HOST_MAX_CHUNKS,
            payload_limits: PayloadLimits::HOST,
            raw_type_policies: &[],
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

    /// Sets the ordered type policies used to classify opened raw nodes.
    ///
    /// For current container semantics,
    /// [`ReservedBitsPolicy::Preserve`](crate::ReservedBitsPolicy::Preserve)
    /// grants reserved-bit preservation and
    /// [`ReservedBitsPolicy::Normalize`](crate::ReservedBitsPolicy::Normalize)
    /// clears those bits and marks the document dirty. The default reject
    /// policy leaves opened bits unchanged without granting preservation.
    /// Higher-minor or flagged containers are not normalized by this option.
    pub const fn with_raw_type_policies(mut self, policies: &'p [RawTypePolicy]) -> Self {
        self.raw_type_policies = policies;
        self
    }

    pub const fn raw_type_policies(&self) -> &'p [RawTypePolicy] {
        self.raw_type_policies
    }
}

impl Default for OpenOptions<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use core::mem::size_of;

    use super::*;
    use crate::{CriticalAssumption, RelocationAssumption, ReservedBitsPolicy};

    const POLICY: RawTypePolicy = RawTypePolicy {
        chunk_type: ChunkType::META,
        policy: RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Preserve,
        },
    };

    #[test]
    fn default_and_host_profiles_are_explicit_and_const_constructible() {
        const DEFAULT: OpenOptions<'static> = OpenOptions::new();
        const HOST: OpenOptions<'static> = OpenOptions::host_tools();

        assert_eq!(DEFAULT.max_chunks(), 256);
        assert_eq!(DEFAULT.payload_limits(), PayloadLimits::EMBEDDED);
        assert!(DEFAULT.raw_type_policies().is_empty());
        assert_eq!(OpenOptions::default(), DEFAULT);

        assert_eq!(HOST.max_chunks(), 4_096);
        assert_eq!(HOST.payload_limits(), PayloadLimits::HOST);
        assert!(HOST.raw_type_policies().is_empty());
    }

    #[test]
    fn builders_retain_the_borrowed_policy_slice_without_allocation() {
        const POLICIES: [RawTypePolicy; 1] = [POLICY];
        const OPTIONS: OpenOptions<'static> = OpenOptions::new()
            .with_max_chunks(17)
            .with_payload_limits(PayloadLimits::HOST)
            .with_raw_type_policies(&POLICIES);

        assert_eq!(OPTIONS.max_chunks(), 17);
        assert_eq!(OPTIONS.payload_limits(), PayloadLimits::HOST);
        assert_eq!(OPTIONS.raw_type_policies(), &POLICIES);
        assert_eq!(OPTIONS.raw_type_policies().as_ptr(), POLICIES.as_ptr());
        assert!(size_of::<RawTypePolicy>() <= 8);
        #[cfg(target_pointer_width = "32")]
        assert!(size_of::<OpenOptions<'_>>() <= 64);
        #[cfg(target_pointer_width = "64")]
        assert!(size_of::<OpenOptions<'_>>() <= 80);
    }
}
