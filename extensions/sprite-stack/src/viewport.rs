use crate::{
    Geometry, Object, StackScene, StackSceneHandle, StackStage, StackStageHandle,
    raster::{Frame, OverlayFrame},
};
use alloc::{boxed::Box, rc::Weak};
use core::{
    cell::{Cell, OnceCell, RefCell},
    num::NonZeroU16,
};
use mirui::{
    __macro_support::{
        AttachDefinition, BindingRole, OwnedEffects, PreparedAttach, QueryReadScope,
        RuntimeInstallError, ViewDescriptor, assemble_view, prepare_attach, prepare_widget_effect,
        report_query_reads, with_owned_effects,
    },
    app::{App, RendererFactory, plugin::Plugin},
    core::model::{ModelHandle, ModelInvalidation, ModelSubscription, SharedValue},
    ecs::{CommandCommitError, DeferredOperation, Entity, World},
    render::{
        RenderError,
        command::{CompositeMode, DrawCommand},
        texture::AlphaMode,
    },
    surface::Surface,
    types::{Fixed, Point, Rect},
    ui::{
        property::{Property, PropertyChange},
        view::{PaintCtx, View, ViewInvalidation},
    },
};

mod content;
mod draw;
mod overlay;
pub use content::{
    ContentFrame, FrameContent, TerrainMember, TerrainTarget, ViewportContent, ViewportOutput,
    WorldCamera,
};
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

/// Selects the raster dimensions independently of layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Resolution {
    /// Render at one pixel per logical viewport unit.
    #[default]
    Native,
    /// Render at an integer scale within the maximum frame extent.
    Integer {
        limit: [NonZeroU16; 2],
        minimum: [u16; 2],
        scale: NonZeroU16,
    },
}

impl Resolution {
    /// Resolves the frame pixels and placement for this viewport.
    pub fn fit(self, viewport: [u16; 2]) -> FrameSize {
        let Self::Integer {
            limit,
            minimum,
            scale,
        } = self
        else {
            return FrameSize::stretch(viewport);
        };
        let [max_width, max_height] = limit.map(NonZeroU16::get);
        let [width, height] = viewport;
        let scale = scale
            .get()
            .max(width.div_ceil(max_width))
            .max(height.div_ceil(max_height));
        let pixels = [
            width.div_ceil(scale).max(minimum[0].min(max_width)),
            height.div_ceil(scale).max(minimum[1].min(max_height)),
        ];
        FrameSize::integer(
            pixels,
            NonZeroU16::new(scale).expect("positive placement scale"),
        )
    }
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

enum Storage {
    Rows(Box<draw::DefaultRenderer>),
    World(Box<content::WorldStorage>),
}

struct PreparedFrame {
    stamp: u64,
    viewport: [u16; 2],
    size: FrameSize,
    overlay: bool,
    content: Option<Weak<RefCell<dyn content::FrameSource>>>,
}

impl PreparedFrame {
    fn matches(&self, stamp: u64, size: [u16; 2], attachments: Attachments<'_>) -> bool {
        self.stamp == stamp
            && self.viewport == size
            && self.overlay == attachments.overlay.is_some()
            && match (&self.content, attachments.content) {
                (None, None) => true,
                (Some(identity), Some(content)) => content.matches(identity),
                _ => false,
            }
    }
}

struct Output {
    storage: Option<Storage>,
    world_output: Option<ViewportOutput>,
    frame: Option<Frame>,
    ready: Option<PreparedFrame>,
    texture_revision: u64,
}

impl Output {
    fn paint(
        &mut self,
        source: draw::DrawSource<'_>,
        camera: Camera,
        resolution: Resolution,
        stamp: u64,
        paint: &mut PaintCtx<'_>,
        attachments: Attachments<'_>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        if !rect.w.is_positive() || !rect.h.is_positive() {
            return Ok(());
        }
        let dimension =
            |v: Fixed| u16::try_from(v.ceil().to_int()).map_err(|_| RenderError::InvalidGeometry);
        let size = [dimension(rect.w)?, dimension(rect.h)?];
        if self
            .ready
            .as_ref()
            .is_none_or(|ready| !ready.matches(stamp, size, attachments))
        {
            let frame_size = resolution.fit(size);
            let pixels @ [width, height] = frame_size.pixels().map(usize::from);
            if width == 0 || height == 0 {
                return Err(RenderError::InvalidGeometry);
            }
            self.ready = None;
            let mut content = attachments
                .content
                .filter(|content| content.available())
                .map(|content| content.source.borrow_mut());
            let world = content
                .as_ref()
                .map(|c| c.camera(frame_size).map(|camera| (camera, c.output())))
                .transpose()?;
            let world_output = world.map(|(_, output)| output);
            let new_frame = self.world_output != world_output
                || self
                    .frame
                    .as_ref()
                    .is_none_or(|f| [f.width, f.height] != pixels);
            if new_frame {
                self.frame = None;
                self.storage = None;
            }
            let frame_camera = match world {
                Some((view, _)) => Some(view.camera),
                None if attachments.overlay.is_some() => {
                    Some(draw::DefaultRenderer::frame_camera(camera, frame_size)?)
                }
                None => None,
            };
            let overlay = attachments
                .overlay
                .map(|layer| {
                    layer.prepare(OverlayFrame {
                        size: pixels,
                        camera: frame_camera,
                    })
                })
                .transpose()?
                .flatten();
            if new_frame {
                let mut frame = match world_output {
                    Some(ViewportOutput::Bounded) => {
                        Frame::with_format(width, height, paint.opaque_output_format(2)?)?
                    }
                    _ => Frame::new(width, height)?,
                };
                frame.texture.alpha_mode = if world_output.is_some() {
                    AlphaMode::Opaque
                } else {
                    AlphaMode::Blend
                };
                self.frame = Some(frame);
                self.world_output = world_output;
            }
            let frame = self.frame.as_mut().expect("viewport frame");
            if let Some((view, policy)) = world {
                if !matches!(&self.storage, Some(Storage::World(storage)) if storage.matches(pixels, view.pivot))
                {
                    self.storage = None;
                    self.storage = Some(Storage::World(Box::new(content::WorldStorage::new(
                        pixels, view.pivot, policy,
                    )?)));
                }
                let Some(Storage::World(storage)) = &mut self.storage else {
                    unreachable!()
                };
                let mut frame = storage.begin(
                    frame_size,
                    view,
                    source,
                    camera.range,
                    overlay.as_deref(),
                    frame,
                );
                content
                    .as_deref_mut()
                    .expect("viewport content")
                    .prepare(&mut frame)?;
            } else {
                let storage = self
                    .storage
                    .get_or_insert_with(|| Storage::Rows(Box::default()));
                let Storage::Rows(renderer) = storage else {
                    unreachable!()
                };
                renderer.render(source, camera, frame_camera, overlay.as_deref(), frame)?;
            }
            self.texture_revision = self.texture_revision.wrapping_add(1);
            frame.texture.cache_revision = self.texture_revision;
            self.ready = Some(PreparedFrame {
                stamp,
                viewport: size,
                size: frame_size,
                overlay: attachments.overlay.is_some(),
                content: attachments.content.map(ViewportContent::identity),
            });
        }
        let (Some(frame), Some(ready)) = (self.frame.as_ref(), self.ready.as_ref()) else {
            return Ok(());
        };
        let frame_size = ready.size;
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
        if let Some(content) = attachments.content.filter(|content| content.available()) {
            content.source.borrow().present(paint, origin, scale);
        }
        Ok(())
    }
}

#[mirui::component]
pub struct StackViewport {
    #[property(ViewportScene)]
    pub scene: Option<StackSceneHandle>,
    #[property(ViewportStage)]
    pub stage: Option<StackStageHandle>,
    #[property(ViewportCamera)]
    pub camera: Camera,
    #[property(ViewportResolution)]
    pub resolution: Resolution,
    bindings: Option<(Option<ModelSubscription>, OwnedEffects)>,
    output: RefCell<Output>,
    stamp: Cell<u64>,
    seen_scene: Cell<u64>,
    tracked: Cell<Option<[Fixed; 3]>>,
}

impl Default for StackViewport {
    fn default() -> Self {
        Self {
            scene: None,
            stage: None,
            camera: Camera::default(),
            resolution: Resolution::Native,
            bindings: None,
            output: RefCell::new(Output {
                storage: None,
                world_output: None,
                frame: None,
                ready: None,
                texture_revision: 0,
            }),
            stamp: Cell::new(0),
            seen_scene: Cell::new(0),
            tracked: Cell::new(None),
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

    fn paint_scene(
        &self,
        scene: &StackScene,
        paint: &mut PaintCtx<'_>,
        attachments: Attachments<'_>,
    ) -> Result<(), RenderError> {
        let revision = scene.revision();
        if self.seen_scene.replace(revision) != revision {
            self.advance_stamp();
        }
        let mut camera = self.camera;
        if let Some(target) = camera.target.filter(|_| attachments.content.is_none()) {
            let position = scene
                .pose(target)
                .map(|p| p.position)
                .or_else(|| self.tracked.get())
                .ok_or(RenderError::InvalidGeometry)?;
            self.tracked.set(Some(position));
            camera.center = core::array::from_fn(|i| position[i] + camera.center[i]);
        }
        self.output.borrow_mut().paint(
            draw::DrawSource::Scene(scene),
            camera,
            self.resolution,
            self.stamp.get(),
            paint,
            attachments,
        )
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
            draw::DrawSource::Stage { stage, world },
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
                view.bindings = Some((None, effects));
            }
            return;
        }
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
            .bindings = Some((Some(sub), effects));
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
        view.stage = None;
        StackViewport::bind(world, entity);
        PropertyChange::Visual
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
        view.tracked.set(None);
        view.advance_stamp();
        view.scene = None;
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
        if camera.target != view.camera.target {
            view.tracked.set(None);
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
        let output = view.output.get_mut();
        output.ready = None;
        output.frame = None;
        output.storage = None;
        output.world_output = None;
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
        if (view.stage.is_some() || view.scene.is_some()) && view.bindings.is_none() {
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
    } else if let Some(scene) = &component.scene {
        scene.read(|scene| component.paint_scene(scene, paint, attachments))
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
