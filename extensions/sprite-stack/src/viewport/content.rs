use super::{
    FrameSize, ViewProjection,
    draw::{DrawSource, StageFrame, ViewObjects},
};
use crate::{
    Geometry, Pose, StackVisual,
    raster::{
        CameraWorkspace, Frame, FrameCamera, FrameEffects, FrameOverlay, RollMapping, SceneLayers,
        draw_layers_with_objects,
    },
};
use alloc::rc::{Rc, Weak};
use core::{
    any::TypeId,
    cell::{Cell, OnceCell, RefCell},
};
use mirui::{
    __macro_support::{BindingRole, OwnedEffects, prepare_widget_effect, with_owned_effects},
    core::model::{BindingOwner, ModelHandle},
    ecs::{CommandCommitError, Entity, World},
    render::RenderError,
    types::{Fixed, Point},
    ui::{Prop, view::PaintCtx},
};

/// A camera and the output-space pivot of its roll.
#[derive(Clone, Copy)]
pub struct WorldCamera {
    pub camera: FrameCamera,
    pub pivot: Fixed,
    /// Enable roll mapping during a transition even when the angle passes through zero.
    pub roll: bool,
}

/// Storage policy for opaque viewport content.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewportOutput {
    #[default]
    Full,
    /// Tile source color and depth and use a two-byte opaque output format.
    Bounded,
}

/// Supplies camera and layer data for a viewport. SpriteStack owns the frame,
/// projection mapping, tile order and presentation.
pub(super) trait FrameSource: 'static {
    fn camera(&self, size: FrameSize) -> Result<WorldCamera, RenderError>;

    fn available(&self) -> bool {
        true
    }

    fn output(&self) -> ViewportOutput {
        ViewportOutput::Full
    }

    /// Prepares procedural layers and renders the frame.
    fn prepare(&mut self, frame: &mut FrameDraw<'_>) -> Result<(), RenderError>;

    /// Paints effects at display resolution after the viewport image.
    fn present(&self, _paint: &mut PaintCtx<'_>, _origin: Point, _scale: Fixed) {}
}

/// Frame inputs available to procedural content before the stage draws.
#[derive(Clone, Copy)]
pub struct ContentFrame {
    pub size: FrameSize,
    pub source_size: [usize; 2],
    pub camera: FrameCamera,
    pub terrain: Option<TerrainMember>,
}

#[derive(Clone, Copy)]
pub struct TerrainMember {
    pub entity: Entity,
    pub pose: Pose,
}

#[derive(Clone, Copy)]
pub struct TerrainTarget(TypeId);

impl TerrainTarget {
    pub fn role<R: 'static>() -> Self {
        Self(TypeId::of::<R>())
    }
}

/// Provides borrowed procedural layers and effects for automatic stage drawing.
pub trait FrameContent<M = ()>: 'static {
    fn camera(&self, model: &M, size: FrameSize) -> Result<WorldCamera, RenderError>;

    fn output(&self) -> ViewportOutput {
        ViewportOutput::Full
    }

    fn terrain(&self) -> Option<TerrainTarget> {
        None
    }

    fn prepare<'a>(
        &'a mut self,
        model: &'a M,
        input: ContentFrame,
    ) -> Result<(impl SceneLayers + 'a, impl FrameEffects + 'a), RenderError>;

    fn finish(&mut self, _: &M, _: ContentFrame, _: usize) -> Result<(), RenderError> {
        Ok(())
    }

    fn present(&self, _paint: &mut PaintCtx<'_>, _origin: Point, _scale: Fixed) {}
}

struct Automatic<C>(C);

impl<C: FrameContent> FrameSource for Automatic<C> {
    fn camera(&self, size: FrameSize) -> Result<WorldCamera, RenderError> {
        self.0.camera(&(), size)
    }

    fn output(&self) -> ViewportOutput {
        self.0.output()
    }

    fn prepare(&mut self, frame: &mut FrameDraw<'_>) -> Result<(), RenderError> {
        prepare_and_draw(&mut self.0, &(), frame)
    }

    fn present(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        self.0.present(paint, origin, scale);
    }
}

struct ModelContent<H, C> {
    model: H,
    content: C,
}

impl<H, C> FrameSource for ModelContent<H, C>
where
    H: ModelHandle + 'static,
    C: FrameContent<H::Data>,
{
    fn available(&self) -> bool {
        self.model.callback_ready()
    }

    fn camera(&self, size: FrameSize) -> Result<WorldCamera, RenderError> {
        self.model.read(|model| self.content.camera(model, size))
    }

    fn output(&self) -> ViewportOutput {
        self.content.output()
    }

    fn prepare(&mut self, frame: &mut FrameDraw<'_>) -> Result<(), RenderError> {
        self.model
            .read(|model| prepare_and_draw(&mut self.content, model, frame))
    }

    fn present(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        self.content.present(paint, origin, scale);
    }
}

fn prepare_and_draw<M>(
    content: &mut impl FrameContent<M>,
    model: &M,
    frame: &mut FrameDraw<'_>,
) -> Result<(), RenderError> {
    let inputs = ContentFrame {
        size: frame.size(),
        source_size: frame.source_size(),
        camera: frame.camera(),
        terrain: match content.terrain() {
            Some(target) => frame.resolve_terrain(target)?,
            None => None,
        },
    };
    {
        let (layers, mut effects) = content.prepare(model, inputs)?;
        frame.draw(&layers, &mut effects, inputs.terrain)?;
    }
    content.finish(model, inputs, frame.storage_bytes())
}

/// Attaches procedural frame content to a StackViewport.
#[mirui::component]
pub struct ViewportContent {
    pub(super) source: Rc<RefCell<dyn FrameSource>>,
    owner: BindingOwner,
    input: Option<Prop<Option<u64>>>,
    effects: Option<OwnedEffects>,
}

impl ViewportContent {
    pub(super) fn identity(&self) -> Weak<RefCell<dyn FrameSource>> {
        Rc::downgrade(&self.source)
    }

    pub(super) fn matches(&self, identity: &Weak<RefCell<dyn FrameSource>>) -> bool {
        core::ptr::addr_eq(identity.as_ptr(), Rc::as_ptr(&self.source))
    }

    pub fn new(content: impl FrameContent) -> Self {
        Self::from_source(Automatic(content))
    }

    fn from_source(content: impl FrameSource) -> Self {
        Self {
            source: Rc::new(RefCell::new(content)),
            owner: BindingOwner::unbound(),
            input: None,
            effects: None,
        }
    }

    pub fn from_model<H, C>(model: &H, observe: impl Fn(&H) -> u64 + 'static, content: C) -> Self
    where
        H: ModelHandle + 'static,
        C: FrameContent<H::Data>,
    {
        let mut mounted = Self::from_source(ModelContent {
            model: model.clone(),
            content,
        });
        mounted.owner = BindingOwner::registered(model);
        let model = model.clone();
        let retirement = OnceCell::new();
        mounted.input = Some(Prop::new(move || {
            let live = retirement.get_or_init(|| model.access().observe_lifetime());
            (live.as_ref().is_some_and(|read| read()) && model.callback_ready())
                .then(|| observe(&model))
        }));
        mounted
    }

    pub(super) fn available(&self) -> bool {
        (self.input.is_none() || self.effects.is_some()) && self.source.borrow().available()
    }

    pub(super) fn attach(world: &mut World, entity: Entity) -> Result<(), CommandCommitError> {
        let Some(content) = world.get::<Self>(entity) else {
            return Ok(());
        };
        if content.effects.is_some() {
            return Ok(());
        }
        let Some(input) = content.input.clone() else {
            return Ok(());
        };
        content
            .owner
            .check_world(world)
            .map_err(|_| CommandCommitError::ForeignOwner {
                service: "stack viewport content",
            })?;
        let (_, effects) = with_owned_effects(|| {
            let previous = Cell::new(None);
            prepare_widget_effect(entity, BindingRole::Display, move || {
                let revision = input.get();
                if previous.replace(Some(revision)) != Some(revision) {
                    mirui::ui::property::apply::<super::SourceChanged>(entity, ());
                }
            })
            .install(world);
        });
        world
            .get_mut::<Self>(entity)
            .expect("attached content")
            .effects = Some(effects);
        Ok(())
    }
}

pub(super) struct WorldStorage {
    workspace: CameraWorkspace,
    objects: ViewObjects,
    size: [usize; 2],
    pivot: Fixed,
    bounded: bool,
}

impl WorldStorage {
    pub(super) fn new(
        size: [usize; 2],
        pivot: Fixed,
        output: ViewportOutput,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            workspace: CameraWorkspace::new(size, pivot, output == ViewportOutput::Bounded)?,
            objects: ViewObjects::default(),
            size,
            pivot,
            bounded: output == ViewportOutput::Bounded,
        })
    }

    pub(super) fn matches(&self, size: [usize; 2], pivot: Fixed) -> bool {
        self.size == size && self.pivot == pivot
    }

    pub(super) fn begin<'a>(
        &'a mut self,
        size: FrameSize,
        view: WorldCamera,
        source: DrawSource<'a>,
        range: Fixed,
        overlay: Option<&'a dyn FrameOverlay>,
        output: &'a mut Frame,
    ) -> FrameDraw<'a> {
        let mut camera = view.camera;
        let rotation = (view.roll || camera.roll != Fixed::ZERO)
            .then(|| (Fixed::sin_rad(camera.roll), Fixed::cos_rad(camera.roll)));
        let mapping = RollMapping::new(self.size, view.pivot, rotation);
        if mapping.rolled {
            let axis = Fixed::from_int(self.size[0] as i32) / 2;
            camera.projection.center =
                mapping.projection_center([axis, camera.projection.center[1]]);
        }
        FrameDraw {
            size,
            camera,
            source,
            range,
            mapping,
            overlay,
            output,
            workspace: &mut self.workspace,
            objects: &mut self.objects,
            bounded: self.bounded,
        }
    }
}

/// Inputs and reusable object preparation for one frame.
pub(super) struct FrameDraw<'a> {
    size: FrameSize,
    camera: FrameCamera,
    source: DrawSource<'a>,
    range: Fixed,
    mapping: RollMapping,
    overlay: Option<&'a dyn FrameOverlay>,
    output: &'a mut Frame,
    workspace: &'a mut CameraWorkspace,
    objects: &'a mut ViewObjects,
    bounded: bool,
}

impl FrameDraw<'_> {
    pub fn size(&self) -> FrameSize {
        self.size
    }

    /// Camera in source coordinates after roll mapping.
    pub fn camera(&self) -> FrameCamera {
        self.camera
    }

    pub fn source_size(&self) -> [usize; 2] {
        self.mapping.size
    }

    /// Bytes allocated for viewport pixels and reusable scene work.
    pub fn storage_bytes(&self) -> usize {
        core::mem::size_of::<WorldStorage>()
            + self.workspace.bytes()
            + self.objects.storage_bytes()
            + self.output.bytes()
    }

    fn resolve_terrain(&self, target: TerrainTarget) -> Result<Option<TerrainMember>, RenderError> {
        let DrawSource { stage, world } = self.source;
        stage.with_entities(|members| {
            let mut matched = None;
            for &entity in members {
                let Some(visual) = world.get::<StackVisual>(entity) else {
                    continue;
                };
                if visual.role() != Some(target.0) {
                    continue;
                }
                let Some(geometry) = visual.drawable_geometry() else {
                    continue;
                };
                if matched.is_some()
                    || !matches!(geometry, Geometry::Field(_) | Geometry::LiveField(_))
                {
                    return Err(RenderError::InvalidGeometry);
                }
                matched = Some(TerrainMember {
                    entity,
                    pose: visual.pose(),
                });
            }
            Ok(matched)
        })
    }

    /// Submits procedural layers and the viewport's members through the same color and depth passes.
    fn draw(
        &mut self,
        layers: &impl SceneLayers,
        effects: &mut impl FrameEffects,
        terrain: Option<TerrainMember>,
    ) -> Result<(), RenderError> {
        self.objects.with_stage(
            self.source.stage,
            self.source.world,
            ViewProjection::from_frame(self.camera, self.range),
            Some(StageFrame {
                camera: self.camera,
                size: self.mapping.size,
                bounded: self.bounded,
                terrain: terrain.map(|member| member.entity),
                mask_pixels: self.workspace.coverage_pixels(),
            }),
            |objects| {
                self.workspace.render_with_effects(
                    self.output,
                    &self.mapping,
                    self.overlay,
                    effects,
                    |tile, pass| {
                        draw_layers_with_objects(layers, tile, pass, |tile, styled| {
                            if styled {
                                objects.draw_styled(tile)
                            } else {
                                objects.draw(tile)
                            }
                        })
                    },
                )
            },
        )
    }
}
