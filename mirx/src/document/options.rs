use super::RawChunkPolicy;
use crate::{ChunkType, PayloadLimits, TrailingBytesPolicy};

/// Selection policy for an encoded document layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum LayoutPolicy {
    /// Retains the current layout until an edit requires CHUNK representation.
    #[default]
    PreserveOrPromote,
    /// Selects the smallest layout that can represent the document without loss.
    SmallestRepresentable,
    /// Requires a lossless FLAT representation.
    ForceFlat,
    /// Requires a CHUNK representation.
    ForceChunk,
}

/// Options controlling checked document encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodeOptions {
    layout_policy: LayoutPolicy,
}

impl EncodeOptions {
    pub const fn new() -> Self {
        Self {
            layout_policy: LayoutPolicy::PreserveOrPromote,
        }
    }

    pub const fn with_layout_policy(mut self, policy: LayoutPolicy) -> Self {
        self.layout_policy = policy;
        self
    }

    pub const fn layout_policy(&self) -> LayoutPolicy {
        self.layout_policy
    }
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Type-wide raw capability policy applied while opening a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawTypePolicy {
    pub chunk_type: ChunkType,
    pub policy: RawChunkPolicy,
}

/// Limits and capability grants used during open.
///
/// Raw type policies are searched from the end, so a later duplicate takes
/// precedence. The policy slice is not retained after opening; evaluated
/// capabilities are copied into the corresponding document nodes. Payload
/// limits are copied into the document for later typed access and edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenOptions<'p> {
    max_chunks: u16,
    payload_limits: PayloadLimits,
    trailing_bytes: TrailingBytesPolicy,
    raw_type_policies: &'p [RawTypePolicy],
}

impl<'p> OpenOptions<'p> {
    pub const DEFAULT_MAX_CHUNKS: u16 = 256;
    pub const HOST_MAX_CHUNKS: u16 = 4_096;

    pub const fn new() -> Self {
        Self {
            max_chunks: Self::DEFAULT_MAX_CHUNKS,
            payload_limits: PayloadLimits::EMBEDDED,
            trailing_bytes: TrailingBytesPolicy::Reject,
            raw_type_policies: &[],
        }
    }

    /// Creates an explicit larger profile for host-side tools.
    pub const fn host_tools() -> Self {
        Self {
            max_chunks: Self::HOST_MAX_CHUNKS,
            payload_limits: PayloadLimits::HOST,
            trailing_bytes: TrailingBytesPolicy::Reject,
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

    /// Selects whether bytes after the logical MIRX boundary may be retained.
    ///
    /// Preserved trailing bytes keep the document read-only until
    /// [`Document::discard_trailing_bytes`](super::Document::discard_trailing_bytes)
    /// is called.
    pub const fn with_trailing_bytes(mut self, policy: TrailingBytesPolicy) -> Self {
        self.trailing_bytes = policy;
        self
    }

    /// Returns the selected trailing-byte policy.
    pub const fn trailing_bytes_policy(&self) -> TrailingBytesPolicy {
        self.trailing_bytes
    }

    /// Sets the ordered type policies used to classify opened raw nodes.
    ///
    /// For current container semantics,
    /// [`ReservedBitsPolicy::Preserve`](crate::ReservedBitsPolicy::Preserve)
    /// grants reserved-bit preservation and
    /// [`ReservedBitsPolicy::Normalize`](crate::ReservedBitsPolicy::Normalize)
    /// clears those bits and marks the document dirty. The default reject
    /// policy leaves opened bits unchanged without granting preservation.
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

    #[test]
    fn encode_options_are_const_and_keep_layout_policy_explicit() {
        const DEFAULT: EncodeOptions = EncodeOptions::new();
        const FORCED: EncodeOptions =
            EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk);

        assert_eq!(DEFAULT.layout_policy(), LayoutPolicy::PreserveOrPromote);
        assert_eq!(EncodeOptions::default(), DEFAULT);
        assert_eq!(FORCED.layout_policy(), LayoutPolicy::ForceChunk);
    }

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
        assert_eq!(DEFAULT.trailing_bytes_policy(), TrailingBytesPolicy::Reject);
        assert!(DEFAULT.raw_type_policies().is_empty());
        assert_eq!(OpenOptions::default(), DEFAULT);

        assert_eq!(HOST.max_chunks(), 4_096);
        assert_eq!(HOST.payload_limits(), PayloadLimits::HOST);
        assert_eq!(HOST.trailing_bytes_policy(), TrailingBytesPolicy::Reject);
        assert!(HOST.raw_type_policies().is_empty());
    }

    #[test]
    fn builders_retain_the_borrowed_policy_slice_without_allocation() {
        const POLICIES: [RawTypePolicy; 1] = [POLICY];
        const OPTIONS: OpenOptions<'static> = OpenOptions::new()
            .with_max_chunks(17)
            .with_payload_limits(PayloadLimits::HOST)
            .with_trailing_bytes(TrailingBytesPolicy::Preserve)
            .with_raw_type_policies(&POLICIES);

        assert_eq!(OPTIONS.max_chunks(), 17);
        assert_eq!(OPTIONS.payload_limits(), PayloadLimits::HOST);
        assert_eq!(
            OPTIONS.trailing_bytes_policy(),
            TrailingBytesPolicy::Preserve
        );
        assert_eq!(OPTIONS.raw_type_policies(), &POLICIES);
        assert_eq!(OPTIONS.raw_type_policies().as_ptr(), POLICIES.as_ptr());
        assert!(size_of::<RawTypePolicy>() <= 8);
        // Includes inline media limits and a 64-bit raster work budget.
        #[cfg(target_pointer_width = "32")]
        assert!(size_of::<OpenOptions<'_>>() <= 88);
        #[cfg(target_pointer_width = "64")]
        assert!(size_of::<OpenOptions<'_>>() <= 112);
    }
}
