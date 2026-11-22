use crate::{StackStage, StackStageHandle};
use core::cell::{Cell, OnceCell, RefCell};
use mirui::{
    __macro_support::{
        AttachDefinition, BindingRole, OwnedEffects, PreparedAttach, QueryReadScope,
        RuntimeInstallError, ViewDescriptor, assemble_view, prepare_attach, prepare_widget_effect,
        report_query_reads, with_owned_effects,
    },
    app::{App, RendererFactory, plugin::Plugin},
    core::model::{ModelHandle, SharedValue},
    ecs::{CommandCommitError, DeferredOperation, Entity, World},
    render::RenderError,
    surface::Surface,
    types::Fixed,
    ui::{
        property::{Property, PropertyChange},
        view::{PaintCtx, View, ViewInvalidation},
    },
};

mod content;
mod content_draw;
mod output;
mod resolution;
use output::Output;
pub use resolution::{FrameSize, Resolution};
mod draw;
mod overlay;
pub use content::{
    ContentFrame, FrameContent, TerrainMember, TerrainTarget, ViewportContent, ViewportOutput,
    WorldCamera,
};
pub use draw::ViewProjection;
pub use overlay::ViewportOverlay;

/// Camera around a world point or a moving object. Positive pitch looks down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Camera {
    pub center: [Fixed; 3],
    pub yaw: Fixed,
    pub pitch: Fixed,
    pub scale: Fixed,
    /// Distance from the eye to the center plane in world units. None uses orthographic projection.
    pub focal_distance: Option<Fixed>,
    /// Half-extent in world units of the sampled terrain around the target.
    pub range: Fixed,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [Fixed::ZERO; 3],
            yaw: Fixed::ZERO,
            pitch: Fixed::from_int(35),
            scale: Fixed::ONE,
            focal_distance: None,
            range: Fixed::from_int(64),
        }
    }
}
impl Camera {
    pub fn overhead() -> Self {
        Self {
            pitch: Fixed::from_int(90),
            ..Self::default()
        }
    }
}

#[mirui::component]
pub struct StackViewport {
    #[property(ViewportStage)]
    pub stage: Option<StackStageHandle>,
    #[property(ViewportCamera)]
    pub camera: Camera,
    #[property(ViewportResolution)]
    pub resolution: Resolution,
    bindings: Option<OwnedEffects>,
    output: RefCell<Output>,
    stamp: Cell<u64>,
}

impl Default for StackViewport {
    fn default() -> Self {
        Self {
            stage: None,
            camera: Camera::default(),
            resolution: Resolution::Native,
            bindings: None,
            output: RefCell::new(Output::default()),
            stamp: Cell::new(0),
        }
    }
}

#[derive(Clone, Copy)]
struct Attachments<'a> {
    overlay: Option<&'a ViewportOverlay>,
    content: Option<&'a ViewportContent>,
}

impl StackViewport {
    fn advance_stamp(&self) {
        self.stamp.set(self.stamp.get().wrapping_add(1));
    }

    fn paint_stage(
        &self,
        stage: &StackStage,
        world: &World,
        paint: &mut PaintCtx<'_>,
        attachments: Attachments<'_>,
    ) -> Result<(), RenderError> {
        stage
            .check_member_owner(world)
            .map_err(|_| RenderError::InvalidGeometry)?;
        self.output.borrow_mut().paint(
            draw::DrawSource { stage, world },
            self.camera,
            self.resolution,
            self.stamp.get(),
            paint,
            attachments,
        )
    }

    fn bind(world: &mut World, entity: Entity) {
        if let Some(stage) = world.get::<Self>(entity).and_then(|v| v.stage.clone()) {
            let lifetime = OnceCell::new();
            let (_, effects) = with_owned_effects(|| {
                prepare_widget_effect(entity, BindingRole::Display, move || {
                    let lifetime = lifetime.get_or_init(|| stage.access().observe_lifetime());
                    if lifetime.as_ref().is_some_and(|live| live()) {
                        stage.assert_registered_owner();
                        stage.revision();
                        mirui::ui::property::apply::<SourceChanged>(entity, ());
                    } else {
                        mirui::ui::property::apply::<StageRetired>(entity, ());
                    }
                })
                .install(world);
            });
            let view = world.get_mut::<Self>(entity).expect("mounted viewport");
            if view.stage.is_some() {
                view.bindings = Some(effects);
            }
        }
    }
}

pub struct ViewportStage;
impl Property for ViewportStage {
    type Value = StackStageHandle;
    fn apply(world: &mut World, entity: Entity, stage: Self::Value) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        view.bindings = None;
        view.advance_stamp();
        view.stage = Some(stage);
        StackViewport::bind(world, entity);
        PropertyChange::Visual
    }
}

pub struct ViewportCamera;
impl Property for ViewportCamera {
    type Value = Camera;
    fn apply(world: &mut World, entity: Entity, camera: Camera) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        if view.camera == camera {
            return PropertyChange::Unchanged;
        }
        view.camera = camera;
        view.advance_stamp();
        PropertyChange::Visual
    }
}

pub struct ViewportResolution;
impl Property for ViewportResolution {
    type Value = Resolution;

    fn apply(world: &mut World, entity: Entity, resolution: Resolution) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        if view.resolution == resolution {
            return PropertyChange::Unchanged;
        }
        view.resolution = resolution;
        view.advance_stamp();
        PropertyChange::Visual
    }
}

struct SourceChanged;
impl Property for SourceChanged {
    type Value = ();
    fn apply(world: &mut World, entity: Entity, _: ()) -> PropertyChange {
        let Some(view) = world.get::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        view.advance_stamp();
        PropertyChange::Visual
    }
}

struct StageRetired;
impl Property for StageRetired {
    type Value = ();
    fn apply(world: &mut World, entity: Entity, _: ()) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        if view.stage.take().is_none() {
            return PropertyChange::Unchanged;
        }
        view.bindings = None;
        view.output.get_mut().clear();
        PropertyChange::Visual
    }
}

struct Mount(Entity);
impl DeferredOperation for Mount {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let Self(entity) = self;
        let Some(view) = world.get::<StackViewport>(entity) else {
            return Ok(());
        };
        if view.stage.is_some() && view.bindings.is_none() {
            StackViewport::bind(world, entity);
        }
        ViewportOverlay::attach(world, entity);
        ViewportContent::attach(world, entity)
    }
}
#[mirui::event]
fn attach(entity: entity!(), commands: commands!()) {
    commands.enqueue(Mount(entity));
}

fn prepare_mount(world: &mut World) -> Result<PreparedAttach, RuntimeInstallError> {
    prepare_attach(world, attach::callback())
}

struct MountOverlay;
impl Property for MountOverlay {
    type Value = ();

    fn apply(world: &mut World, entity: Entity, _: ()) -> PropertyChange {
        if !world.has::<StackViewport>(entity) {
            return PropertyChange::Unchanged;
        }
        ViewportOverlay::attach(world, entity);
        SourceChanged::apply(world, entity, ())
    }
}

#[mirui::event]
fn attach_overlay(entity: entity!(), commands: commands!()) {
    commands.set::<MountOverlay>(entity, ());
}

#[mirui::view(component = ViewportOverlay, attach = attach_overlay::callback(), invalidation = Visual)]
fn overlay_attachment() {}

fn paint_viewport(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let Some(component) = world.get::<StackViewport>(entity) else {
        return;
    };
    let attachments = Attachments {
        overlay: world.get(entity),
        content: world.get(entity),
    };
    let result = if let Some(stage) = &component.stage {
        stage.read(|stage| component.paint_stage(stage, world, paint, attachments))
    } else {
        return;
    };
    if let Err(error) = result {
        paint.fail(error);
    }
    report_query_reads(paint, &QueryReadScope::new(entity));
}

fn viewport() -> View {
    assemble_view::<StackViewport>(ViewDescriptor {
        name: "StackViewport",
        paint: Some((60, paint_viewport)),
        attach: Some(AttachDefinition::Callback(prepare_mount)),
        input: None,
        observation: None,
        query_dependencies: &[
            core::any::TypeId::of::<ViewportOverlay>,
            core::any::TypeId::of::<ViewportContent>,
        ],
        systems: &[],
        invalidation: ViewInvalidation::Visual,
    })
}

fn content_attachment() -> View {
    assemble_view::<ViewportContent>(ViewDescriptor {
        name: "ViewportContent",
        paint: None,
        attach: Some(AttachDefinition::Callback(prepare_mount)),
        input: None,
        observation: None,
        query_dependencies: &[],
        systems: &[],
        invalidation: ViewInvalidation::Visual,
    })
}

/// Registers SpriteStack and StackViewport paint and attachment views at priority 60.
pub struct SpriteStackPlugin;
impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for SpriteStackPlugin {
    fn build(&mut self, app: &mut App<B, F>) {
        app.with_widget(crate::view())
            .with_widget(viewport())
            .with_widget(overlay_attachment::view())
            .with_widget(content_attachment());
    }
}
