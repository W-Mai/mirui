use super::{FrameSize, content_draw::FrameDraw};
use crate::{
    Pose,
    raster::{FrameCamera, FrameEffects, SceneLayers},
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
pub struct TerrainTarget(pub(super) TypeId);

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
