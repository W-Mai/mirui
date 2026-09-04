use super::{EncodedImageError, ImageGroups, preflight::Preflight};
use crate::image::units::ScalarProfile;
use crate::{
    PayloadLimits,
    image::{
        BufferRequirementError, BufferRequirements, RegionMemoryPlan, SurfaceMemoryPlan,
        SurfacePlanError, SurfaceRequirements, SurfaceView,
    },
};

mod region;
use region::DecodeScope;

/// Scalar image reconstruction using one reusable caller-owned unit buffer.
///
/// Requested units and DATA integrity are preflighted. Prepared group slots and input
/// remain borrowed; no per-unit plan array or decoded storage is allocated.
#[derive(Clone, Copy, Debug)]
pub struct ImageDecodePlan<'a, 'g> {
    groups: ImageGroups<'a, 'g>,
    memory: RegionMemoryPlan,
    scope: DecodeScope,
    workspace: BufferRequirements,
    units: u64,
    work: u64,
    input_bytes: u64,
    checksum_bytes: u64,
}

impl<'a, 'g> ImageGroups<'a, 'g> {
    /// Preflights every scalar unit and plans complete image reconstruction.
    ///
    /// Group preparation has already validated exact coverage. This work budget
    /// covers output initialization, complete DATA checksums, unit preflight,
    /// execution-time re-preflight/replay and placement. The decoded-byte limit
    /// applies to one tight unit; the final allocation is bounded by checked
    /// geometry and the work budget. No decoded bytes or plan array are allocated.
    ///
    /// ```
    /// use mirx::{PayloadLimits, coding::Rle, image::{ColorDescription,
    ///     CoverageBudget, EncodedImageAsset, EncodedImageView, SampleLayout,
    ///     SurfaceDescriptor, SurfaceRequirements}};
    /// let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]).encode().unwrap();
    /// let image = EncodedImageView::open(&bytes).unwrap();
    /// let mut slots = [None];
    /// let groups = image.groups_into(&mut slots, &mut CoverageBudget::new(100)).unwrap();
    /// let plan = groups.decode_plan(SurfaceRequirements::new().with_stride_multiple(8),
    ///     &PayloadLimits::EMBEDDED).unwrap();
    /// assert_eq!(plan.memory_plan().byte_len(), 16);
    /// assert_eq!(plan.workspace_requirements().byte_len(), 8);
    /// let mut output = [0; 16];
    /// let mut workspace = [0; 8];
    /// let decoded = plan.decode_into(&mut output, &mut workspace).unwrap();
    /// assert_eq!(decoded.plane(0).unwrap().row(1).unwrap(), Some(&[42; 4][..]));
    /// ```
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
        limits: &PayloadLimits,
    ) -> Result<ImageDecodePlan<'a, 'g>, DecodeError> {
        let mut preflight = Preflight::new(limits, self.len()).map_err(DecodeError::Image)?;
        let memory = self
            .image()
            .surface()
            .memory_plan(requirements)
            .map_err(DecodeError::Memory)?;
        preflight
            .spend(u64::from(memory.byte_len()) + u64::from(self.image.checksum_bytes))
            .map_err(DecodeError::Image)?;
        let mut workspace = 0;
        let mut input_bytes = 0;
        for (index, group) in self.iter().enumerate() {
            preflight.group(index, group).map_err(DecodeError::Image)?;
            for unit in group.iter() {
                let unit_memory = unit
                    .memory_plan(SurfaceRequirements::new())
                    .expect("preflighted unit geometry");
                let profile =
                    ScalarProfile::new(unit.coding(), self.image().surface().sample_layout())
                        .expect("preflighted scalar profile");
                let profile_work = profile
                    .extra_work(unit_memory)
                    .expect("preflighted profile work");
                workspace = workspace.max(unit_memory.byte_len());
                input_bytes += unit.data().len() as u64;
                preflight
                    .spend_replay(
                        unit.data().len(),
                        unit_memory.sample_byte_len(),
                        profile_work,
                    )
                    .map_err(DecodeError::Image)?;
            }
        }
        self.image().validate_data().map_err(DecodeError::Image)?;
        Ok(ImageDecodePlan {
            groups: self,
            memory: RegionMemoryPlan::whole(memory),
            scope: DecodeScope::Whole,
            workspace: BufferRequirements::new(workspace, 1).map_err(DecodeError::Memory)?,
            units: preflight.total_units(),
            work: preflight.work(),
            input_bytes,
            checksum_bytes: u64::from(self.image.checksum_bytes),
        })
    }
}

impl<'a> ImageDecodePlan<'a, '_> {
    pub const fn memory_plan(self) -> SurfaceMemoryPlan {
        self.memory.memory_plan()
    }

    /// Exact original region and its independently aligned output allocation.
    pub const fn region_plan(self) -> RegionMemoryPlan {
        self.memory
    }

    /// Selected encoded bytes, excluding alignment gaps and metadata.
    pub const fn input_byte_len(self) -> u64 {
        self.input_bytes
    }

    /// Actual DATA checksum bytes, including required partition expansion.
    pub const fn checksum_byte_len(self) -> u64 {
        self.checksum_bytes
    }

    /// Largest tight selected unit, reused during execution; scalar alignment is one.
    /// A whole-image encoded stream requires whole-image staging with this path.
    pub const fn workspace_requirements(self) -> BufferRequirements {
        self.workspace
    }

    pub const fn unit_count(self) -> u64 {
        self.units
    }

    /// Charged preparation and execution work, excluding prior group preparation.
    pub const fn work(self) -> u64 {
        self.work
    }

    /// Reconstructs the requested samples after validating both caller buffers.
    ///
    /// Binding errors leave output and workspace unchanged. Successful output
    /// has zero physical padding; both buffer suffixes are preserved. Immutable
    /// preflighted input is rechecked/replayed within the planned work bound.
    /// Samples borrow output; an indexed palette stays borrowed from the source.
    pub fn decode_into<'output>(
        self,
        output: &'output mut [u8],
        workspace: &mut [u8],
    ) -> Result<SurfaceView<'output>, DecodeError>
    where
        'a: 'output,
    {
        let memory = self.memory_plan();
        memory
            .buffer_requirements()
            .validate(output)
            .map_err(DecodeError::Output)?;
        self.workspace
            .validate(workspace)
            .map_err(DecodeError::Workspace)?;
        let output = &mut output[..memory.byte_len() as usize];
        output.fill(0);
        for group in self.groups.iter() {
            for unit in self.scope.units(group, self.memory) {
                let plan = unit
                    .decode_plan(SurfaceRequirements::new())
                    .expect("immutable preflighted unit");
                let decoded = plan
                    .decode_into(workspace)
                    .expect("validated unit workspace");
                decoded
                    .copy_region_into(output, self.memory)
                    .expect("validated unit placement");
            }
        }
        Ok(SurfaceView::from_plan(
            memory,
            output,
            self.groups.image().color_table(),
        ))
    }
}

/// Unsupported image execution, exhausted limits, or invalid caller storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DecodeError {
    Image(EncodedImageError),
    Memory(SurfacePlanError),
    Output(BufferRequirementError),
    Workspace(BufferRequirementError),
}

#[cfg(test)]
mod tests;
