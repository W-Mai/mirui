#![doc = include_str!("../../docs/decode-memory.md")]

use super::SurfaceRequirements;

/// How encoded samples are intended to reach their consumer.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DecodeExecution {
    /// Reconstruct ordinary sample planes into caller-owned memory.
    #[default]
    Reconstruct,
    /// Reconstruct through a compute backend into a backend-visible surface.
    Compute,
    /// Preserve a backend-native compressed layout for direct upload.
    DirectUpload,
}

/// Where a buffer is stored and which agents can access it.
///
/// Shared memory may be cache coherent or require explicit cache maintenance.
/// The variants encode only access semantics; they do not name a particular
/// allocator, bus, GPU, or firmware implementation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum MemoryPlacement {
    /// CPU-readable and CPU-writable memory with no device visibility promise.
    #[default]
    Cpu,
    /// Immutable memory-mapped storage readable by the CPU.
    Flash,
    /// CPU-readable, CPU-writable, and device-visible coherent memory.
    SharedCoherent,
    /// CPU-readable, CPU-writable, and device-visible memory requiring cache maintenance.
    SharedNoncoherent,
    /// Device-visible memory without CPU access.
    Device,
}

impl MemoryPlacement {
    pub const fn is_cpu_readable(self) -> bool {
        matches!(
            self,
            Self::Cpu | Self::Flash | Self::SharedCoherent | Self::SharedNoncoherent
        )
    }

    pub const fn is_cpu_writable(self) -> bool {
        matches!(
            self,
            Self::Cpu | Self::SharedCoherent | Self::SharedNoncoherent
        )
    }

    pub const fn is_device_visible(self) -> bool {
        matches!(
            self,
            Self::SharedCoherent | Self::SharedNoncoherent | Self::Device
        )
    }

    pub const fn requires_explicit_sync(self) -> bool {
        matches!(self, Self::SharedNoncoherent)
    }
}

/// Cache operation owed by the caller at a decode boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CacheSync {
    #[default]
    None,
    /// Invalidate cached input before planning or executing a CPU read.
    InvalidateBeforeRead,
    /// Clean decoded output before a device reads it.
    CleanAfterWrite,
}

/// Execution and memory contract for one image decode.
///
/// `new` selects CPU reconstruction into CPU memory. Optional target policy is
/// configured fluently while physical output geometry remains owned by
/// [`SurfaceRequirements`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeRequest {
    requirements: SurfaceRequirements,
    execution: DecodeExecution,
    input: MemoryPlacement,
    output: MemoryPlacement,
    workspace: MemoryPlacement,
    workspace_alignment: u32,
}

impl DecodeRequest {
    pub const fn new(requirements: SurfaceRequirements) -> Self {
        Self {
            requirements,
            execution: DecodeExecution::Reconstruct,
            input: MemoryPlacement::Cpu,
            output: MemoryPlacement::Cpu,
            workspace: MemoryPlacement::Cpu,
            workspace_alignment: 1,
        }
    }

    pub const fn with_execution(mut self, execution: DecodeExecution) -> Self {
        self.execution = execution;
        self
    }

    pub const fn with_input(mut self, placement: MemoryPlacement) -> Self {
        self.input = placement;
        self
    }

    pub const fn with_output(mut self, placement: MemoryPlacement) -> Self {
        self.output = placement;
        self
    }

    pub const fn with_workspace(mut self, placement: MemoryPlacement) -> Self {
        self.workspace = placement;
        self
    }

    pub const fn with_workspace_alignment(mut self, alignment: u32) -> Self {
        self.workspace_alignment = alignment;
        self
    }

    pub const fn requirements(self) -> SurfaceRequirements {
        self.requirements
    }

    pub const fn execution(self) -> DecodeExecution {
        self.execution
    }

    pub const fn input(self) -> MemoryPlacement {
        self.input
    }

    pub const fn output(self) -> MemoryPlacement {
        self.output
    }

    pub const fn workspace(self) -> MemoryPlacement {
        self.workspace
    }

    pub const fn workspace_alignment(self) -> u32 {
        self.workspace_alignment
    }

    /// Cache action required before this CPU decoder reads encoded input.
    pub const fn input_sync(self) -> CacheSync {
        if self.input.requires_explicit_sync() {
            CacheSync::InvalidateBeforeRead
        } else {
            CacheSync::None
        }
    }

    /// Cache action required before a device consumes reconstructed output.
    pub const fn output_sync(self) -> CacheSync {
        if self.output.requires_explicit_sync() {
            CacheSync::CleanAfterWrite
        } else {
            CacheSync::None
        }
    }

    pub(crate) const fn validate_reconstruction(self) -> Result<(), DecodeRequestError> {
        if !matches!(self.execution, DecodeExecution::Reconstruct) {
            return Err(DecodeRequestError::UnsupportedExecution(self.execution));
        }
        if !self.input.is_cpu_readable() {
            return Err(DecodeRequestError::InputNotReadable(self.input));
        }
        if !self.output.is_cpu_writable() {
            return Err(DecodeRequestError::OutputNotWritable(self.output));
        }
        if !self.workspace.is_cpu_writable() {
            return Err(DecodeRequestError::WorkspaceNotWritable(self.workspace));
        }
        if !self.workspace_alignment.is_power_of_two() {
            return Err(DecodeRequestError::InvalidWorkspaceAlignment(
                self.workspace_alignment,
            ));
        }
        Ok(())
    }
}

impl Default for DecodeRequest {
    fn default() -> Self {
        Self::new(SurfaceRequirements::new())
    }
}

/// Unsupported execution or buffer access requested from the CPU decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DecodeRequestError {
    UnsupportedExecution(DecodeExecution),
    InputNotReadable(MemoryPlacement),
    OutputNotWritable(MemoryPlacement),
    WorkspaceNotWritable(MemoryPlacement),
    InvalidWorkspaceAlignment(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_expose_access_without_invalid_combinations() {
        assert!(MemoryPlacement::Flash.is_cpu_readable());
        assert!(!MemoryPlacement::Flash.is_cpu_writable());
        assert!(!MemoryPlacement::Flash.is_device_visible());
        assert!(MemoryPlacement::SharedCoherent.is_device_visible());
        assert!(MemoryPlacement::SharedNoncoherent.requires_explicit_sync());
        assert!(!MemoryPlacement::Device.is_cpu_readable());
    }

    #[test]
    fn request_reports_directional_cache_boundaries() {
        let request = DecodeRequest::default()
            .with_input(MemoryPlacement::SharedNoncoherent)
            .with_output(MemoryPlacement::SharedNoncoherent);
        assert_eq!(request.input_sync(), CacheSync::InvalidateBeforeRead);
        assert_eq!(request.output_sync(), CacheSync::CleanAfterWrite);
        assert_eq!(request.validate_reconstruction(), Ok(()));
    }

    #[test]
    fn reconstruction_rejects_unreachable_buffers_and_other_execution() {
        assert_eq!(
            DecodeRequest::default()
                .with_execution(DecodeExecution::Compute)
                .validate_reconstruction(),
            Err(DecodeRequestError::UnsupportedExecution(
                DecodeExecution::Compute
            ))
        );
        assert_eq!(
            DecodeRequest::default()
                .with_input(MemoryPlacement::Device)
                .validate_reconstruction(),
            Err(DecodeRequestError::InputNotReadable(
                MemoryPlacement::Device
            ))
        );
        assert_eq!(
            DecodeRequest::default()
                .with_output(MemoryPlacement::Flash)
                .validate_reconstruction(),
            Err(DecodeRequestError::OutputNotWritable(
                MemoryPlacement::Flash
            ))
        );
        assert_eq!(
            DecodeRequest::default()
                .with_workspace(MemoryPlacement::Device)
                .validate_reconstruction(),
            Err(DecodeRequestError::WorkspaceNotWritable(
                MemoryPlacement::Device
            ))
        );
        assert_eq!(
            DecodeRequest::default()
                .with_workspace_alignment(3)
                .validate_reconstruction(),
            Err(DecodeRequestError::InvalidWorkspaceAlignment(3))
        );
    }
}
