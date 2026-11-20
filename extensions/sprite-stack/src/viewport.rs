use crate::{Geometry, Object, StackScene, StackSceneHandle, raster::Frame};
use alloc::boxed::Box;
use core::{
    cell::{Cell, RefCell},
    num::NonZeroU16,
};
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
    types::{Fixed, Point, Rect},
    ui::{
        property::{Property, PropertyChange},
        view::PaintCtx,
    },
};

mod draw;
mod overlay;
pub use draw::{ObjectPass, ProjectedObjects, ViewProjection};
pub use overlay::ViewportOverlay;

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

/// Output pixel dimensions and their placement in a scene viewport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSize {
    pixels: [u16; 2],
    scale: Option<NonZeroU16>,
}

impl FrameSize {
    pub const fn stretch(pixels: [u16; 2]) -> Self {
        Self {
            pixels,
            scale: None,
        }
    }

    pub const fn integer(pixels: [u16; 2], scale: NonZeroU16) -> Self {
        Self {
            pixels,
            scale: Some(scale),
        }
    }

    pub const fn pixels(self) -> [u16; 2] {
        self.pixels
    }

    pub const fn integer_scale(self) -> Option<NonZeroU16> {
        self.scale
    }

    fn placed(self, viewport: Rect) -> Rect {
        let Some(scale) = self.scale else {
            return viewport;
        };
        let [w, h] = self
            .pixels
            .map(|side| Fixed::from(side) * Fixed::from(scale.get()));
        Rect::new(
            viewport.x + ((viewport.w - w) / 2).round(),
            viewport.y + ((viewport.h - h) / 2).round(),
            w,
            h,
        )
    }

    /// Maps a point in the viewport's coordinate space into the frame.
    pub fn frame_point(self, viewport: Rect, point: Point) -> Option<Point> {
        if !viewport.w.is_positive() || !viewport.h.is_positive() || self.pixels.contains(&0) {
            return None;
        }
        let rect = self.placed(viewport);
        let delta = Point::new(point.x - rect.x, point.y - rect.y);
        let [sx, sy] = if let Some(scale) = self.scale {
            [Fixed::from(scale.get()); 2]
        } else {
            [
                rect.w / Fixed::from(self.pixels[0]),
                rect.h / Fixed::from(self.pixels[1]),
            ]
        };
        if !sx.is_positive() || !sy.is_positive() {
            return None;
        }
        Some(Point::new(delta.x / sx, delta.y / sy))
    }
}

/// Produces the image of a game-directed camera. The viewport handles invalidation, texture revisions, placement and presentation.
pub trait SceneRenderer: 'static {
    fn frame_size(&self, viewport: [u16; 2]) -> FrameSize {
        FrameSize::stretch(viewport)
    }
    /// Request opaque output bounded by the given bytes per pixel.
    /// The default uses an RGBA frame with transparency.
    fn opaque_frame_byte_limit(&self) -> Option<usize> {
        None
    }
    /// Prepare resources before the viewport allocates a new frame.
    fn prepare_frame(&mut self, _size: FrameSize) -> Result<(), RenderError> {
        Ok(())
    }
    /// Draw into the viewport's frame. Return false to reuse a compatible previous image.
    fn render(
        &mut self,
        scene: &StackScene,
        size: FrameSize,
        target: &mut Frame,
    ) -> Result<bool, RenderError>;
    /// Adds screen-space effects after the scene image is presented.
    fn overlay(&self, _paint: &mut PaintCtx<'_>, _origin: Point, _scale: Fixed) {}
}

enum Renderer {
    Camera(Box<draw::DefaultRenderer>),
    Directed(Box<dyn SceneRenderer>),
}

impl Renderer {
    fn producer(&mut self) -> &mut dyn SceneRenderer {
        match self {
            Self::Camera(renderer) => renderer.as_mut(),
            Self::Directed(renderer) => renderer.as_mut(),
        }
    }
}

struct Output {
    renderer: Renderer,
    frame: Option<Frame>,
    ready: Option<(u64, [u16; 2], FrameSize)>,
    texture_revision: u64,
}

impl Output {
    fn paint(
        &mut self,
        scene: &StackScene,
        camera: Camera,
        stamp: u64,
        paint: &mut PaintCtx<'_>,
        overlay: Option<&ViewportOverlay>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        if !rect.w.is_positive() || !rect.h.is_positive() {
            return Ok(());
        }
        let dimension =
            |v: Fixed| u16::try_from(v.ceil().to_int()).map_err(|_| RenderError::InvalidGeometry);
        let size = [dimension(rect.w)?, dimension(rect.h)?];
        if let Renderer::Camera(renderer) = &mut self.renderer {
            renderer.camera = camera;
        }
        if self
            .ready
            .is_none_or(|(revision, viewport, _)| (revision, viewport) != (stamp, size))
        {
            let renderer = self.renderer.producer();
            let frame_size = renderer.frame_size(size);
            let [width, height] = frame_size.pixels().map(usize::from);
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
                renderer.prepare_frame(frame_size)?;
            }
            let overlay = match self.renderer {
                Renderer::Camera(_) => overlay
                    .map(|layer| layer.prepare([width, height]))
                    .transpose()?,
                Renderer::Directed(_) => None,
            };
            if new_frame {
                let renderer = self.renderer.producer();
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
            let rendered = match &mut self.renderer {
                Renderer::Camera(renderer) => {
                    self.ready = None;
                    renderer.render_with_overlay(scene, frame, overlay.as_deref())?;
                    true
                }
                Renderer::Directed(renderer) => match renderer.render(scene, frame_size, frame) {
                    Ok(rendered) => rendered,
                    Err(error) => {
                        self.ready = None;
                        return Err(error);
                    }
                },
            };
            if rendered {
                self.texture_revision = self.texture_revision.wrapping_add(1);
                frame.texture.cache_revision = self.texture_revision;
            } else if self.ready.is_none() {
                return Ok(());
            }
            self.ready = Some((stamp, size, frame_size));
        }
        let (Some(frame), Some((_, _, frame_size))) = (self.frame.as_ref(), self.ready) else {
            return Ok(());
        };
        let renderer = self.renderer.producer();
        let placement = frame_size.placed(rect);
        let origin = Point::new(placement.x, placement.y);
        let dimensions = Point::new(placement.w, placement.h);
        let scale = frame_size
            .integer_scale()
            .map_or(Fixed::ONE, |scale| Fixed::from(scale.get()));
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

    fn paint_scene(
        &self,
        scene: &StackScene,
        paint: &mut PaintCtx<'_>,
        overlay: Option<&ViewportOverlay>,
    ) -> Result<(), RenderError> {
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
            .paint(scene, camera, self.stamp.get(), paint, overlay)
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
        ViewportOverlay::attach(world, entity);
        PropertyChange::Unchanged
    }
}
#[mirui::event]
fn attach(entity: entity!(), commands: commands!()) {
    commands.set::<Mount>(entity, ());
}

#[mirui::view(component=StackViewport, name="StackViewport", priority=60, invalidation=Visual, attach=attach::callback())]
fn viewport(
    rows: query!(StackViewport, optional ViewportOverlay),
    entity: entity!(),
    paint: paint!(),
) {
    let Some((component, overlay)) = rows.get(entity) else {
        return;
    };
    if let Some(scene) = &component.scene {
        let result = scene.read(|scene| component.paint_scene(scene, paint, overlay));
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
