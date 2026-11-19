use crate::{Geometry, Object, StackScene, StackSceneHandle};
use alloc::boxed::Box;
use core::cell::{Cell, RefCell};
use mirui::{
    __macro_support::{BindingRole, OwnedEffects, prepare_widget_effect, with_owned_effects},
    app::{App, RendererFactory, plugin::Plugin},
    core::model::{ModelHandle, ModelInvalidation, ModelSubscription, SharedValue},
    ecs::{Entity, World},
    render::RenderError,
    surface::Surface,
    types::Fixed,
    ui::{
        property::{Property, PropertyChange},
        view::PaintCtx,
    },
};

mod draw;

/// Orthographic camera around a world point or a moving object. Positive pitch looks down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Camera {
    pub target: Option<Object>,
    pub center: [Fixed; 3],
    pub yaw: Fixed,
    pub pitch: Fixed,
    pub scale: Fixed,
    /// Half-extent in world units of the sampled terrain around the target.
    pub range: Fixed,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            target: None,
            center: [Fixed::ZERO; 3],
            yaw: Fixed::ZERO,
            pitch: Fixed::from_int(35),
            scale: Fixed::ONE,
            range: Fixed::from_int(64),
        }
    }
}
impl Camera {
    pub fn chase(object: Object) -> Self {
        Self {
            target: Some(object),
            ..Self::default()
        }
    }
    pub fn overhead() -> Self {
        Self {
            pitch: Fixed::from_int(90),
            ..Self::default()
        }
    }
}

/// Paints one view of shared game state. The painter and its workspace belong to that viewport.
/// The revision changes on scene, camera or live geometry edits. Dimensions come from the paint rectangle. The camera center passed to the painter is the resolved world position including its target offset.
pub trait ScenePainter: 'static {
    fn paint(
        &mut self,
        scene: &StackScene,
        camera: Camera,
        revision: u64,
        paint: &mut PaintCtx<'_>,
    ) -> Result<(), RenderError>;
}

#[mirui::component]
pub struct StackViewport {
    #[property(ViewportScene)]
    pub scene: Option<StackSceneHandle>,
    #[property(ViewportCamera)]
    pub camera: Camera,
    bindings: Option<(ModelSubscription, OwnedEffects)>,
    painter: RefCell<Box<dyn ScenePainter>>,
    stamp: Cell<u64>,
    seen_scene: Cell<u64>,
    tracked: Cell<Option<[Fixed; 3]>>,
}

impl Default for StackViewport {
    fn default() -> Self {
        Self {
            scene: None,
            camera: Camera::default(),
            bindings: None,
            painter: RefCell::new(Box::new(draw::DefaultPainter::default())),
            stamp: Cell::new(0),
            seen_scene: Cell::new(0),
            tracked: Cell::new(None),
        }
    }
}

impl StackViewport {
    /// Supplies per-viewport drawing state for a game with its own materials and effects.
    pub fn with_painter(
        scene: StackSceneHandle,
        camera: Camera,
        painter: impl ScenePainter,
    ) -> Self {
        Self {
            scene: Some(scene),
            camera,
            bindings: None,
            painter: RefCell::new(Box::new(painter)),
            stamp: Cell::new(0),
            seen_scene: Cell::new(0),
            tracked: Cell::new(None),
        }
    }

    fn advance_stamp(&self) {
        self.stamp.set(self.stamp.get().wrapping_add(1));
    }

    fn paint_scene(&self, scene: &StackScene, paint: &mut PaintCtx<'_>) -> Result<(), RenderError> {
        let revision = scene.revision();
        if self.seen_scene.replace(revision) != revision {
            self.advance_stamp();
        }
        let mut camera = self.camera;
        if let Some(target) = camera.target {
            let position = scene
                .pose(target)
                .map(|p| p.position)
                .or_else(|| self.tracked.get())
                .ok_or(RenderError::InvalidGeometry)?;
            self.tracked.set(Some(position));
            camera.center = core::array::from_fn(|i| position[i] + camera.center[i]);
        }
        self.painter
            .borrow_mut()
            .paint(scene, camera, self.stamp.get(), paint)
    }

    fn bind(world: &mut World, entity: Entity) {
        let Some(scene) = world.get::<Self>(entity).and_then(|v| v.scene.clone()) else {
            return;
        };
        // The ordinary field observer drives pose changes. The effect only tracks source membership and mutable geometry.
        let sub = scene.__mirui_subscribe_revision(world, entity, ModelInvalidation::Visual);
        let (_, effects) = with_owned_effects(|| {
            prepare_widget_effect(entity, BindingRole::Display, move || {
                scene.assert_registered_owner();
                scene.membership();
                scene.read(|scene| {
                    scene.for_each(|_, geometry, _| {
                        if let Geometry::LiveField(field) = geometry {
                            field.assert_registered_owner();
                            field.revision();
                        }
                    })
                });
                mirui::ui::property::apply::<SourceChanged>(entity, ());
            })
            .install(world);
        });
        world
            .get_mut::<Self>(entity)
            .expect("mounted viewport")
            .bindings = Some((sub, effects));
    }
}

pub struct ViewportScene;
impl Property for ViewportScene {
    type Value = StackSceneHandle;
    fn apply(world: &mut World, entity: Entity, scene: Self::Value) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        view.bindings = None;
        view.tracked.set(None);
        view.advance_stamp();
        view.scene = Some(scene);
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
        if camera.target != view.camera.target {
            view.tracked.set(None);
        }
        view.camera = camera;
        view.advance_stamp();
        PropertyChange::Visual
    }
}

struct SourceChanged;
impl Property for SourceChanged {
    type Value = ();
    fn apply(world: &mut World, entity: Entity, _: ()) -> PropertyChange {
        let Some(view) = world.get_mut::<StackViewport>(entity) else {
            return PropertyChange::Unchanged;
        };
        view.advance_stamp();
        PropertyChange::Visual
    }
}

struct Mount;
impl Property for Mount {
    type Value = ();
    fn apply(world: &mut World, entity: Entity, _: ()) -> PropertyChange {
        if world
            .get::<StackViewport>(entity)
            .is_some_and(|v| v.scene.is_some() && v.bindings.is_none())
        {
            StackViewport::bind(world, entity);
        }
        PropertyChange::Unchanged
    }
}
#[mirui::event]
fn attach(entity: entity!(), commands: commands!()) {
    commands.set::<Mount>(entity, ());
}

#[mirui::view(component=StackViewport, name="StackViewport", priority=60, invalidation=Visual, attach=attach::callback())]
fn viewport(component: component!(), paint: paint!()) {
    if let Some(scene) = &component.scene {
        let result = scene.read(|scene| component.paint_scene(scene, paint));
        if let Err(e) = result {
            paint.fail(e);
        }
    }
}

/// Registers SpriteStack and StackViewport paint and attachment views at priority 60.
pub struct SpriteStackPlugin;
impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for SpriteStackPlugin {
    fn build(&mut self, app: &mut App<B, F>) {
        app.with_widget(crate::view()).with_widget(viewport::view());
    }
}
