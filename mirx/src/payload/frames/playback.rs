use super::{BlendMode, DisposalMode, FrameGroups, SectionedFramesError};
use crate::{
    PayloadLimits,
    image::{
        BufferRequirementError, BufferRequirements, EncodedImageError, Preflight, ReferenceMode,
        ScalarProfile, SurfaceMemoryPlan, SurfacePlanError, SurfaceRequirements, SurfaceView,
    },
};

/// Validated reconstruction of one presentation frame into a caller-owned canvas.
///
/// The canvas is initialized for an independent frame and retained for a
/// previous-reference frame. One caller-owned unit workspace is reused for all
/// selected units. This plan currently executes replacement composition with
/// keep disposal; other composition modes fail during planning.
#[derive(Debug)]
pub struct FrameDecodePlan<'a, 'g> {
    groups: FrameGroups<'a, 'g>,
    memory: SurfaceMemoryPlan,
    workspace: BufferRequirements,
    initialize_canvas: bool,
    units: u64,
    work: u64,
    input_bytes: u64,
    checksum_bytes: u64,
}

impl<'a, 'g> FrameGroups<'a, 'g> {
    /// Preflights scalar syntax, selected DATA integrity and caller memory needs.
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
        limits: &PayloadLimits,
    ) -> Result<FrameDecodePlan<'a, 'g>, FrameDecodeError> {
        let presentation = self.presentation();
        if presentation.composition().blend() != BlendMode::Replace {
            return Err(FrameDecodeError::UnsupportedBlend(
                presentation.composition().blend(),
            ));
        }
        if presentation.composition().disposal() != DisposalMode::Keep {
            return Err(FrameDecodeError::UnsupportedDisposal(
                presentation.composition().disposal(),
            ));
        }
        let memory = self
            .frames()
            .surface()
            .memory_plan(requirements)
            .map_err(FrameDecodeError::Memory)?;
        let initialize_canvas = self.reference() == ReferenceMode::Independent;
        let mut preflight = Preflight::new(limits, self.len()).map_err(FrameDecodeError::Image)?;
        if initialize_canvas {
            preflight
                .spend(u64::from(memory.byte_len()))
                .map_err(FrameDecodeError::Image)?;
        }
        let mut workspace = 0u32;
        let mut input_bytes = 0u64;
        let mut checksum_bytes = 0u64;
        for (group_index, group) in self.iter().enumerate() {
            preflight
                .group(group_index, group)
                .map_err(FrameDecodeError::Image)?;
            for (ordinal, unit) in group.iter().enumerate() {
                let unit_memory = unit
                    .memory_plan(SurfaceRequirements::new())
                    .expect("preflighted frame unit geometry");
                let profile =
                    ScalarProfile::new(unit.coding(), self.frames().surface().sample_layout())
                        .expect("preflighted scalar profile");
                let profile_work = profile
                    .extra_work(unit_memory)
                    .expect("preflighted scalar work");
                workspace = workspace.max(unit_memory.byte_len());
                input_bytes = input_bytes
                    .checked_add(unit.data().len() as u64)
                    .ok_or(FrameDecodeError::SizeOverflow)?;
                let check = self
                    .unit_check_plan(group_index, ordinal)
                    .map_err(FrameDecodeError::Frames)?;
                checksum_bytes = checksum_bytes
                    .checked_add(u64::from(check.byte_len()))
                    .ok_or(FrameDecodeError::SizeOverflow)?;
                preflight
                    .spend(u64::from(check.byte_len()))
                    .map_err(FrameDecodeError::Image)?;
                check
                    .verify()
                    .map_err(SectionedFramesError::Media)
                    .map_err(FrameDecodeError::Frames)?;
                preflight
                    .spend_replay(
                        unit.data().len(),
                        unit_memory.sample_byte_len(),
                        profile_work,
                    )
                    .map_err(FrameDecodeError::Image)?;
            }
        }
        Ok(FrameDecodePlan {
            groups: self,
            memory,
            workspace: BufferRequirements::new(workspace, 1).map_err(FrameDecodeError::Memory)?,
            initialize_canvas,
            units: preflight.total_units(),
            work: preflight.work(),
            input_bytes,
            checksum_bytes,
        })
    }
}

impl<'a> FrameDecodePlan<'a, '_> {
    pub const fn memory_plan(&self) -> SurfaceMemoryPlan {
        self.memory
    }

    pub const fn workspace_requirements(&self) -> BufferRequirements {
        self.workspace
    }

    pub const fn initializes_canvas(&self) -> bool {
        self.initialize_canvas
    }

    pub const fn unit_count(&self) -> u64 {
        self.units
    }

    pub const fn work(&self) -> u64 {
        self.work
    }

    pub const fn input_byte_len(&self) -> u64 {
        self.input_bytes
    }

    pub const fn checksum_byte_len(&self) -> u64 {
        self.checksum_bytes
    }

    /// Applies this frame after validating both caller buffers.
    ///
    /// A previous-reference frame requires `canvas` to contain the preceding
    /// presented frame. Binding errors leave both buffers unchanged.
    pub fn decode_into<'output>(
        self,
        canvas: &'output mut [u8],
        workspace: &mut [u8],
    ) -> Result<SurfaceView<'output>, FrameDecodeError>
    where
        'a: 'output,
    {
        self.memory
            .buffer_requirements()
            .validate(canvas)
            .map_err(FrameDecodeError::Canvas)?;
        self.workspace
            .validate(workspace)
            .map_err(FrameDecodeError::Workspace)?;
        let canvas = &mut canvas[..self.memory.byte_len() as usize];
        if self.initialize_canvas {
            canvas.fill(0);
        }
        for group in self.groups.iter() {
            for unit in group.iter() {
                let decoded = unit
                    .decode_plan(SurfaceRequirements::new())
                    .expect("immutable preflighted frame unit")
                    .decode_into(workspace)
                    .expect("validated frame unit workspace");
                decoded
                    .copy_into(canvas, self.memory)
                    .expect("validated frame unit placement");
            }
        }
        Ok(SurfaceView::from_plan(
            self.memory,
            canvas,
            self.groups.frames().color_table(),
        ))
    }
}

/// Unsupported frame composition, exhausted limits, or invalid caller storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameDecodeError {
    Frames(SectionedFramesError),
    Image(EncodedImageError),
    UnsupportedBlend(BlendMode),
    UnsupportedDisposal(DisposalMode),
    Memory(SurfacePlanError),
    Canvas(BufferRequirementError),
    Workspace(BufferRequirementError),
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FrameSequence, SectionedFramesAsset, SectionedFramesView,
        image::{
            ColorDescription, CoverageBudget, SampleLayout, SurfaceDescriptor, UnitGroupRecord,
        },
        media::{CodingRecord, UnitSelectionEncoding},
    };

    #[repr(align(64))]
    struct Aligned<const N: usize>([u8; N]);

    #[test]
    fn independent_then_previous_frame_reuses_aligned_canvas_and_unit_workspace() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let codings = [CodingRecord::RAW];
        let groups = [
            UnitGroupRecord::new(0, 0..2).unwrap(),
            UnitGroupRecord::new(0, 2..3)
                .unwrap()
                .with_tiles(1, 1)
                .with_selection(crate::image::GroupSelection::List(1))
                .with_reference(ReferenceMode::Previous),
        ];
        let mut index = [0; 4];
        UnitSelectionEncoding::List
            .encode_into(2, &[1], &mut index)
            .unwrap();
        let bytes =
            SectionedFramesAsset::new(sequence, surface, &codings, &groups, &[1, 1], &[1, 2, 9])
                .unwrap()
                .with_index(&index)
                .encode()
                .unwrap();
        let frames = SectionedFramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let requirements = SurfaceRequirements::new()
            .with_base_alignment(64)
            .with_stride_multiple(64);
        let mut canvas = Aligned([0xad; 64]);
        let mut workspace = [0; 2];

        let mut slots = [None];
        let first = frames
            .groups_into(0, &mut slots, &mut CoverageBudget::new(100))
            .unwrap()
            .decode_plan(requirements, &PayloadLimits::HOST)
            .unwrap();
        assert!(first.initializes_canvas());
        assert_eq!(first.memory_plan().byte_len(), 64);
        assert_eq!(first.workspace_requirements().byte_len(), 2);
        {
            let decoded = first.decode_into(&mut canvas.0, &mut workspace).unwrap();
            assert_eq!(decoded.plane(0).unwrap().row(0).unwrap(), Some(&[1, 2][..]));
        }
        assert!(canvas.0[2..].iter().all(|&byte| byte == 0));

        let second = frames
            .groups_into(1, &mut slots, &mut CoverageBudget::new(100))
            .unwrap()
            .decode_plan(requirements, &PayloadLimits::HOST)
            .unwrap();
        assert!(!second.initializes_canvas());
        assert_eq!(second.workspace_requirements().byte_len(), 1);
        {
            let decoded = second.decode_into(&mut canvas.0, &mut workspace).unwrap();
            assert_eq!(decoded.plane(0).unwrap().row(0).unwrap(), Some(&[1, 9][..]));
        }
        assert!(canvas.0[2..].iter().all(|&byte| byte == 0));
    }
}
