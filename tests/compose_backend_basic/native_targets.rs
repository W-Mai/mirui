use std::any::Any;
use std::rc::Rc;

use mirui::render::texture::TexturePixelsMut;
use mirui::render::{BackendTarget, PlaneLayout, TargetAlpha, TargetDescriptor, TargetPlan};
use mirui::types::Viewport;

use super::*;

pub(super) struct Slot {
    pub value: Cell<u8>,
    pub completed: Cell<bool>,
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

    fn read_pixels_into(&self, dst: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        if !self.slot.completed.get()
            || dst.width() != 1
            || dst.height() != 1
            || dst.format() != ColorFormat::RGBA8888
        {
            return Err(RenderError::InvalidTexture);
        }
        dst.set_pixel(0, 0, &Color::rgb(self.slot.value.get(), 0, 0));
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
            completed: Cell::new(false),
        }),
        descriptor: requested.descriptor,
    }))
}

pub(super) fn bind(
    renderer: &mut BorrowedDummy<'_>,
    target: &dyn BackendTarget,
) -> Result<Option<Rc<Slot>>, RenderError> {
    let target = target
        .as_any()
        .downcast_ref::<Target>()
        .ok_or(RenderError::InvalidTexture)?;
    if !Rc::ptr_eq(&renderer.device, &target.device)
        || renderer
            .active
            .as_ref()
            .is_some_and(|s| Rc::ptr_eq(s, &target.slot))
    {
        return Err(RenderError::InvalidTexture);
    }
    target.slot.completed.set(false);
    Ok(renderer.active.replace(Rc::clone(&target.slot)))
}

struct Inner<'a>(&'a Cell<u32>);
impl RenderEngine<BorrowedDummy<'_>> for Inner<'_> {
    fn submit(
        &mut self,
        target: &mut BorrowedDummy<'_>,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        self.0.set(self.0.get() + 1);
        target.submit(request)
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
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        self.0.set(self.0.get() + 1);
        let route = target.route(request)?;
        target.submit_with_route(request, route)
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
    let route = renderer.route(&request)?;
    renderer.submit_with_route(&request, route)
}

fn read(target: &dyn BackendTarget) -> Result<u8, RenderError> {
    let mut bytes = [0; 4];
    let mut texture = Texture::new(&mut bytes[..], 1, 1, ColorFormat::RGBA8888)?;
    target.read_pixels_into(&mut texture.pixels_mut()?)?;
    Ok(texture.read_pixels()?.get_pixel(0, 0).r)
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
    assert_eq!(read(&*first), Err(RenderError::InvalidTexture));

    let erased: &mut dyn Renderer = &mut renderer;
    erased
        .with_target(&mut *first, viewport, &mut [], &mut |outer| {
            draw(outer)?;
            outer.with_target(&mut *second, viewport, &mut [], &mut |inner| {
                draw(inner)?;
                draw(inner)
            })?;
            assert_eq!(
                outer.with_target(&mut *failed, viewport, &mut [], &mut |inner| {
                    draw(inner)?;
                    Err(RenderError::InvalidGeometry)
                }),
                Err(RenderError::InvalidGeometry)
            );
            draw(outer)
        })
        .unwrap();
    draw(erased).unwrap();
    assert!(renderer.target.target.active.is_none());
    assert_eq!(inner_count.get(), 6);
    assert_eq!(outer_count.get(), 6);
    assert_eq!(read(&*first), Ok(2));
    assert_eq!(read(&*second), Ok(2));
    assert_eq!(read(&*failed), Err(RenderError::InvalidTexture));
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
    assert_eq!(
        renderer.with_target(
            &mut *target,
            Viewport::new(1, 1, Fixed::ONE),
            &mut [],
            &mut |_| panic!("foreign target")
        ),
        Err(RenderError::InvalidTexture)
    );
    draw(&mut renderer).unwrap();
    assert_eq!(count.get(), 1);
    assert!(renderer.target.active.is_none());
    assert_eq!(screen[0], 1);
}
