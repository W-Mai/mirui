use std::any::Any;
use std::rc::Rc;

use mirui::app::RendererFactory;
use mirui::render::texture::TexturePixelsMut;
use mirui::render::{
    BackendTarget, PlaneLayout, TargetAlpha, TargetDescriptor, TargetPlan, TargetUpdate,
};
use mirui::surface::{DisplayInfo, InputEvent, Surface};
use mirui::types::PhysicalRect;
use mirui::types::Viewport;

use super::*;

#[path = "native_targets/updates.rs"]
mod updates;

pub(super) struct Slot {
    pub value: Cell<u8>,
    pub alpha: Cell<u8>,
    pub completed: Cell<bool>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Fault {
    Initialize,
    Flush,
    Restore,
}

struct Target {
    device: Rc<()>,
    slot: Rc<Slot>,
    descriptor: TargetDescriptor,
}

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        self.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        4
    }
    fn validate(&self) -> Result<(), RenderError> {
        Ok(())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read_pixels_into(
        &self,
        region: mirui::render::texture::Region,
        dst: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        self.descriptor().readback_region(region, dst)?;
        if !self.slot.completed.get()
            || dst.width() != 1
            || dst.height() != 1
            || dst.format() != ColorFormat::RGBA8888
        {
            return Err(RenderError::InvalidTexture);
        }
        dst.set_pixel(
            0,
            0,
            &Color::rgba(self.slot.value.get(), 0, 0, self.slot.alpha.get()),
        );
        Ok(())
    }
}

pub(super) fn plan(layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
    if layout != PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap() {
        return Ok(None);
    }
    Ok(Some(TargetPlan {
        descriptor: TargetDescriptor {
            layout,
            alpha: TargetAlpha::Straight,
        },
        storage_bytes: 4,
        workspace_bytes: 0,
        readback_bytes: 0,
    }))
}

pub(super) fn create(
    renderer: &BorrowedDummy<'_>,
    requested: &TargetPlan,
) -> Result<Box<dyn BackendTarget>, RenderError> {
    if plan(requested.descriptor.layout)? != Some(*requested) {
        return Err(RenderError::InvalidTexture);
    }
    Ok(Box::new(Target {
        device: Rc::clone(&renderer.device),
        slot: Rc::new(Slot {
            value: Cell::new(0),
            alpha: Cell::new(0),
            completed: Cell::new(false),
        }),
        descriptor: requested.descriptor,
    }))
}

fn matching_target<'a>(
    renderer: &BorrowedDummy<'_>,
    target: &'a dyn BackendTarget,
    requested: &TargetPlan,
) -> Result<Option<&'a Target>, RenderError> {
    if renderer.poisoned {
        return Err(RenderError::InvalidScope);
    }
    let Some(target) = target.as_any().downcast_ref::<Target>() else {
        return Ok(None);
    };
    if !Rc::ptr_eq(&renderer.device, &target.device)
        || target.descriptor != requested.descriptor
        || plan(requested.descriptor.layout)? != Some(*requested)
    {
        return Ok(None);
    }
    if renderer
        .active
        .as_ref()
        .is_some_and(|s| Rc::ptr_eq(s, &target.slot))
    {
        return Err(RenderError::InvalidScope);
    }
    Ok(Some(target))
}

pub(super) fn can_reuse(
    renderer: &BorrowedDummy<'_>,
    target: &dyn BackendTarget,
    plan: &TargetPlan,
) -> Result<bool, RenderError> {
    Ok(matching_target(renderer, target, plan)?.is_some())
}

pub(super) fn bind(
    renderer: &mut BorrowedDummy<'_>,
    target: &dyn BackendTarget,
    update: TargetUpdate,
) -> Result<Option<Rc<Slot>>, RenderError> {
    let requested = plan(target.descriptor().layout)?.ok_or(RenderError::InvalidTexture)?;
    let target =
        matching_target(renderer, target, &requested)?.ok_or(RenderError::InvalidTexture)?;
    match update {
        TargetUpdate::Rebuild => {
            target.slot.value.set(0);
            target.slot.alpha.set(0);
        }
        TargetUpdate::Incremental if !target.slot.completed.get() => {
            return Err(RenderError::InvalidTexture);
        }
        TargetUpdate::Incremental => {}
    }
    target.slot.completed.set(false);
    if renderer.fault == Some(Fault::Initialize) {
        renderer.fault = None;
        return Err(RenderError::BackendFailure);
    }
    Ok(renderer.active.replace(Rc::clone(&target.slot)))
}

pub(super) fn flush(renderer: &mut BorrowedDummy<'_>) -> Result<(), RenderError> {
    if renderer.poisoned {
        return Err(RenderError::InvalidScope);
    }
    if renderer.active.is_some() && renderer.fault == Some(Fault::Flush) {
        renderer.fault = None;
        return Err(RenderError::BackendFailure);
    }
    Ok(())
}

pub(super) fn restore(
    renderer: &mut BorrowedDummy<'_>,
    saved: Option<Rc<Slot>>,
    completed: bool,
) -> Result<(), RenderError> {
    let failed = renderer.fault == Some(Fault::Restore);
    if failed {
        renderer.fault = None;
        renderer.poisoned = true;
    }
    if let Some(child) = renderer.active.take() {
        child
            .completed
            .set(completed && !failed && !renderer.poisoned);
    }
    renderer.active = saved;
    if renderer.poisoned {
        Err(RenderError::InvalidScope)
    } else {
        Ok(())
    }
}

struct Inner<'a>(&'a Cell<u32>);
impl RenderEngine<BorrowedDummy<'_>> for Inner<'_> {
    fn submit(
        &mut self,
        target: &mut BorrowedDummy<'_>,
        draw: &::mirui::render::PreparedDraw<'_>,
    ) -> Result<(), RenderError> {
        self.0.set(self.0.get() + 1);
        target.submit(draw)
    }
}

compose_backend! {
    struct NativeInner {
        target: BorrowedDummy,
        paths: Inner,
    }
    route { default => target, fill_path => paths }
}

struct Outer<'a>(&'a Cell<u32>);
impl RenderEngine<NativeInner<BorrowedDummy<'_>, Inner<'_>>> for Outer<'_> {
    fn submit(
        &mut self,
        target: &mut NativeInner<BorrowedDummy<'_>, Inner<'_>>,
        draw: &::mirui::render::PreparedDraw<'_>,
    ) -> Result<(), RenderError> {
        self.0.set(self.0.get() + 1);
        target.submit(draw)
    }
}

compose_backend! {
    struct NativeOuter {
        target: NativeInner,
        paths: Outer,
    }
    route { default => target, fill_path => paths }
}

fn draw(renderer: &mut dyn Renderer) -> Result<(), RenderError> {
    let path = Path::rect(0.into(), 0.into(), 1.into(), 1.into());
    let paint = Paint::Color(Color::rgb(1, 0, 0).into());
    let command = mirui::render::DrawCommand::FillPath {
        path: &path,
        paint: &paint,
        transform: Transform::IDENTITY,
        opa: 255,
        fill_rule: mirui::render::raster::FillRule::NonZero,
    };
    let request = DrawRequest::new(&command, Rect::new(0, 0, 1, 1));
    renderer.draw_request(&request)
}

fn read_color(target: &dyn BackendTarget) -> Result<Color, RenderError> {
    let mut bytes = [0; 4];
    let mut texture = Texture::new(&mut bytes[..], 1, 1, ColorFormat::RGBA8888)?;
    target.read_pixels_into(target.descriptor().region(), &mut texture.pixels_mut()?)?;
    Ok(texture.read_pixels()?.get_pixel(0, 0))
}

fn read(target: &dyn BackendTarget) -> Result<u8, RenderError> {
    Ok(read_color(target)?.r)
}

#[test]
fn external_nested_composition_preserves_concrete_borrowed_engines_and_sources() {
    let mut screen = [0; 4];
    let inner_count = Cell::new(0);
    let outer_count = Cell::new(0);
    let mut renderer = NativeOuter::new(
        NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&inner_count)),
        Outer(&outer_count),
    );
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    let plan = renderer
        .plan_target(PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(plan.storage_bytes, 4);
    let mut first = renderer.create_target(&plan).unwrap();
    let mut second = renderer.create_target(&plan).unwrap();
    let mut failed = renderer.create_target(&plan).unwrap();
    assert!(renderer.can_reuse_target(&*first, &plan).unwrap());
    let mut invalid_plan = plan;
    invalid_plan.workspace_bytes += 1;
    assert!(!renderer.can_reuse_target(&*first, &invalid_plan).unwrap());
    assert_eq!(read(&*first), Err(RenderError::InvalidTexture));

    let erased: &mut dyn Renderer = &mut renderer;
    erased
        .with_target(
            &mut *first,
            viewport,
            TargetUpdate::Rebuild,
            &mut [],
            &mut |outer| {
                assert!(outer.can_reuse_target(&*second, &plan)?);
                draw(outer)?;
                outer.with_target(
                    &mut *second,
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut |inner| {
                        draw(inner)?;
                        draw(inner)
                    },
                )?;
                assert_eq!(
                    outer.with_target(
                        &mut *failed,
                        viewport,
                        TargetUpdate::Rebuild,
                        &mut [],
                        &mut |inner| {
                            draw(inner)?;
                            Err(RenderError::InvalidGeometry)
                        }
                    ),
                    Err(RenderError::InvalidGeometry)
                );
                draw(outer)
            },
        )
        .unwrap();
    draw(erased).unwrap();
    assert!(renderer.target.target.active.is_none());
    assert_eq!(inner_count.get(), 6);
    assert_eq!(outer_count.get(), 6);
    assert_eq!(read(&*first), Ok(2));
    assert_eq!(read(&*second), Ok(2));
    assert_eq!(read(&*failed), Err(RenderError::InvalidTexture));
    assert!(renderer.can_reuse_target(&*failed, &plan).unwrap());
    drop(renderer);
    assert_eq!(screen[0], 1);
    assert_eq!(read(&*first), Ok(2));
}

#[test]
fn foreign_target_rejection_keeps_the_composed_parent_usable() {
    let mut screen = [0; 4];
    let count = Cell::new(0);
    let mut renderer = NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&count));
    let mut other_screen = [0; 4];
    let mut other = BorrowedDummy::new(&mut other_screen);
    let plan = renderer
        .plan_target(PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap())
        .unwrap()
        .unwrap();
    let mut target = other.create_target(&plan).unwrap();
    assert!(!renderer.can_reuse_target(&*target, &plan).unwrap());
    assert!(other.can_reuse_target(&*target, &plan).unwrap());
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    other
        .with_target(
            &mut *target,
            viewport,
            TargetUpdate::Rebuild,
            &mut [],
            &mut draw,
        )
        .unwrap();
    for update in [TargetUpdate::Rebuild, TargetUpdate::Incremental] {
        assert_eq!(
            renderer.with_target(&mut *target, viewport, update, &mut [], &mut |_| panic!(
                "foreign target"
            )),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(read(&*target), Ok(1));
    }
    draw(&mut renderer).unwrap();
    assert_eq!(count.get(), 1);
    assert!(renderer.target.active.is_none());
    assert_eq!(screen[0], 1);
}

#[test]
fn composed_target_updates_rebuild_or_preserve_completed_contents() {
    let mut screen = [0; 4];
    let inner_count = Cell::new(0);
    let outer_count = Cell::new(0);
    let mut renderer = NativeOuter::new(
        NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&inner_count)),
        Outer(&outer_count),
    );
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    let plan = renderer
        .plan_target(PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap())
        .unwrap()
        .unwrap();
    let mut target = renderer.create_target(&plan).unwrap();
    assert_eq!(
        renderer.with_target(
            target.as_mut(),
            viewport,
            TargetUpdate::Incremental,
            &mut [],
            &mut |_| panic!("uninitialized target must not run the callback"),
        ),
        Err(RenderError::InvalidTexture),
    );
    for (update, expected) in [
        (TargetUpdate::Rebuild, 1),
        (TargetUpdate::Incremental, 2),
        (TargetUpdate::Rebuild, 1),
    ] {
        renderer
            .with_target(target.as_mut(), viewport, update, &mut [], &mut draw)
            .unwrap();
        assert_eq!(read(target.as_ref()), Ok(expected));
    }
    renderer
        .with_target(
            target.as_mut(),
            viewport,
            TargetUpdate::Rebuild,
            &mut [],
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(read(target.as_ref()), Ok(0));
    assert_eq!(read_color(target.as_ref()), Ok(Color::rgba(0, 0, 0, 0)));
    assert_eq!(inner_count.get(), 3);
    assert_eq!(outer_count.get(), 3);
}

struct NativeSurface {
    device: Rc<()>,
    screen: [u8; 4],
}

impl mirui::core::cache::InspectCaches for NativeSurface {}

impl Surface for NativeSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: 1,
            height: 1,
            scale: Fixed::ONE,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _: PhysicalRect) -> Result<(), RenderError> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
}

#[derive(Default)]
struct NativeFactory {
    inner_count: Cell<u32>,
    outer_count: Cell<u32>,
}

impl mirui::core::cache::InspectCaches for NativeFactory {}

impl RendererFactory<NativeSurface> for NativeFactory {
    type Renderer<'a> = NativeOuter<NativeInner<BorrowedDummy<'a>, Inner<'a>>, Outer<'a>>;

    fn make<'a>(&'a mut self, surface: &'a mut NativeSurface, _: &Viewport) -> Self::Renderer<'a> {
        let target = BorrowedDummy {
            buf: &mut surface.screen,
            fills: Cell::new(0),
            device: Rc::clone(&surface.device),
            active: None,
            fault: None,
            poisoned: false,
        };
        NativeOuter::new(
            NativeInner::new(target, Inner(&self.inner_count)),
            Outer(&self.outer_count),
        )
    }
}

#[mirui_macros::component]
struct OutputMarker;

#[mirui_macros::view(component = OutputMarker)]
fn output_paint(paint: paint!()) {
    let path = Path::rect(0.into(), 0.into(), 1.into(), 1.into());
    paint.draw(&mirui::render::DrawCommand::FillPath {
        path: &path,
        paint: &Paint::Color(Color::rgb(1, 0, 0).into()),
        transform: Transform::IDENTITY,
        opa: 255,
        fill_rule: mirui::render::raster::FillRule::NonZero,
    });
    paint.set_background_handled(true);
}

#[test]
fn automatic_output_production_preserves_both_concrete_engine_routes() {
    let surface = NativeSurface {
        device: Rc::new(()),
        screen: [0; 4],
    };
    let mut app = mirui::app::App::with_factory(surface, NativeFactory::default());
    app.with_default_widgets()
        .with_offscreen_pool_budget(1024)
        .with_widget(output_paint::view());
    let root = app.spawn_root().id();
    app.world.insert(root, OutputMarker);
    app.world.insert(root, mirui::ui::OffscreenRender::new());
    app.render().unwrap();
    assert_eq!(app.factory.inner_count.get(), 1);
    assert_eq!(app.factory.outer_count.get(), 1);
    let outputs = app
        .world
        .resource::<mirui::ui::Outputs<mirui::ui::WidgetTexture>>()
        .unwrap();
    let image = outputs.get(root).unwrap();
    assert!(image.texture().backend_target().unwrap().is_some());
    assert_eq!(
        image.texture().read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(1, 0, 0)
    );
}

#[test]
fn factory_frames_keep_concrete_engine_borrows_and_source_device_lifetime() {
    let mut surface = NativeSurface {
        device: Rc::new(()),
        screen: [0; 4],
    };
    let owner = Rc::downgrade(&surface.device);
    let viewport = surface.viewport();
    let target = {
        let mut factory = NativeFactory::default();
        let mut target = {
            let mut renderer = factory.make(&mut surface, &viewport);
            let plan = renderer
                .plan_target(PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap())
                .unwrap()
                .unwrap();
            let mut target = renderer.create_target(&plan).unwrap();
            renderer
                .with_target(
                    &mut *target,
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut draw,
                )
                .unwrap();
            draw(&mut renderer).unwrap();
            target
        };
        assert_eq!(read(&*target), Ok(1));
        {
            let mut renderer = factory.make(&mut surface, &viewport);
            renderer
                .with_target(
                    &mut *target,
                    viewport,
                    TargetUpdate::Incremental,
                    &mut [],
                    &mut draw,
                )
                .unwrap();
            draw(&mut renderer).unwrap();
        }
        assert_eq!(factory.inner_count.get(), 4);
        assert_eq!(factory.outer_count.get(), 4);
        target
    };
    assert_eq!(surface.screen[0], 2);
    drop(surface);
    assert!(owner.upgrade().is_some());
    assert_eq!(read(&*target), Ok(2));
    drop(target);
    assert!(owner.upgrade().is_none());
}
