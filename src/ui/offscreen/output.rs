use super::BufferKey;
use crate::core::cache::HasSize;
use crate::ecs::Entity;
use crate::render::RenderError;
use crate::render::budget::MemoryBudget;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::Rect;
use crate::ui::outputs::{
    HistoryOutput, Output, OutputState, Outputs,
    private::{HistoryKind, OutputKind},
    workspace::PaintWorkspace,
};
use core::cell::Ref;
use hashbrown::HashMap;

#[cfg(feature = "perf")]
#[derive(Clone, Copy, Default)]
pub(crate) struct AdmissionTrace {
    pub protected: usize,
    pub cpu_consumer: bool,
}

#[cfg(feature = "perf")]
#[derive(Clone, Copy)]
struct DecisionTrace {
    key: BufferKey,
    slot: u8,
    retained: bool,
    admission: AdmissionTrace,
}

#[cfg(feature = "perf")]
impl DecisionTrace {
    fn new(outputs: &Outputs<WidgetTexture>, key: BufferKey) -> Self {
        Self {
            key,
            slot: u8::MAX,
            retained: outputs
                .state(key.entity)
                .is_some_and(|state| state.retained),
            admission: AdmissionTrace::default(),
        }
    }

    fn record(
        self,
        outcome: crate::core::perf::DecisionOutcome,
        storage: crate::core::perf::DecisionStorage,
        requested: Option<usize>,
        available: usize,
    ) {
        use crate::core::perf::{ResourceDecision, ResourceKind, decision};
        decision(ResourceDecision {
            kind: ResourceKind::OutputTarget,
            resource: (u64::from(self.key.entity.id) << 32) | u64::from(self.key.entity.generation),
            requested_bytes: requested.unwrap_or(0) as u64,
            requested_known: requested.is_some(),
            resource_known: true,
            available_bytes: available as u64,
            scale: self.key.scale,
            width: self.key.w,
            height: self.key.h,
            outcome,
            storage,
            slot: self.slot,
            retained: self.retained,
        });
    }

    fn native_budget(self, requested: Option<usize>, available: usize) {
        self.record(
            if requested.is_some_and(|bytes| {
                bytes > available && bytes <= available.saturating_add(self.admission.protected)
            }) {
                crate::core::perf::DecisionOutcome::WorkingSetProtected
            } else {
                crate::core::perf::DecisionOutcome::Budget
            },
            crate::core::perf::DecisionStorage::Native,
            requested,
            available,
        );
    }
}

macro_rules! trace_output {
    ($trace:expr, $outcome:ident, $storage:ident, $requested:expr, $available:expr) => {
        #[cfg(feature = "perf")]
        $trace.record(
            crate::core::perf::DecisionOutcome::$outcome,
            crate::core::perf::DecisionStorage::$storage,
            $requested,
            $available,
        );
    };
}

/// The completed pixel output of a widget subtree.
pub struct WidgetTexture;

/// A shared borrow of completed widget pixels and their published logical geometry.
pub struct WidgetTextureRead<'a> {
    texture: Ref<'a, Texture<'static>>,
    layout_rect: Rect,
    local_bounds: Rect,
}

/// An independently owned completed widget output and its published logical geometry.
///
/// Pixel storage retains its original output-budget charge until the texture is released. Native access follows the creating backend's owner and thread rules.
pub struct WidgetTextureOwned {
    texture: Texture<'static>,
    layout_rect: Rect,
    local_bounds: Rect,
}

impl WidgetTextureOwned {
    pub fn texture(&self) -> &Texture<'static> {
        &self.texture
    }

    /// Source rectangle in root layout coordinates, excluding scrolling and external transforms.
    pub fn layout_rect(&self) -> Rect {
        self.layout_rect
    }

    /// Prepared logical pixel footprint relative to the source layout origin.
    pub fn local_bounds(&self) -> Rect {
        self.local_bounds
    }

    pub fn into_texture(self) -> Texture<'static> {
        self.texture
    }
}

impl WidgetTextureRead<'_> {
    pub fn texture(&self) -> &Texture<'static> {
        &self.texture
    }

    /// Source rectangle in root layout coordinates, excluding scrolling and external transforms.
    pub fn layout_rect(&self) -> Rect {
        self.layout_rect
    }

    /// Prepared logical pixel footprint relative to the source layout origin.
    pub fn local_bounds(&self) -> Rect {
        self.local_bounds
    }
}

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct WidgetTextureRecord {
    entity: Entity,
    slot: usize,
    key: BufferKey,
}

fn writable_slot(state: OutputState<WidgetTextureRecord>) -> usize {
    if state.retained {
        state
            .previous
            .map_or(0, |previous| 1 - previous.record.slot)
    } else {
        state.current.map_or(0, |current| current.record.slot)
    }
}

struct TextureSlots {
    planes: [Option<OutputPlane>; 2],
    shape: (u16, u16, ColorFormat),
    writing: bool,
    discard: bool,
}

struct OutputPlane {
    texture: Texture<'static>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureBaseline {
    AllocateCpu,
    ReuseCpu,
    ReuseNative,
}

fn local_mapping_changed(state: OutputState<WidgetTextureRecord>, key: BufferKey) -> bool {
    state.current.or(state.previous).is_some_and(|published| {
        let previous = published.record.key;
        previous.layout_rect.w != key.layout_rect.w
            || previous.layout_rect.h != key.layout_rect.h
            || previous.local_bounds != key.local_bounds
            || previous.scale != key.scale
    })
}

#[doc(hidden)]
pub struct WidgetTextureStorage {
    textures: HashMap<Entity, TextureSlots>,
    budget: MemoryBudget,
    pub(crate) plan: Option<crate::ui::render_system::output_plan::TexturePlan>,
    deferred_workspace_trim: bool,
}

impl WidgetTextureStorage {
    fn independent_plane(&self, record: WidgetTextureRecord) -> Option<&OutputPlane> {
        self.plan.as_ref()?;
        let key = record.key;
        let slots = self.textures.get(&record.entity)?;
        if key.entity != record.entity
            || slots.writing
            || slots.discard
            || slots.shape != (key.w, key.h, key.format)
        {
            return None;
        }
        let plane = slots.planes.get(record.slot)?.as_ref()?;
        (plane.texture.width == key.w
            && plane.texture.height == key.h
            && plane.texture.format == key.format)
            .then_some(plane)
    }

    fn admit_native_plane(
        &mut self,
        entity: Entity,
        slot: usize,
        native: (&dyn crate::render::Renderer, &crate::render::TargetPlan),
        workspace: &mut PaintWorkspace,
        paint_bytes: usize,
        #[cfg(feature = "perf")] trace: DecisionTrace,
    ) -> Result<bool, RenderError> {
        let (renderer, plan) = native;
        let plane = self
            .textures
            .get(&entity)
            .and_then(|slots| slots.planes[slot].as_ref());
        let reusable = match plane {
            Some(plane) => {
                let result = plane.texture.can_reuse_native(renderer, plan);
                #[cfg(feature = "perf")]
                if result.is_err() {
                    trace_output!(trace, BackendError, Native, None, self.budget.available());
                }
                result?
            }
            None => false,
        };
        let reclaim = if reusable {
            0
        } else {
            plane.map_or(0, |plane| plane.texture.cache_size())
        };
        let storage_bytes = if reusable {
            Some(0)
        } else {
            Texture::native_cpu_bytes(plan.descriptor.layout)
                .and_then(|bytes| bytes.checked_add(plan.storage_bytes))
                .and_then(|bytes| bytes.checked_add(plan.readback_bytes))
        };
        let required = storage_bytes.and_then(|bytes| bytes.checked_add(paint_bytes));
        let growth = plan.workspace_bytes.saturating_sub(workspace.capacity());
        let total = required.and_then(|bytes| bytes.checked_add(growth));
        let available = self.budget.available().saturating_add(reclaim);
        let Some(total) = total else {
            trace_output!(trace, ArithmeticOverflow, Native, None, available);
            return Ok(false);
        };
        if total > available {
            #[cfg(feature = "perf")]
            trace.native_budget(Some(total), available);
            return Ok(false);
        }
        if !reusable && let Some(slots) = self.textures.get_mut(&entity) {
            slots.planes[slot] = None;
        }
        if !self.reserve_workspace(workspace, plan.workspace_bytes.max(workspace.capacity())) {
            #[cfg(feature = "perf")]
            trace.record(
                if self.plan.is_some() || self.deferred_workspace_trim {
                    crate::core::perf::DecisionOutcome::WorkspaceUnavailable
                } else {
                    crate::core::perf::DecisionOutcome::WorkspaceAllocation
                },
                crate::core::perf::DecisionStorage::Native,
                Some(plan.workspace_bytes.max(workspace.capacity())),
                self.budget.available().saturating_add(workspace.capacity()),
            );
            return Ok(false);
        }
        let admitted = required.is_some_and(|bytes| bytes <= self.budget.available());
        #[cfg(feature = "perf")]
        if !admitted {
            trace.native_budget(required, self.budget.available());
        }
        Ok(admitted)
    }

    fn reserve_plane(
        &mut self,
        entity: Entity,
        slot: usize,
        layout: crate::render::PlaneLayout,
        mut native: Option<(&mut dyn crate::render::Renderer, crate::render::TargetPlan)>,
        mut workspace: Option<(&mut PaintWorkspace, usize, bool)>,
        #[cfg(feature = "perf")] trace: DecisionTrace,
    ) -> Result<bool, RenderError> {
        if let (Some((renderer, plan)), Some((workspace, paint_bytes, _))) =
            (&native, &mut workspace)
            && !self.admit_native_plane(
                entity,
                slot,
                (&**renderer, plan),
                workspace,
                *paint_bytes,
                #[cfg(feature = "perf")]
                trace,
            )?
        {
            native = None;
        }
        if native.is_none()
            && let Some((workspace, paint_bytes, false)) = workspace
        {
            let plane = self
                .textures
                .get(&entity)
                .and_then(|slots| slots.planes[slot].as_ref());
            let reusable = plane.is_some_and(|plane| !plane.texture.is_native());
            let reclaim = if reusable {
                0
            } else {
                plane.map_or(0, |plane| plane.texture.cache_size())
            };
            let storage_bytes = if reusable { 0 } else { layout.required_bytes() };
            if storage_bytes
                .checked_add(paint_bytes)
                .is_none_or(|required| required > self.budget.available().saturating_add(reclaim))
            {
                workspace.release();
            }
        }
        if let Some(plane) = self
            .textures
            .get(&entity)
            .and_then(|slots| slots.planes[slot].as_ref())
        {
            let reusable = match &native {
                Some((renderer, plan)) => {
                    let result = plane.texture.can_reuse_native(&**renderer, plan);
                    #[cfg(feature = "perf")]
                    if result.is_err() {
                        trace_output!(trace, BackendError, Native, None, self.budget.available());
                    }
                    result?
                }
                None => !plane.texture.is_native(),
            };
            if reusable {
                #[cfg(feature = "perf")]
                trace.record(
                    crate::core::perf::DecisionOutcome::Reused,
                    if native.is_some() {
                        crate::core::perf::DecisionStorage::Native
                    } else {
                        crate::core::perf::DecisionStorage::Cpu
                    },
                    Some(0),
                    self.budget.available(),
                );
                return Ok(true);
            }
            self.textures
                .get_mut(&entity)
                .expect("existing write slot")
                .planes[slot] = None;
        }
        #[cfg(feature = "perf")]
        let selected = if native.is_some() {
            crate::core::perf::DecisionStorage::Native
        } else {
            crate::core::perf::DecisionStorage::Cpu
        };
        let plane = if let Some((renderer, plan)) = native {
            let texture = Texture::native(renderer, plan, &self.budget);
            #[cfg(feature = "perf")]
            if texture.is_err() {
                trace_output!(trace, BackendError, Native, None, self.budget.available());
            }
            OutputPlane { texture: texture? }
        } else {
            let Some(reservation) = self.budget.reserve(layout.required_bytes()) else {
                trace_output!(
                    trace,
                    Budget,
                    Cpu,
                    Some(layout.required_bytes()),
                    self.budget.available()
                );
                return Ok(false);
            };
            let texture = Texture::owned(layout.width(), layout.height(), layout.format());
            #[cfg(feature = "perf")]
            if texture.is_err() {
                trace_output!(
                    trace,
                    AllocationFailed,
                    Cpu,
                    Some(layout.required_bytes()),
                    self.budget.available()
                );
            }
            let texture = texture?;
            let Ok(allocation) = reservation.commit(texture.cache_size()) else {
                trace_output!(
                    trace,
                    Budget,
                    Cpu,
                    Some(texture.cache_size()),
                    self.budget.available()
                );
                return Ok(false);
            };
            OutputPlane {
                texture: texture.with_allocation(allocation),
            }
        };
        if !self.textures.contains_key(&entity) && self.textures.try_reserve(1).is_err() {
            #[cfg(feature = "perf")]
            trace.record(
                crate::core::perf::DecisionOutcome::AllocationFailed,
                selected,
                None,
                self.budget.available(),
            );
            return Ok(false);
        }
        #[cfg(feature = "perf")]
        trace.record(
            crate::core::perf::DecisionOutcome::Allocated,
            selected,
            Some(plane.texture.cache_size()),
            self.budget.available(),
        );
        let slots = self.textures.entry(entity).or_insert_with(|| TextureSlots {
            planes: [None, None],
            shape: (layout.width(), layout.height(), layout.format()),
            writing: false,
            discard: false,
        });
        slots.planes[slot] = Some(plane);
        Ok(true)
    }

    fn reserve_workspace(&self, workspace: &mut PaintWorkspace, required_bytes: usize) -> bool {
        if self.plan.is_some() {
            return false;
        }
        let previous_capacity = workspace.capacity();
        if required_bytes == 0 {
            workspace.release();
            return true;
        }
        if self.deferred_workspace_trim {
            return false;
        }
        if required_bytes <= previous_capacity {
            return true;
        }
        let available = self.budget.available().saturating_add(previous_capacity);
        if required_bytes > available {
            return false;
        }
        workspace.release();
        workspace.allocate(required_bytes, &self.budget)
    }
}

impl OutputKind for WidgetTexture {
    fn install(world: &mut crate::ecs::World) {
        if world.resource::<Outputs<Self>>().is_none() {
            world.insert_resource(Outputs::<Self>::default());
        }
    }

    fn reconcile(world: &crate::ecs::World) {
        super::reconcile_texture_outputs(world);
    }
}
impl HistoryKind for WidgetTexture {}

impl Output for WidgetTexture {
    type Storage = WidgetTextureStorage;
    type Record = WidgetTextureRecord;
    type Read<'a> = WidgetTextureRead<'a>;

    fn read<'a>(storage: Ref<'a, Self::Storage>, record: Self::Record) -> Option<Self::Read<'a>> {
        Ref::filter_map(storage, |storage| {
            Some(
                &storage.textures.get(&record.entity)?.planes[record.slot]
                    .as_ref()?
                    .texture,
            )
        })
        .ok()
        .map(|texture| WidgetTextureRead {
            texture,
            layout_rect: record.key.layout_rect,
            local_bounds: record.key.local_bounds,
        })
    }
}

impl HistoryOutput for WidgetTexture {}

impl Outputs<WidgetTexture> {
    pub(crate) fn take_texture(
        &self,
        entity: Entity,
    ) -> Result<Option<WidgetTextureOwned>, RenderError> {
        let Some(current) = self.independent_current(entity) else {
            return Ok(None);
        };
        if current.record.entity != entity
            || current.input_version != u64::from(current.record.key.generation)
        {
            return Ok(None);
        }
        {
            let Some(storage) = self.try_storage() else {
                return Ok(None);
            };
            let Some(plane) = storage.independent_plane(current.record) else {
                return Ok(None);
            };
            // Validate without preparing pixels or holding exclusive output borrows.
            plane.texture.pixels()?;
        }
        let detached = self.take_independent(entity, current.version, |storage, record| {
            storage.independent_plane(record)?;
            let mut slots = storage.textures.remove(&entity)?;
            let plane = slots.planes[record.slot]
                .take()
                .expect("validated output plane");
            Some((
                WidgetTextureOwned {
                    texture: plane.texture,
                    layout_rect: record.key.layout_rect,
                    local_bounds: record.key.local_bounds,
                },
                slots,
            ))
        });
        Ok(detached.map(|(output, unused)| {
            drop(unused);
            output
        }))
    }

    pub(crate) fn texture_budget_available(&self) -> usize {
        self.storage().budget.available()
    }
    pub(crate) fn reserve_texture_budget(
        &self,
        bytes: usize,
    ) -> Option<crate::render::budget::Reservation> {
        self.storage().budget.reserve(bytes)
    }

    pub(crate) fn texture_baseline(
        &self,
        key: BufferKey,
        renderer: &dyn crate::render::Renderer,
        plan: Option<&crate::render::TargetPlan>,
    ) -> Result<Option<TextureBaseline>, RenderError> {
        if let Some(plan) = plan
            && (plan.descriptor.layout.width() != key.w
                || plan.descriptor.layout.height() != key.h
                || plan.descriptor.layout.format() != key.format)
        {
            return Err(RenderError::InvalidTexture);
        }
        let state = self.state(key.entity);
        if state.is_some_and(|state| state.retained && state.published_in_batch) {
            return Ok(None);
        }
        let storage = self.storage();
        let Some(slots) = storage.textures.get(&key.entity) else {
            return Ok(Some(TextureBaseline::AllocateCpu));
        };
        if slots.writing {
            return Ok(None);
        }
        if slots.shape != (key.w, key.h, key.format)
            || state.is_none_or(|state| local_mapping_changed(state, key))
        {
            return Ok(Some(TextureBaseline::AllocateCpu));
        }
        let Some(plane) = slots.planes[writable_slot(state.unwrap())].as_ref() else {
            return Ok(Some(TextureBaseline::AllocateCpu));
        };
        if !plane.texture.is_native() {
            return Ok(Some(TextureBaseline::ReuseCpu));
        }
        if let Some(plan) = plan
            && plane.texture.can_reuse_native(renderer, plan)?
        {
            return Ok(Some(TextureBaseline::ReuseNative));
        }
        Ok(Some(TextureBaseline::AllocateCpu))
    }

    pub(crate) fn write_texture_is_native(&self, entity: Entity) -> bool {
        let Some(state) = self.state(entity) else {
            return false;
        };
        self.storage()
            .textures
            .get(&entity)
            .and_then(|slots| slots.planes[writable_slot(state)].as_ref())
            .is_some_and(|plane| plane.texture.is_native())
    }
    /// Reserve a byte budget for widget output pixels. Zero disables offscreen storage.
    pub fn with_budget(budget: usize) -> Self {
        Self::new(WidgetTextureStorage {
            textures: HashMap::new(),
            budget: MemoryBudget::new(budget),
            plan: Some(Default::default()),
            deferred_workspace_trim: false,
        })
    }

    pub(crate) fn take_texture_plan(
        &self,
    ) -> Option<crate::ui::render_system::output_plan::TexturePlan> {
        self.storage_mut().plan.take()
    }

    pub(crate) fn put_texture_plan(
        &self,
        mut plan: crate::ui::render_system::output_plan::TexturePlan,
    ) {
        let mut storage = self.storage_mut();
        assert!(storage.plan.is_none(), "texture plan is already stored");
        if storage.deferred_workspace_trim {
            plan.trim_memory();
            storage.deferred_workspace_trim = false;
        }
        storage.plan = Some(plan);
    }

    pub(crate) fn reserve_workspace(
        &self,
        workspace: &mut PaintWorkspace,
        required_bytes: usize,
    ) -> bool {
        self.storage().reserve_workspace(workspace, required_bytes)
    }

    pub(crate) fn texture_matches(&self, key: BufferKey) -> Result<bool, RenderError> {
        let Some(current) = self
            .state(key.entity)
            .filter(|state| state.current_available)
            .and_then(|state| state.current)
            .filter(|current| {
                current.record.key == key && current.input_version == key.generation as u64
            })
        else {
            return Ok(false);
        };
        let storage = self.storage();
        let Some(plane) = storage
            .textures
            .get(&key.entity)
            .and_then(|slots| slots.planes[current.record.slot].as_ref())
        else {
            return Ok(false);
        };
        plane.texture.pixels()?;
        Ok(true)
    }

    #[cfg(feature = "perf")]
    pub(crate) fn trace_texture_reuse(&self, key: BufferKey) {
        let Some(state) = self.state(key.entity) else {
            return;
        };
        let Some(current) = state.current else { return };
        let storage = self.storage();
        let Some(plane) = storage
            .textures
            .get(&key.entity)
            .and_then(|slots| slots.planes[current.record.slot].as_ref())
        else {
            return;
        };
        DecisionTrace {
            key,
            slot: current.record.slot as u8,
            retained: state.retained,
            admission: AdmissionTrace::default(),
        }
        .record(
            crate::core::perf::DecisionOutcome::Unchanged,
            if plane.texture.is_native() {
                crate::core::perf::DecisionStorage::Native
            } else {
                crate::core::perf::DecisionStorage::Cpu
            },
            Some(0),
            storage.budget.available(),
        );
    }

    pub(crate) fn reserve_texture(
        &self,
        key: BufferKey,
        native: Option<(&mut dyn crate::render::Renderer, crate::render::TargetPlan)>,
    ) -> Result<bool, RenderError> {
        if self.texture_matches(key)? {
            return Ok(true);
        }
        self.reserve_texture_slot(
            key,
            native,
            None,
            #[cfg(feature = "perf")]
            AdmissionTrace::default(),
        )
    }

    pub(crate) fn plan_render_texture(
        &self,
        key: BufferKey,
        renderer: &dyn crate::render::Renderer,
    ) -> Result<Option<crate::render::TargetPlan>, RenderError> {
        let layout = crate::render::PlaneLayout::packed(key.w, key.h, key.format)
            .map_err(|_| RenderError::InvalidTexture)?;
        let plan = renderer.plan_target(layout);
        #[cfg(feature = "perf")]
        if plan.is_err() {
            trace_output!(
                DecisionTrace::new(self, key),
                BackendError,
                Unselected,
                None,
                self.storage().budget.available()
            );
        }
        plan
    }

    pub(crate) fn reserve_render_texture(
        &self,
        key: BufferKey,
        native: Option<(&mut dyn crate::render::Renderer, crate::render::TargetPlan)>,
        workspace: &mut PaintWorkspace,
        paint_bytes: usize,
        binding_required: bool,
        #[cfg(feature = "perf")] admission: AdmissionTrace,
    ) -> Result<bool, RenderError> {
        self.reserve_texture_slot(
            key,
            native,
            Some((workspace, paint_bytes, binding_required)),
            #[cfg(feature = "perf")]
            admission,
        )
    }

    pub(crate) fn write_workspace_bytes(&self, entity: Entity) -> usize {
        let Some(state) = self.state(entity) else {
            return 0;
        };
        let slot = writable_slot(state);
        self.storage()
            .textures
            .get(&entity)
            .and_then(|slots| slots.planes[slot].as_ref())
            .map_or(0, |plane| plane.texture.target_workspace_bytes())
    }

    fn reserve_texture_slot(
        &self,
        key: BufferKey,
        native: Option<(&mut dyn crate::render::Renderer, crate::render::TargetPlan)>,
        workspace: Option<(&mut PaintWorkspace, usize, bool)>,
        #[cfg(feature = "perf")] admission: AdmissionTrace,
    ) -> Result<bool, RenderError> {
        #[cfg(feature = "perf")]
        let mut trace = DecisionTrace::new(self, key);
        #[cfg(feature = "perf")]
        {
            trace.admission = admission;
        }
        if let Some((_, plan)) = &native
            && (plan.descriptor.layout.width() != key.w
                || plan.descriptor.layout.height() != key.h
                || plan.descriptor.layout.format() != key.format)
        {
            trace_output!(
                trace,
                InvalidPlan,
                Native,
                None,
                self.storage().budget.available()
            );
            return Err(RenderError::InvalidTexture);
        }
        if key.w == 0 || key.h == 0 {
            trace_output!(
                trace,
                ZeroSize,
                Unselected,
                Some(0),
                self.storage().budget.available()
            );
            self.reset_texture(key.entity);
            return Ok(false);
        }
        if self
            .state(key.entity)
            .is_some_and(|state| state.retained && state.published_in_batch)
        {
            trace_output!(
                trace,
                HistoryFrozen,
                Unselected,
                None,
                self.storage().budget.available()
            );
            return Ok(false);
        }
        let shape = (key.w, key.h, key.format);
        let layout = crate::render::PlaneLayout::packed(key.w, key.h, key.format)
            .map_err(|_| RenderError::InvalidTexture)?;
        if self.state(key.entity).is_none() && self.reserve(1).is_err() {
            trace_output!(
                trace,
                AllocationFailed,
                Unselected,
                None,
                self.storage().budget.available()
            );
            return Ok(false);
        }
        let mut storage = self.storage_mut();
        if storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.writing)
        {
            trace_output!(trace, Busy, Unselected, None, storage.budget.available());
            return Ok(false);
        }
        let local_mapping_changed = self
            .state(key.entity)
            .is_some_and(|state| local_mapping_changed(state, key));
        let pixel_shape_changed = storage
            .textures
            .get(&key.entity)
            .is_some_and(|slots| slots.shape != shape);
        if pixel_shape_changed {
            storage.textures.remove(&key.entity);
        }
        if pixel_shape_changed || local_mapping_changed {
            self.reset(key.entity);
        }
        // Geometry changes invalidate current even when generation is unchanged.
        self.invalidate(key.entity);
        if self.prepare(key.entity, key.generation as u64).is_err() {
            return Ok(false);
        }
        let Some(state) = self.state(key.entity) else {
            return Ok(false);
        };
        let slot = writable_slot(state);
        #[cfg(feature = "perf")]
        {
            trace.slot = slot as u8;
            if native.is_none() && workspace.is_some() {
                trace.record(
                    if trace.admission.cpu_consumer {
                        crate::core::perf::DecisionOutcome::CpuConsumerPreferred
                    } else {
                        crate::core::perf::DecisionOutcome::NotOffered
                    },
                    crate::core::perf::DecisionStorage::Native,
                    None,
                    storage.budget.available(),
                );
            }
        }
        storage.reserve_plane(
            key.entity,
            slot,
            layout,
            native,
            workspace,
            #[cfg(feature = "perf")]
            trace,
        )
    }

    pub(crate) fn prepare_texture(&self, key: BufferKey) -> Option<TextureWrite<'_>> {
        let state = self.state(key.entity)?;
        if key.w == 0
            || key.h == 0
            || state.input_version != key.generation as u64
            || (state.retained && state.published_in_batch)
        {
            return None;
        }
        let slot = writable_slot(state);
        let mut storage = self.storage_mut();
        let slots = storage.textures.get_mut(&key.entity)?;
        if slots.writing || slots.discard || slots.shape != (key.w, key.h, key.format) {
            return None;
        }
        let texture = slots.planes[slot].take()?;
        slots.writing = true;
        drop(storage);
        self.invalidate(key.entity);
        let mut write = TextureWrite {
            outputs: self,
            record: WidgetTextureRecord {
                entity: key.entity,
                slot,
                key,
            },
            texture: Some(texture),
        };
        if !write.texture_mut().is_native() {
            write.texture_mut().pixels_mut().ok()?.bytes_mut().fill(0);
        } else {
            write.texture_mut().invalidate_native();
        }
        Some(write)
    }

    #[cfg(test)]
    pub(crate) fn prepare_test_texture(&self, key: BufferKey) -> Option<TextureWrite<'_>> {
        if self
            .state(key.entity)
            .is_some_and(|state| state.retained && state.published_in_batch)
        {
            return None;
        }
        self.invalidate(key.entity);
        if !self.reserve_texture(key, None).ok()? {
            return None;
        }
        self.prepare_texture(key)
    }

    fn reset_texture(&self, entity: Entity) {
        self.reset(entity);
        let mut storage = self.storage_mut();
        let Some(slots) = storage.textures.get_mut(&entity) else {
            return;
        };
        for plane in &mut slots.planes {
            *plane = None;
        }
        slots.discard = true;
        if !slots.writing {
            storage.textures.remove(&entity);
        }
    }

    pub(crate) fn trim_memory(&self) {
        let Some(mut storage) = self.try_storage_mut() else {
            return;
        };
        storage.textures.retain(|entity, slots| {
            self.reset(*entity);
            for plane in &mut slots.planes {
                *plane = None;
            }
            slots.discard = true;
            slots.writing
        });
        if let Some(plan) = storage.plan.as_mut() {
            plan.trim_memory();
        } else {
            storage.deferred_workspace_trim = true;
        }
    }

    pub(crate) fn reconcile_textures(
        &self,
        requested: impl Fn(Entity) -> bool,
        visible: impl Fn(Entity) -> bool,
    ) {
        let mut storage = self.storage_mut();
        storage.textures.retain(|entity, slots| {
            let demanded = self.is_requested(*entity) || requested(*entity);
            if demanded && visible(*entity) {
                if let Some(state) = self.state(*entity)
                    && !state.retained
                    && !slots.writing
                {
                    let keep = state.current.map(|current| current.record.slot);
                    for (index, plane) in slots.planes.iter_mut().enumerate() {
                        if keep != Some(index) {
                            *plane = None;
                        }
                    }
                }
                return true;
            }
            slots.planes = [None, None];
            slots.discard = true;
            if demanded {
                self.reset(*entity);
            } else {
                self.release(*entity);
            }
            slots.writing
        });
        self.retain_entities(requested);
    }
}

impl Default for Outputs<WidgetTexture> {
    fn default() -> Self {
        Self::with_budget(0)
    }
}

pub(crate) struct TextureWrite<'a> {
    outputs: &'a Outputs<WidgetTexture>,
    record: WidgetTextureRecord,
    texture: Option<OutputPlane>,
}

impl TextureWrite<'_> {
    pub(crate) fn texture_mut(&mut self) -> &mut Texture<'static> {
        &mut self.texture.as_mut().expect("active output write").texture
    }

    pub(crate) fn differs_from_previous(&self) -> Result<bool, RenderError> {
        let Some(previous) = self.outputs.previous(self.record.entity) else {
            return Ok(false);
        };
        let previous = previous.texture();
        let current = &self.texture.as_ref().expect("active output write").texture;
        if current.width != previous.width
            || current.height != previous.height
            || current.format != previous.format
        {
            return Ok(false);
        }
        let current = current.read_pixels()?;
        let previous = previous.read_pixels()?;
        let Some(format) = current
            .direct_format()
            .filter(|format| Some(*format) == previous.direct_format())
        else {
            return Ok((0..i32::from(current.height())).any(|y| {
                (0..i32::from(current.width()))
                    .any(|x| current.get_pixel(x, y) != previous.get_pixel(x, y))
            }));
        };
        let row_bytes = usize::from(current.width()) * format.bytes_per_pixel();
        Ok((0..usize::from(current.height())).any(|row| {
            current.bytes()[row * current.stride()..][..row_bytes]
                != previous.bytes()[row * previous.stride()..][..row_bytes]
        }))
    }

    pub(crate) fn publish(mut self) -> bool {
        self.finish(true)
    }

    fn finish(&mut self, publish: bool) -> bool {
        let Some(mut texture) = self.texture.take() else {
            return false;
        };
        let mut storage = self.outputs.storage_mut();
        if storage
            .textures
            .get(&self.record.entity)
            .is_some_and(|slots| slots.discard)
        {
            storage.textures.remove(&self.record.entity);
            return false;
        }
        let published = publish
            && texture.texture.valid_storage()
            && self
                .outputs
                .publish(
                    self.record.entity,
                    self.record.key.generation as u64,
                    self.record,
                )
                .is_ok();
        if published {
            let revision = self
                .outputs
                .state(self.record.entity)
                .and_then(|state| state.current)
                .expect("published output")
                .version;
            texture.texture.set_output_revision(revision);
        }
        let slots = storage
            .textures
            .get_mut(&self.record.entity)
            .expect("reserved output slot");
        slots.planes[self.record.slot] = Some(texture);
        slots.writing = false;
        published
    }
}

impl Drop for TextureWrite<'_> {
    fn drop(&mut self) {
        self.finish(false);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/offscreen/output/native_tests.rs"]
mod native_tests;
#[cfg(test)]
#[path = "../../../tests/unit/ui/offscreen/output/take_tests.rs"]
mod take_tests;
#[cfg(test)]
#[path = "../../../tests/unit/ui/offscreen/output/tests.rs"]
mod tests;
