use super::{BlendMode, DisposalMode, FrameGroups, FramesError, FramesView};
use crate::{
    PayloadLimits,
    image::{
        BufferRequirementError, BufferRequirements, CoverageBudget, EncodedImageError, Preflight,
        ReferenceMode, ScalarProfile, SurfaceMemoryPlan, SurfacePlanError, SurfaceRequirements,
        SurfaceView, UnitGroup,
    },
};

/// Validated reconstruction of one presentation frame into a caller-owned canvas.
///
/// The canvas is initialized for an independent frame and retained for a
/// previous-reference frame. One caller-owned unit workspace is reused for all
/// selected units. Disposal is applied explicitly after presentation so its
/// lifetime cannot be hidden inside decoding.
#[derive(Debug)]
pub struct FrameDecodePlan<'a, 'g> {
    groups: FrameGroups<'a, 'g>,
    memory: SurfaceMemoryPlan,
    workspace: BufferRequirements,
    backup: BufferRequirements,
    composition: super::FrameComposition,
    initialize_canvas: bool,
    units: u64,
    work: u64,
    input_bytes: u64,
    checksum_bytes: u64,
}

/// Checked post-presentation operation for one frame.
#[derive(Debug)]
pub struct FrameDisposalPlan<'a, 'g> {
    groups: FrameGroups<'a, 'g>,
    memory: SurfaceMemoryPlan,
    backup: BufferRequirements,
    composition: super::FrameComposition,
    work: u64,
}

impl<'a, 'g> FrameGroups<'a, 'g> {
    /// Plans disposal without scanning or decoding the frame payload.
    pub fn disposal_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<FrameDisposalPlan<'a, 'g>, FrameDecodeError> {
        let memory = self
            .frames()
            .surface()
            .memory_plan(requirements)
            .map_err(FrameDecodeError::Memory)?;
        let composition = self.presentation().composition();
        let (backup, work) = FrameDisposalPlan::requirements(memory, composition)?;
        Ok(FrameDisposalPlan {
            groups: self,
            memory,
            backup,
            composition,
            work,
        })
    }
}

impl<'a, 'g> FrameGroups<'a, 'g> {
    /// Preflights scalar syntax, selected DATA integrity and caller memory needs.
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
        limits: &PayloadLimits,
    ) -> Result<FrameDecodePlan<'a, 'g>, FrameDecodeError> {
        let presentation = self.presentation();
        let composition = presentation.composition();
        let memory = self
            .frames()
            .surface()
            .memory_plan(requirements)
            .map_err(FrameDecodeError::Memory)?;
        if composition.blend() == BlendMode::SourceOver
            && !self
                .frames()
                .surface()
                .sample_layout()
                .supports_source_over()
        {
            return Err(FrameDecodeError::UnsupportedSourceOver(
                self.frames().surface().sample_layout(),
            ));
        }
        let initialize_canvas = self.reference() == ReferenceMode::Independent;
        let mut preflight = Preflight::new(limits, self.len()).map_err(FrameDecodeError::Image)?;
        if initialize_canvas {
            preflight
                .spend(u64::from(memory.byte_len()))
                .map_err(FrameDecodeError::Image)?;
        }
        let (backup, disposal_work) = FrameDisposalPlan::requirements(memory, composition)?;
        preflight
            .spend(disposal_work)
            .map_err(FrameDecodeError::Image)?;
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
                if composition.blend() == BlendMode::SourceOver {
                    preflight
                        .spend(
                            (unit_memory.sample_byte_len() as u64)
                                .checked_mul(4)
                                .ok_or(FrameDecodeError::SizeOverflow)?,
                        )
                        .map_err(FrameDecodeError::Image)?;
                }
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
                    .map_err(FramesError::Media)
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
            backup,
            composition,
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

    /// Retained storage needed until disposal; zero unless restore-previous is used.
    pub const fn backup_requirements(&self) -> BufferRequirements {
        self.backup
    }

    pub const fn composition(&self) -> super::FrameComposition {
        self.composition
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
        &self,
        canvas: &'output mut [u8],
        workspace: &mut [u8],
        backup: &mut [u8],
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
        self.backup
            .validate(backup)
            .map_err(FrameDecodeError::Backup)?;
        let canvas = &mut canvas[..self.memory.byte_len() as usize];
        if self.composition.disposal() == DisposalMode::RestorePrevious {
            let backup = &mut backup[..self.memory.byte_len() as usize];
            if self.initialize_canvas {
                backup.fill(0);
            } else {
                backup.copy_from_slice(canvas);
            }
        }
        if self.initialize_canvas {
            canvas.fill(0);
        }
        for group in self.groups.iter() {
            for unit in group.iter() {
                let plan = unit
                    .decode_plan(SurfaceRequirements::new())
                    .expect("immutable preflighted frame unit");
                let decoded = if unit.coding().id() == crate::CodingId::FRAME_DELTA {
                    let reference = SurfaceView::from_plan(
                        self.memory,
                        &*canvas,
                        self.groups.frames().color_table(),
                    );
                    plan.decode_from(reference, workspace)
                        .expect("validated frame reference and unit workspace")
                } else {
                    plan.decode_into(workspace)
                        .expect("validated frame unit workspace")
                };
                match self.composition.blend() {
                    BlendMode::Replace => decoded
                        .copy_into(canvas, self.memory)
                        .expect("validated frame unit placement"),
                    BlendMode::SourceOver => decoded
                        .source_over_into(canvas, self.memory)
                        .expect("validated frame source-over placement"),
                }
            }
        }
        Ok(SurfaceView::from_plan(
            self.memory,
            canvas,
            self.groups.frames().color_table(),
        ))
    }

    /// Applies this frame's post-presentation disposal to the retained canvas.
    pub fn dispose_into(&self, canvas: &mut [u8], backup: &[u8]) -> Result<(), FrameDecodeError> {
        FrameDisposalPlan {
            groups: self.groups.clone(),
            memory: self.memory,
            backup: self.backup,
            composition: self.composition,
            work: 0,
        }
        .dispose_into(canvas, backup)
    }
}

impl FrameDisposalPlan<'_, '_> {
    fn requirements(
        memory: SurfaceMemoryPlan,
        composition: super::FrameComposition,
    ) -> Result<(BufferRequirements, u64), FrameDecodeError> {
        let empty = BufferRequirements::new(0, 1).map_err(FrameDecodeError::Memory)?;
        match composition.disposal() {
            DisposalMode::Keep => Ok((empty, 0)),
            DisposalMode::Clear => Ok((empty, u64::from(memory.byte_len()))),
            DisposalMode::RestorePrevious => Ok((
                memory.buffer_requirements(),
                u64::from(memory.byte_len())
                    .checked_mul(2)
                    .ok_or(FrameDecodeError::SizeOverflow)?,
            )),
        }
    }

    pub const fn backup_requirements(&self) -> BufferRequirements {
        self.backup
    }

    pub const fn work(&self) -> u64 {
        self.work
    }

    /// Applies this frame's disposal without reading its encoded unit bytes.
    pub fn dispose_into(&self, canvas: &mut [u8], backup: &[u8]) -> Result<(), FrameDecodeError> {
        self.memory
            .buffer_requirements()
            .validate(canvas)
            .map_err(FrameDecodeError::Canvas)?;
        self.backup
            .validate(backup)
            .map_err(FrameDecodeError::Backup)?;
        let canvas = &mut canvas[..self.memory.byte_len() as usize];
        match self.composition.disposal() {
            DisposalMode::Keep => {}
            DisposalMode::Clear => {
                if let Some(region) = self.composition.region() {
                    self.memory.clear_region(canvas, region);
                } else {
                    for group in self.groups.iter() {
                        for unit in group.iter() {
                            for plane in 0..self.memory.surface().plane_count() {
                                if let Some(region) = unit.plane_region(plane) {
                                    self.memory.clear_plane_region(canvas, plane, region);
                                }
                            }
                        }
                    }
                }
            }
            DisposalMode::RestorePrevious => {
                canvas.copy_from_slice(&backup[..self.memory.byte_len() as usize]);
            }
        }
        Ok(())
    }
}

/// Stateful, allocation-free playback over a sectioned frame sequence.
///
/// All mutable storage is borrowed for the session lifetime. This keeps the
/// canvas and restore-previous snapshot paired across sequential calls while
/// allowing random seeks to restart from the closest recovery frame.
#[derive(Debug)]
pub struct FrameSession<'a, 'storage> {
    frames: FramesView<'a>,
    requirements: SurfaceRequirements,
    limits: PayloadLimits,
    memory: SurfaceMemoryPlan,
    group_workspace_len: usize,
    backup_requirements: BufferRequirements,
    group_workspace: &'storage mut [Option<UnitGroup<'a>>],
    canvas: &'storage mut [u8],
    unit_workspace: &'storage mut [u8],
    backup: &'storage mut [u8],
    current: Option<u32>,
}

/// Preflighted storage contract for allocation-free frame playback.
///
/// Planning validates every frame representation once and retains only the
/// largest caller-buffer requirements. Binding does not read encoded DATA or
/// allocate; the returned session reuses the supplied storage for every frame.
#[derive(Clone, Copy, Debug)]
pub struct FramesPlaybackPlan<'a> {
    frames: FramesView<'a>,
    requirements: SurfaceRequirements,
    limits: PayloadLimits,
    memory: SurfaceMemoryPlan,
    group_workspace_len: usize,
    workspace: BufferRequirements,
    backup: BufferRequirements,
}

impl<'a> FramesView<'a> {
    /// Preflights every frame and returns exact reusable storage requirements.
    ///
    /// `group_workspace` is temporary planning storage. Its required length is
    /// the largest number of groups referenced by one frame, not the total
    /// number of groups in the sequence.
    pub fn playback_plan(
        self,
        requirements: SurfaceRequirements,
        limits: PayloadLimits,
        group_workspace: &mut [Option<UnitGroup<'a>>],
    ) -> Result<FramesPlaybackPlan<'a>, FrameDecodeError> {
        let memory = self
            .surface()
            .memory_plan(requirements)
            .map_err(FrameDecodeError::Memory)?;
        let group_workspace_len = self
            .frame_map()
            .into_iter()
            .map(|range| usize::try_from(range.end - range.start).expect("u32 fits usize"))
            .max()
            .unwrap_or(0);
        if group_workspace.len() < group_workspace_len {
            return Err(FrameDecodeError::GroupWorkspaceTooSmall {
                needed: group_workspace_len,
                available: group_workspace.len(),
            });
        }

        let mut required = ReplayRequirements::new();
        for frame in 0..self.sequence().frame_count() {
            let mut budget = CoverageBudget::new(limits.max_raster_work());
            let groups = self
                .groups_into(frame, group_workspace, &mut budget)
                .map_err(FrameDecodeError::Frames)?;
            let plan = groups.decode_plan(
                requirements,
                &limits.with_max_raster_work(budget.remaining()),
            )?;
            required.include(&plan);
        }

        Ok(FramesPlaybackPlan {
            frames: self,
            requirements,
            limits,
            memory,
            group_workspace_len,
            workspace: required.workspace(),
            backup: required.backup(),
        })
    }

    /// Binds caller-owned playback storage and validates stable requirements.
    pub fn session<'storage>(
        self,
        requirements: SurfaceRequirements,
        limits: PayloadLimits,
        group_workspace: &'storage mut [Option<UnitGroup<'a>>],
        canvas: &'storage mut [u8],
        unit_workspace: &'storage mut [u8],
        backup: &'storage mut [u8],
    ) -> Result<FrameSession<'a, 'storage>, FrameDecodeError> {
        let memory = self
            .surface()
            .memory_plan(requirements)
            .map_err(FrameDecodeError::Memory)?;
        memory
            .buffer_requirements()
            .validate(canvas)
            .map_err(FrameDecodeError::Canvas)?;
        let group_workspace_len = self
            .frame_map()
            .into_iter()
            .map(|range| usize::try_from(range.end - range.start).expect("u32 fits usize"))
            .max()
            .unwrap_or(0);
        if group_workspace.len() < group_workspace_len {
            return Err(FrameDecodeError::GroupWorkspaceTooSmall {
                needed: group_workspace_len,
                available: group_workspace.len(),
            });
        }
        let needs_backup = (0..self.sequence().frame_count()).any(|frame| {
            self.frame(frame).is_some_and(|presentation| {
                presentation.composition().disposal() == DisposalMode::RestorePrevious
            })
        });
        let backup_requirements = if needs_backup {
            memory.buffer_requirements()
        } else {
            BufferRequirements::new(0, 1).expect("valid empty buffer requirements")
        };
        backup_requirements
            .validate(backup)
            .map_err(FrameDecodeError::Backup)?;
        Ok(FrameSession {
            frames: self,
            requirements,
            limits,
            memory,
            group_workspace_len,
            backup_requirements,
            group_workspace,
            canvas,
            unit_workspace,
            backup,
            current: None,
        })
    }
}

impl<'a> FramesPlaybackPlan<'a> {
    pub const fn frames(self) -> FramesView<'a> {
        self.frames
    }

    pub const fn memory_plan(self) -> SurfaceMemoryPlan {
        self.memory
    }

    pub const fn group_workspace_len(self) -> usize {
        self.group_workspace_len
    }

    pub const fn canvas_requirements(self) -> BufferRequirements {
        self.memory.buffer_requirements()
    }

    pub const fn workspace_requirements(self) -> BufferRequirements {
        self.workspace
    }

    pub const fn backup_requirements(self) -> BufferRequirements {
        self.backup
    }

    /// Binds the preflighted contract to storage retained for the session.
    pub fn bind<'storage>(
        self,
        group_workspace: &'storage mut [Option<UnitGroup<'a>>],
        canvas: &'storage mut [u8],
        unit_workspace: &'storage mut [u8],
        backup: &'storage mut [u8],
    ) -> Result<FrameSession<'a, 'storage>, FrameDecodeError> {
        if group_workspace.len() < self.group_workspace_len {
            return Err(FrameDecodeError::GroupWorkspaceTooSmall {
                needed: self.group_workspace_len,
                available: group_workspace.len(),
            });
        }
        self.canvas_requirements()
            .validate(canvas)
            .map_err(FrameDecodeError::Canvas)?;
        self.workspace
            .validate(unit_workspace)
            .map_err(FrameDecodeError::Workspace)?;
        self.backup
            .validate(backup)
            .map_err(FrameDecodeError::Backup)?;
        Ok(FrameSession {
            frames: self.frames,
            requirements: self.requirements,
            limits: self.limits,
            memory: self.memory,
            group_workspace_len: self.group_workspace_len,
            backup_requirements: self.backup,
            group_workspace,
            canvas,
            unit_workspace,
            backup,
            current: None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReplayRequirements {
    workspace_bytes: usize,
    workspace_alignment: usize,
    backup_bytes: usize,
    backup_alignment: usize,
}

impl ReplayRequirements {
    const fn new() -> Self {
        Self {
            workspace_bytes: 0,
            workspace_alignment: 1,
            backup_bytes: 0,
            backup_alignment: 1,
        }
    }

    fn include(&mut self, plan: &FrameDecodePlan<'_, '_>) {
        let workspace = plan.workspace_requirements();
        self.workspace_bytes = self.workspace_bytes.max(workspace.byte_len());
        self.workspace_alignment = self.workspace_alignment.max(workspace.base_alignment());
        let backup = plan.backup_requirements();
        self.backup_bytes = self.backup_bytes.max(backup.byte_len());
        self.backup_alignment = self.backup_alignment.max(backup.base_alignment());
    }

    fn include_disposal(&mut self, plan: &FrameDisposalPlan<'_, '_>) {
        let backup = plan.backup_requirements();
        self.backup_bytes = self.backup_bytes.max(backup.byte_len());
        self.backup_alignment = self.backup_alignment.max(backup.base_alignment());
    }

    fn workspace(self) -> BufferRequirements {
        Self::buffer(self.workspace_bytes, self.workspace_alignment)
    }

    fn backup(self) -> BufferRequirements {
        Self::buffer(self.backup_bytes, self.backup_alignment)
    }

    fn buffer(byte_len: usize, alignment: usize) -> BufferRequirements {
        BufferRequirements::new(
            u32::try_from(byte_len).expect("planned buffer length originates as u32"),
            u32::try_from(alignment).expect("planned buffer alignment originates as u32"),
        )
        .expect("merged buffer requirements remain valid")
    }
}

impl<'a> FrameSession<'a, '_> {
    pub const fn frames(&self) -> FramesView<'a> {
        self.frames
    }

    /// The frame currently presented on the retained canvas.
    pub const fn current_frame(&self) -> Option<u32> {
        self.current
    }

    pub const fn canvas_requirements(&self) -> BufferRequirements {
        self.memory.buffer_requirements()
    }

    pub const fn group_workspace_len(&self) -> usize {
        self.group_workspace_len
    }

    pub const fn backup_requirements(&self) -> BufferRequirements {
        self.backup_requirements
    }

    /// Forgets playback position without modifying caller storage.
    pub fn reset(&mut self) {
        self.current = None;
    }

    /// Presents `frame`, replaying only the bounded dependency path required.
    ///
    /// The complete path and all buffers are checked before the first canvas
    /// write. A short forward move reuses the retained canvas; other seeks
    /// restart from the closest independent recovery frame.
    pub fn present(&mut self, frame: u32) -> Result<SurfaceView<'_>, FrameDecodeError> {
        if frame >= self.frames.sequence().frame_count() {
            return Err(FrameDecodeError::FrameOutOfBounds(frame));
        }
        if self.current == Some(frame) {
            return Ok(SurfaceView::from_plan(
                self.memory,
                &self.canvas[..self.memory.byte_len() as usize],
                self.frames.color_table(),
            ));
        }
        let (dispose_current, start) = self.replay_start(frame)?;
        let required = self.preflight_run(dispose_current, start, frame)?;
        required
            .workspace()
            .validate(self.unit_workspace)
            .map_err(FrameDecodeError::Workspace)?;
        required
            .backup()
            .validate(self.backup)
            .map_err(FrameDecodeError::Backup)?;

        if dispose_current {
            self.dispose_current();
        }
        for replayed in start..frame {
            self.decode_frame(replayed, true);
        }
        self.decode_frame(frame, false);
        self.current = Some(frame);
        Ok(SurfaceView::from_plan(
            self.memory,
            &self.canvas[..self.memory.byte_len() as usize],
            self.frames.color_table(),
        ))
    }

    fn replay_start(&self, target: u32) -> Result<(bool, u32), FrameDecodeError> {
        if let Some(current) = self.current {
            let forward = target.checked_sub(current);
            if forward.is_some_and(|distance| {
                distance <= u32::from(self.frames.sequence().max_delta_frames()) + 1
            }) {
                return Ok((true, current + 1));
            }
        }
        self.frames
            .recovery_frame(target)
            .map(|frame| (false, frame))
            .ok_or(FrameDecodeError::FrameOutOfBounds(target))
    }

    fn preflight_run(
        &mut self,
        dispose_current: bool,
        start: u32,
        target: u32,
    ) -> Result<ReplayRequirements, FrameDecodeError> {
        // Playback performs the same immutable validation again while applying
        // frames. Reserving half the work bound accounts for both passes.
        let mut budget = CoverageBudget::new(self.limits.max_raster_work() / 2);
        let mut required = ReplayRequirements::new();
        if dispose_current {
            self.preflight_disposal(
                self.current.expect("continuation has current frame"),
                &mut budget,
                &mut required,
            )?;
        }
        for frame in start..=target {
            self.preflight_frame(frame, &mut budget, &mut required)?;
        }
        Ok(required)
    }

    fn preflight_disposal(
        &mut self,
        frame: u32,
        budget: &mut CoverageBudget,
        required: &mut ReplayRequirements,
    ) -> Result<(), FrameDecodeError> {
        let groups = self
            .frames
            .groups_into(frame, self.group_workspace, budget)
            .map_err(FrameDecodeError::Frames)?;
        let plan = groups.disposal_plan(self.requirements)?;
        budget
            .spend_many(plan.work())
            .map_err(EncodedImageError::Coverage)
            .map_err(FrameDecodeError::Image)?;
        required.include_disposal(&plan);
        Ok(())
    }

    fn preflight_frame(
        &mut self,
        frame: u32,
        budget: &mut CoverageBudget,
        required: &mut ReplayRequirements,
    ) -> Result<(), FrameDecodeError> {
        let groups = self
            .frames
            .groups_into(frame, self.group_workspace, budget)
            .map_err(FrameDecodeError::Frames)?;
        let limits = self.limits.with_max_raster_work(budget.remaining());
        let plan = groups.decode_plan(self.requirements, &limits)?;
        budget
            .spend_many(plan.work())
            .map_err(EncodedImageError::Coverage)
            .map_err(FrameDecodeError::Image)?;
        required.include(&plan);
        Ok(())
    }

    fn execution_plan<'groups>(
        frames: FramesView<'a>,
        requirements: SurfaceRequirements,
        limits: PayloadLimits,
        group_workspace: &'groups mut [Option<UnitGroup<'a>>],
        frame: u32,
    ) -> FrameDecodePlan<'a, 'groups> {
        let groups = frames
            .groups_into(frame, group_workspace, &mut CoverageBudget::new(u64::MAX))
            .expect("preflighted immutable frame groups");
        groups
            .decode_plan(requirements, &limits.with_max_raster_work(u64::MAX))
            .expect("preflighted immutable frame decode")
    }

    fn decode_frame(&mut self, frame: u32, dispose: bool) {
        let plan = Self::execution_plan(
            self.frames,
            self.requirements,
            self.limits,
            self.group_workspace,
            frame,
        );
        plan.decode_into(self.canvas, self.unit_workspace, self.backup)
            .expect("preflighted playback buffers");
        if dispose {
            plan.dispose_into(self.canvas, self.backup)
                .expect("preflighted playback disposal");
        }
    }

    fn dispose_current(&mut self) {
        self.dispose_frame(self.current.expect("continuation has current frame"));
    }

    fn dispose_frame(&mut self, frame: u32) {
        let groups = self
            .frames
            .groups_into(
                frame,
                self.group_workspace,
                &mut CoverageBudget::new(u64::MAX),
            )
            .expect("preflighted immutable frame groups");
        let plan = groups
            .disposal_plan(self.requirements)
            .expect("preflighted immutable frame disposal");
        plan.dispose_into(self.canvas, self.backup)
            .expect("preflighted playback buffers");
    }
}

/// Unsupported frame composition, exhausted limits, or invalid caller storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameDecodeError {
    FrameOutOfBounds(u32),
    GroupWorkspaceTooSmall { needed: usize, available: usize },
    Frames(FramesError),
    Image(EncodedImageError),
    UnsupportedSourceOver(crate::image::SampleLayout),
    Memory(SurfacePlanError),
    Canvas(BufferRequirementError),
    Workspace(BufferRequirementError),
    Backup(BufferRequirementError),
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FrameComposition, FrameCompositionOverride, FrameSequence, FramesAsset, FramesView,
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
        let bytes = FramesAsset::new(sequence, surface, &codings, &groups, &[1, 1], &[1, 2, 9])
            .unwrap()
            .with_unit_index(&index)
            .encode()
            .unwrap();
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
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
            let decoded = first
                .decode_into(&mut canvas.0, &mut workspace, &mut [])
                .unwrap();
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
            let decoded = second
                .decode_into(&mut canvas.0, &mut workspace, &mut [])
                .unwrap();
            assert_eq!(decoded.plane(0).unwrap().row(0).unwrap(), Some(&[1, 9][..]));
        }
        assert!(canvas.0[2..].iter().all(|&byte| byte == 0));
    }

    fn composed_payload(disposal: DisposalMode) -> alloc::vec::Vec<u8> {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let codings = [CodingRecord::RAW];
        let groups = [
            UnitGroupRecord::new(0, 0..4).unwrap(),
            UnitGroupRecord::new(0, 4..8)
                .unwrap()
                .with_reference(ReferenceMode::Previous),
        ];
        let composition = [FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::SourceOver, disposal),
        )];
        FramesAsset::new(
            sequence,
            surface,
            &codings,
            &groups,
            &[1, 1],
            &[10, 20, 30, 255, 200, 0, 0, 128],
        )
        .unwrap()
        .with_composition(&composition)
        .unwrap()
        .encode()
        .unwrap()
    }

    #[test]
    fn source_over_restore_and_clear_have_explicit_retained_lifetimes() {
        for disposal in [DisposalMode::RestorePrevious, DisposalMode::Clear] {
            let bytes = composed_payload(disposal);
            let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
            let mut slots = [None];
            let mut canvas = [0xad; 4];
            let mut workspace = [0; 4];
            frames
                .groups_into(0, &mut slots, &mut CoverageBudget::new(100))
                .unwrap()
                .decode_plan(SurfaceRequirements::new(), &PayloadLimits::HOST)
                .unwrap()
                .decode_into(&mut canvas, &mut workspace, &mut [])
                .unwrap();
            assert_eq!(canvas, [10, 20, 30, 255]);

            let plan = frames
                .groups_into(1, &mut slots, &mut CoverageBudget::new(100))
                .unwrap()
                .decode_plan(SurfaceRequirements::new(), &PayloadLimits::HOST)
                .unwrap();
            let expected_backup = if disposal == DisposalMode::RestorePrevious {
                4
            } else {
                0
            };
            assert_eq!(plan.backup_requirements().byte_len(), expected_backup);
            let mut backup = [0; 4];
            if disposal == DisposalMode::RestorePrevious {
                let before = canvas;
                assert!(matches!(
                    plan.decode_into(&mut canvas, &mut workspace, &mut backup[..3]),
                    Err(FrameDecodeError::Backup(BufferRequirementError::TooSmall {
                        needed: 4,
                        available: 3,
                    }))
                ));
                assert_eq!(canvas, before);
            }
            plan.decode_into(&mut canvas, &mut workspace, &mut backup)
                .unwrap();
            assert_eq!(canvas, [105, 10, 15, 255]);
            plan.dispose_into(&mut canvas, &backup).unwrap();
            assert_eq!(
                canvas,
                if disposal == DisposalMode::RestorePrevious {
                    [10, 20, 30, 255]
                } else {
                    [0; 4]
                }
            );
        }
    }

    fn session_payload(index_keyframes: bool) -> alloc::vec::Vec<u8> {
        let sequence = FrameSequence::new(4, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
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
            UnitGroupRecord::new(0, 3..5).unwrap(),
        ];
        let mut index = [0; 4];
        UnitSelectionEncoding::List
            .encode_into(2, &[1], &mut index)
            .unwrap();
        let compositions = [FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::Replace, DisposalMode::Clear),
        )];
        let asset = FramesAsset::new(
            sequence,
            surface,
            &codings,
            &groups,
            &[1, 1, 0, 1],
            &[1, 2, 9, 3, 4],
        )
        .unwrap()
        .with_unit_index(&index)
        .with_composition(&compositions)
        .unwrap();
        if index_keyframes {
            asset.with_keyframes(&[0, 3]).unwrap().encode().unwrap()
        } else {
            asset.encode().unwrap()
        }
    }

    #[test]
    fn session_sequences_disposal_and_bounded_random_access() {
        for index_keyframes in [false, true] {
            let bytes = session_payload(index_keyframes);
            let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
            assert_eq!(frames.recovery_frame(2), Some(0));
            assert_eq!(frames.recovery_frame(3), Some(3));
            assert_eq!(frames.recovery_frame(4), None);

            let mut slots = [None];
            let mut canvas = [0xad; 2];
            let mut workspace = [0; 2];
            let mut session = frames
                .session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST,
                    &mut slots,
                    &mut canvas,
                    &mut workspace,
                    &mut [],
                )
                .unwrap();

            assert_eq!(session.current_frame(), None);
            assert_eq!(session.group_workspace_len(), 1);
            assert_eq!(session.backup_requirements().byte_len(), 0);
            assert_eq!(
                session.present(0).unwrap().plane(0).unwrap().bytes(),
                &[1, 2]
            );
            assert_eq!(session.current_frame(), Some(0));
            assert_eq!(
                session.present(1).unwrap().plane(0).unwrap().bytes(),
                &[1, 9]
            );
            assert_eq!(
                session.present(2).unwrap().plane(0).unwrap().bytes(),
                &[1, 0]
            );
            assert_eq!(
                session.present(3).unwrap().plane(0).unwrap().bytes(),
                &[3, 4]
            );

            assert_eq!(
                session.present(1).unwrap().plane(0).unwrap().bytes(),
                &[1, 9]
            );
            assert_eq!(
                session.present(1).unwrap().plane(0).unwrap().bytes(),
                &[1, 9]
            );
            session.reset();
            assert_eq!(session.current_frame(), None);
            assert_eq!(
                session.present(2).unwrap().plane(0).unwrap().bytes(),
                &[1, 0]
            );
        }
    }

    #[test]
    fn playback_plan_reports_exact_reusable_storage_and_binds_it() {
        let bytes = session_payload(false);
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let mut planning_slots = [None];
        let plan = frames
            .playback_plan(
                SurfaceRequirements::new().with_base_alignment(2),
                PayloadLimits::HOST,
                &mut planning_slots,
            )
            .unwrap();

        assert_eq!(plan.group_workspace_len(), 1);
        assert_eq!(plan.canvas_requirements().byte_len(), 2);
        assert_eq!(plan.canvas_requirements().base_alignment(), 2);
        assert_eq!(plan.workspace_requirements().byte_len(), 2);
        assert_eq!(plan.backup_requirements().byte_len(), 0);

        let mut slots = [None];
        let mut canvas = [0xad; 2];
        let mut workspace = [0; 2];
        let mut session = plan
            .bind(&mut slots, &mut canvas, &mut workspace, &mut [])
            .unwrap();
        assert_eq!(
            session.present(1).unwrap().plane(0).unwrap().bytes(),
            &[1, 9]
        );
    }

    #[test]
    fn playback_plan_includes_restore_previous_backup() {
        let bytes = composed_payload(DisposalMode::RestorePrevious);
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let mut planning_slots = [None];
        let plan = frames
            .playback_plan(
                SurfaceRequirements::new().with_base_alignment(4),
                PayloadLimits::HOST,
                &mut planning_slots,
            )
            .unwrap();

        assert_eq!(plan.canvas_requirements().byte_len(), 4);
        assert_eq!(plan.backup_requirements(), plan.canvas_requirements());
        let mut slots = [None];
        let mut canvas = [0xad; 4];
        let mut workspace = [0; 4];
        let mut short_backup = [0; 3];
        assert!(matches!(
            plan.bind(&mut slots, &mut canvas, &mut workspace, &mut short_backup,),
            Err(FrameDecodeError::Backup(BufferRequirementError::TooSmall {
                needed: 4,
                available: 3,
            }))
        ));
        assert_eq!(canvas, [0xad; 4]);
    }

    #[test]
    fn session_rejects_storage_before_mutating_canvas() {
        let bytes = session_payload(false);
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let mut no_slots = [];
        let mut canvas = [0xad; 2];
        let mut workspace = [0; 2];
        assert!(matches!(
            frames.session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut no_slots,
                &mut canvas,
                &mut workspace,
                &mut [],
            ),
            Err(FrameDecodeError::GroupWorkspaceTooSmall {
                needed: 1,
                available: 0,
            })
        ));
        assert_eq!(canvas, [0xad; 2]);

        let mut slots = [None];
        let mut canvas = [0xad; 2];
        let mut short_workspace = [0; 1];
        {
            let mut session = frames
                .session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST,
                    &mut slots,
                    &mut canvas,
                    &mut short_workspace,
                    &mut [],
                )
                .unwrap();
            assert!(matches!(
                session.present(0),
                Err(FrameDecodeError::Workspace(
                    BufferRequirementError::TooSmall {
                        needed: 2,
                        available: 1,
                    }
                ))
            ));
            assert_eq!(session.current_frame(), None);
        }
        assert_eq!(canvas, [0xad; 2]);

        let mut slots = [None];
        let mut canvas = [0xad; 2];
        let mut workspace = [0; 2];
        {
            let mut session = frames
                .session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST.with_max_raster_work(0),
                    &mut slots,
                    &mut canvas,
                    &mut workspace,
                    &mut [],
                )
                .unwrap();
            assert!(session.present(0).is_err());
            assert_eq!(session.current_frame(), None);
        }
        assert_eq!(canvas, [0xad; 2]);

        let bytes = composed_payload(DisposalMode::RestorePrevious);
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let mut slots = [None];
        let mut canvas = [0xad; 4];
        let mut workspace = [0; 4];
        let mut short_backup = [0; 3];
        assert!(matches!(
            frames.session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut slots,
                &mut canvas,
                &mut workspace,
                &mut short_backup,
            ),
            Err(FrameDecodeError::Backup(BufferRequirementError::TooSmall {
                needed: 4,
                available: 3,
            }))
        ));
        assert_eq!(canvas, [0xad; 4]);
    }

    #[test]
    fn session_reconstructs_chained_frame_delta_units() {
        let codec = crate::coding::FrameDelta::new();
        let root = [250, 2];
        let next = [5, 2];
        let last = [5, 9];
        let mut first_delta = [0; 4];
        let first_len = codec.encode_into(&root, &next, &mut first_delta).unwrap();
        let mut second_delta = [0; 4];
        let second_len = codec.encode_into(&next, &last, &mut second_delta).unwrap();
        let mut data = alloc::vec::Vec::from(root);
        data.extend_from_slice(&first_delta[..first_len]);
        data.extend_from_slice(&second_delta[..second_len]);
        let first_end = root.len() as u32 + first_len as u32;
        let groups = [
            UnitGroupRecord::new(0, 0..root.len() as u32).unwrap(),
            UnitGroupRecord::new(1, root.len() as u32..first_end)
                .unwrap()
                .with_reference(ReferenceMode::Previous),
            UnitGroupRecord::new(1, first_end..data.len() as u32)
                .unwrap()
                .with_reference(ReferenceMode::Previous),
        ];
        let sequence = FrameSequence::new(3, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let codings = [CodingRecord::RAW, codec.record()];
        let bytes = FramesAsset::new(sequence, surface, &codings, &groups, &[1, 1, 1], &data)
            .unwrap()
            .encode()
            .unwrap();
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        let data_offset = frames
            .media()
            .section(crate::media::MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset() as usize;
        let mut slots = [None];
        let mut canvas = [0xad; 2];
        let mut workspace = [0; 2];
        {
            let mut session = frames
                .session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST,
                    &mut slots,
                    &mut canvas,
                    &mut workspace,
                    &mut [],
                )
                .unwrap();
            assert_eq!(session.present(0).unwrap().plane(0).unwrap().bytes(), &root);
            assert_eq!(session.present(1).unwrap().plane(0).unwrap().bytes(), &next);
            assert_eq!(session.present(2).unwrap().plane(0).unwrap().bytes(), &last);
            session.reset();
            assert_eq!(session.present(2).unwrap().plane(0).unwrap().bytes(), &last);
        }

        let mut corrupted = bytes.clone();
        corrupted[data_offset + data.len() - 1] ^= 1;
        let frames = FramesView::open(&corrupted, &PayloadLimits::HOST).unwrap();
        canvas = [0xad; 2];
        {
            let mut session = frames
                .session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST,
                    &mut slots,
                    &mut canvas,
                    &mut workspace,
                    &mut [],
                )
                .unwrap();
            assert!(session.present(2).is_err());
            assert_eq!(session.current_frame(), None);
        }
        assert_eq!(canvas, [0xad; 2]);
    }
}
