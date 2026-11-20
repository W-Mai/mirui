use crate::{Geometry, Object, StackScene, StackSceneHandle, raster::Frame};
use alloc::boxed::Box;
use core::cell::{Cell, RefCell};
use mirui::{
    __macro_support::{BindingRole, OwnedEffects, prepare_widget_effect, with_owned_effects},
    app::{App, RendererFactory, plugin::Plugin},
    core::model::{ModelHandle, ModelInvalidation, ModelSubscription, SharedValue},
    ecs::{Entity, World},
    render::{
        RenderError,
        command::{CompositeMode, DrawCommand},
        texture::AlphaMode,
    },
    surface::Surface,
    types::{Fixed, Point},
    ui::{
        property::{Property, PropertyChange},
        view::PaintCtx,
    },
};

mod draw;
pub use draw::{ObjectPass, ProjectedObjects, ViewProjection};

/// Camera around a world point or a moving object. Positive pitch looks down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Camera {
    pub target: Option<Object>,
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
            target: None,
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

/// Produces the image of a game-directed camera. The viewport handles invalidation, texture revisions, placement and presentation.
pub trait SceneRenderer: 'static {
    fn frame_size(&self, viewport: [u16; 2]) -> [u16; 2] {
        viewport
    }
    /// Request opaque output bounded by the given bytes per pixel.
    /// The default uses an RGBA frame with transparency.
    fn opaque_frame_byte_limit(&self) -> Option<usize> {
        None
    }
    /// Prepare resources before the viewport allocates a new frame.
    fn prepare_frame(&mut self, _viewport: [u16; 2]) -> Result<(), RenderError> {
        Ok(())
    }
    /// Draw into the viewport's frame. Return false to reuse a compatible previous image.
    fn render(
        &mut self,
        scene: &StackScene,
        size: [u16; 2],
        target: &mut Frame,
    ) -> Result<bool, RenderError>;
    /// Centers native scene pixels at this integer scale. By default the frame fills the viewport.
    fn pixel_scale(&self) -> Option<core::num::NonZeroU16> {
        None
    }
    /// Adds screen-space effects after the scene image is presented.
    fn overlay(&self, _paint: &mut PaintCtx<'_>, _origin: Point, _scale: Fixed) {}
}

enum Renderer {
    Camera(Box<draw::DefaultRenderer>),
    Directed(Box<dyn SceneRenderer>),
}

struct Output {
    renderer: Renderer,
    frame: Option<Frame>,
    ready: Option<(u64, [u16; 2])>,
    texture_revision: u64,
}

impl Output {
    fn paint(
        &mut self,
        scene: &StackScene,
        camera: Camera,
        stamp: u64,
        paint: &mut PaintCtx<'_>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        if !rect.w.is_positive() || !rect.h.is_positive() {
            return Ok(());
        }
        let dimension =
            |v: Fixed| u16::try_from(v.ceil().to_int()).map_err(|_| RenderError::InvalidGeometry);
        let size = [dimension(rect.w)?, dimension(rect.h)?];
        let renderer: &mut dyn SceneRenderer = match &mut self.renderer {
            Renderer::Camera(r) => {
                r.camera = camera;
                r.as_mut()
            }
            Renderer::Directed(r) => r.as_mut(),
        };
        if self.ready != Some((stamp, size)) {
            let [width, height] = renderer.frame_size(size).map(usize::from);
            if width == 0 || height == 0 {
                return Err(RenderError::InvalidGeometry);
            }
            let new_frame = self
                .frame
                .as_ref()
                .is_none_or(|f| [f.width, f.height] != [width, height]);
            if new_frame {
                self.ready = None;
                self.frame = None;
                renderer.prepare_frame(size)?;
                let frame = if let Some(limit) = renderer.opaque_frame_byte_limit() {
                    Frame::with_format(width, height, paint.opaque_output_format(limit)?)?
                } else {
                    let mut frame = Frame::new(width, height)?;
                    frame.texture.alpha_mode = AlphaMode::Blend;
                    frame
                };
                self.frame = Some(frame);
            }
            let frame = self.frame.as_mut().expect("viewport frame");
            if renderer.render(scene, size, frame)? {
                self.texture_revision = self.texture_revision.wrapping_add(1);
                frame.texture.cache_revision = self.texture_revision;
            } else if self.ready.is_none() {
                return Ok(());
            }
            self.ready = Some((stamp, size));
        }
        let Some(frame) = self.frame.as_ref() else {
            return Ok(());
        };
        let (origin, dimensions, scale) = if let Some(scale) = renderer.pixel_scale() {
            let scale = Fixed::from(scale.get());
            let dimensions = Point::new(
                Fixed::from_int(frame.width as i32) * scale,
                Fixed::from_int(frame.height as i32) * scale,
            );
            let origin = Point::new(
                rect.x + ((rect.w - dimensions.x) / 2).round(),
                rect.y + ((rect.h - dimensions.y) / 2).round(),
            );
            (origin, dimensions, scale)
        } else {
            (
                Point::new(rect.x, rect.y),
                Point::new(rect.w, rect.h),
                Fixed::ONE,
            )
        };
        if frame.texture.alpha_mode == AlphaMode::Opaque
            && dimensions.x >= rect.w
            && dimensions.y >= rect.h
        {
            paint.set_background_handled(true);
        }
        paint.draw(&DrawCommand::Blit {
            pos: origin,
            size: dimensions,
            transform: paint.transform(),
            quad: None,
            texture: &frame.texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
        renderer.overlay(paint, origin, scale);
        Ok(())
    }
}

#[mirui::component]
pub struct StackViewport {
    #[property(ViewportScene)]
    pub scene: Option<StackSceneHandle>,
    #[property(ViewportCamera)]
    pub camera: Camera,
    bindings: Option<(ModelSubscription, OwnedEffects)>,
    output: RefCell<Output>,
    stamp: Cell<u64>,
    seen_scene: Cell<u64>,
    tracked: Cell<Option<[Fixed; 3]>>,
}

impl Default for StackViewport {
    fn default() -> Self {
        Self::new(None, Renderer::Camera(Box::default()))
    }
}

impl StackViewport {
    fn new(scene: Option<StackSceneHandle>, renderer: Renderer) -> Self {
        Self {
            scene,
            camera: Camera::default(),
            bindings: None,
            output: RefCell::new(Output {
                renderer,
                frame: None,
                ready: None,
                texture_revision: 0,
            }),
            stamp: Cell::new(0),
            seen_scene: Cell::new(0),
            tracked: Cell::new(None),
        }
    }

    /// Installs a game-directed camera. Assigning the camera property selects an ordinary scene camera for this viewport.
    pub fn with_renderer(scene: StackSceneHandle, renderer: impl SceneRenderer) -> Self {
        Self::new(Some(scene), Renderer::Directed(Box::new(renderer)))
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
        self.output
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
        let output = view.output.get_mut();
        if matches!(&output.renderer, Renderer::Directed(_)) {
            output.renderer = Renderer::Camera(Box::default());
            output.frame = None;
            output.ready = None;
        } else if view.camera == camera {
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
