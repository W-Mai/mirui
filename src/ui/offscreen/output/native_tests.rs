use super::*;
use crate::render::texture::TexturePixelsMut;
use crate::render::{
    BackendTarget, DrawCommand, DrawRequest, PlaneLayout, RenderRoute, Renderer, TargetAlpha,
    TargetDescriptor, TargetPlan, TargetUpdate,
};
use crate::test_support::tracked_allocations;
use crate::types::{Color, Fixed, Transform, Viewport};
use alloc::{boxed::Box, rc::Rc};
use core::{any::Any, cell::Cell};

#[derive(Default)]
struct Device {
    lost: Cell<bool>,
    read_failure: Cell<bool>,
    reads: Cell<usize>,
    creates: Cell<usize>,
}

struct Contents {
    color: Cell<Color>,
    completed: Cell<bool>,
}

impl Default for Contents {
    fn default() -> Self {
        Self {
            color: Cell::new(Color::rgba(0, 0, 0, 0)),
            completed: Cell::new(false),
        }
    }
}

struct Target {
    device: Rc<Device>,
    contents: Rc<Contents>,
    descriptor: TargetDescriptor,
}

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        self.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.descriptor.layout.required_bytes()
    }
    fn validate(&self) -> Result<(), RenderError> {
        if self.device.lost.get() {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read_pixels_into(&self, pixels: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        self.validate()?;
        if !self.contents.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        self.device.reads.set(self.device.reads.get() + 1);
        for y in 0..i32::from(pixels.height()) {
            for x in 0..i32::from(pixels.width()) {
                pixels.set_pixel(x, y, &self.contents.color.get());
            }
        }
        if self.device.read_failure.get() {
            return Err(RenderError::BackendFailure);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Fixture {
    device: Rc<Device>,
    active: Option<Rc<Contents>>,
}

impl Renderer for Fixture {
    type TargetState = Option<Rc<Contents>>;
    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        Ok(Some(TargetPlan {
            descriptor: TargetDescriptor {
                layout,
                alpha: TargetAlpha::Straight,
            },
            storage_bytes: layout.required_bytes(),
            workspace_bytes: 0,
            readback_bytes: 64,
        }))
    }
    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        self.device.creates.set(self.device.creates.get() + 1);
        Ok(Box::new(Target {
            device: Rc::clone(&self.device),
            contents: Rc::new(Contents::default()),
            descriptor: plan.descriptor,
        }))
    }
    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        _: Viewport,
        update: TargetUpdate,
        _: &mut [u8],
    ) -> Result<Self::TargetState, RenderError> {
        let target = target
            .as_any()
            .downcast_ref::<Target>()
            .ok_or(RenderError::InvalidTexture)?;
        assert_eq!(update, TargetUpdate::Rebuild);
        target.contents.completed.set(false);
        target.contents.color.set(Color::rgba(0, 0, 0, 0));
        Ok(self.active.replace(Rc::clone(&target.contents)))
    }
    fn restore_target(
        &mut self,
        previous: Self::TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        self.active.take().unwrap().completed.set(completed);
        self.active = previous;
        Ok(())
    }
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        Ok(RenderRoute::Native)
    }
    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let DrawCommand::Fill { color, .. } = request.command else {
            return Err(RenderError::InvalidGeometry);
        };
        self.active.as_ref().unwrap().color.set(*color);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
}

fn plan(renderer: &Fixture) -> TargetPlan {
    renderer
        .plan_target(PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap())
        .unwrap()
        .unwrap()
}

fn draw(
    texture: &mut Texture<'_>,
    renderer: &mut dyn Renderer,
    color: Color,
    fail: bool,
) -> Result<(), RenderError> {
    texture.render_native(
        renderer,
        Viewport::new(4, 4, Fixed::ONE),
        &mut [],
        &mut |renderer| {
            renderer.submit(&DrawRequest::new(
                &DrawCommand::Fill {
                    area: Rect::new(0, 0, 4, 4),
                    transform: Transform::IDENTITY,
                    quad: None,
                    color,
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                Rect::new(0, 0, 4, 4),
            ))?;
            if fail {
                Err(RenderError::InvalidGeometry)
            } else {
                Ok(())
            }
        },
    )
}

#[test]
fn source_pixels_are_lazy_borrowed_budgeted_and_survive_the_renderer() {
    let mut renderer = Fixture::default();
    let device = Rc::downgrade(&renderer.device);
    let budget = MemoryBudget::new(192);
    let plan = plan(&renderer);
    assert!(Texture::native(&mut renderer, plan, &MemoryBudget::new(191)).is_err());
    assert_eq!(renderer.device.creates.get(), 0);
    let mut texture = Texture::native(&mut renderer, plan, &budget).unwrap();
    assert_eq!(texture.cache_size(), 192);
    assert_eq!(budget.available(), 0);
    assert_eq!(budget.allocated(), 64);
    assert!(matches!(texture.pixels(), Err(RenderError::InvalidTexture)));
    let color = Color::rgba(20, 40, 60, 128);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    assert!(texture.pixels().unwrap().is_none());
    assert_eq!(renderer.device.reads.get(), 0);
    assert!(texture.pixels_mut().is_err());
    {
        let first = texture.read_pixels().unwrap();
        let second = texture.pixels().unwrap().unwrap();
        assert_eq!(first.bytes().as_ptr(), second.bytes().as_ptr());
        assert_eq!(first.get_pixel(3, 3), color);
        assert_eq!(budget.allocated(), 128);
        assert_eq!(budget.available(), 0);
        assert_eq!(texture.cache_size(), 192);
    }
    let allocations = tracked_allocations(|| {
        for _ in 0..8 {
            assert_eq!(texture.read_pixels().unwrap().get_pixel(1, 1), color);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(renderer.device.reads.get(), 1);
    let address = texture.read_pixels().unwrap().bytes().as_ptr();
    let updated = Color::rgb(100, 120, 140);
    let allocations = tracked_allocations(|| {
        draw(&mut texture, &mut renderer, updated, false).unwrap();
        assert!(texture.pixels().unwrap().is_none());
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.bytes().as_ptr(), address);
        assert_eq!(pixels.get_pixel(2, 2), updated);
    });
    assert_eq!(allocations, 0);
    drop(renderer);
    assert_eq!(device.upgrade().unwrap().reads.get(), 2);
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), updated);
    drop(texture);
    assert!(device.upgrade().is_none());
    assert_eq!(budget.available(), 192);
    assert_eq!(budget.allocated(), 0);
}

#[test]
fn failed_reads_and_writes_never_expose_partial_or_previous_pixels() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let mut texture = Texture::native(&mut renderer, plan, &MemoryBudget::new(192)).unwrap();
    let color = Color::rgb(1, 2, 3);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    renderer.device.read_failure.set(true);
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::BackendFailure)
    ));
    assert!(texture.pixels().unwrap().is_none());
    renderer.device.read_failure.set(false);
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), color);
    assert_eq!(
        draw(&mut texture, &mut renderer, Color::rgb(40, 50, 60), true),
        Err(RenderError::InvalidGeometry)
    );
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::InvalidTexture)
    ));
    draw(&mut texture, &mut renderer, color, false).unwrap();
    texture.read_pixels().unwrap();
    renderer.device.lost.set(true);
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), color);
    texture.cache_revision += 1;
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::BackendFailure)
    ));
}

#[test]
fn native_output_writes_share_publication_history_and_failed_slot_reuse() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let outputs = Outputs::<WidgetTexture>::with_budget(384);
    let source = super::tests::key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(source.entity, true).unwrap();
    let color = Color::rgb(10, 20, 30);
    {
        let _batch = outputs.begin_batch();
        assert!(
            outputs
                .reserve_texture(source, Some((&mut renderer, plan)))
                .unwrap()
        );
        let mut write = outputs.prepare_texture(source).unwrap();
        assert!(outputs.get(source.entity).is_none());
        draw(write.texture_mut(), &mut renderer, color, false).unwrap();
        write.texture_mut().read_pixels().unwrap();
        assert!(write.publish());
        let read = outputs.get(source.entity).unwrap();
        assert_eq!(read.texture().read_pixels().unwrap().get_pixel(0, 0), color);
        assert_eq!(renderer.device.reads.get(), 1);
    }
    {
        let _batch = outputs.begin_batch();
        let changed = BufferKey {
            generation: 2,
            ..source
        };
        assert!(
            outputs
                .reserve_texture(changed, Some((&mut renderer, plan)))
                .unwrap()
        );
        let mut write = outputs.prepare_texture(changed).unwrap();
        assert!(
            draw(
                write.texture_mut(),
                &mut renderer,
                Color::rgb(70, 80, 90),
                true
            )
            .is_err()
        );
        assert!(!write.publish());
        assert!(outputs.get(source.entity).is_none());
        let previous = outputs.previous(source.entity).unwrap();
        assert_eq!(
            previous.texture().read_pixels().unwrap().get_pixel(0, 0),
            color
        );
        drop(previous);
        let mut write = outputs.prepare_texture(changed).unwrap();
        draw(
            write.texture_mut(),
            &mut renderer,
            Color::rgb(70, 80, 90),
            false,
        )
        .unwrap();
        assert!(write.publish());
    }
    assert_eq!(renderer.device.creates.get(), 2);
    outputs.trim_memory();
    assert_eq!(outputs.storage().budget.available(), 384);
}

#[test]
fn native_sources_use_the_existing_software_blit_and_error_path() {
    use crate::render::backends::sw::SwRenderer;
    use crate::render::command::CompositeMode;
    use crate::types::Point;

    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let mut texture = Texture::native(&mut renderer, plan, &MemoryBudget::new(192)).unwrap();
    let color = Color::rgba(10, 20, 30, 128);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    let mut target = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    target.alpha_mode = crate::render::texture::AlphaMode::Blend;
    let mut software = SwRenderer::new(target);
    let blit = |software: &mut SwRenderer<'_>, texture: &Texture<'_>| {
        software.submit(&DrawRequest::new(
            &DrawCommand::Blit {
                pos: Point::new(0, 0),
                size: Point::new(4, 4),
                transform: Transform::IDENTITY,
                quad: None,
                texture,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            },
            Rect::new(0, 0, 4, 4),
        ))
    };
    renderer.device.read_failure.set(true);
    assert_eq!(
        blit(&mut software, &texture),
        Err(RenderError::BackendFailure)
    );
    assert!(
        software
            .target
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(
        Renderer::flush(&mut software),
        Err(RenderError::BackendFailure)
    );
    let mut software = SwRenderer::new(software.target);
    renderer.device.read_failure.set(false);
    blit(&mut software, &texture).unwrap();
    Renderer::flush(&mut software).unwrap();
    assert_eq!(
        software.target.read_pixels().unwrap().get_pixel(2, 2),
        color
    );
    assert_eq!(renderer.device.reads.get(), 2);
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
#[test]
fn native_upload_keys_retain_identity_without_retaining_the_device() {
    use crate::render::texture::TextureKey;

    let mut renderer = Fixture::default();
    let device = Rc::downgrade(&renderer.device);
    let plan = plan(&renderer);
    let budget = MemoryBudget::new(384);
    let mut first = Texture::native(&mut renderer, plan, &budget).unwrap();
    let second = Texture::native(&mut renderer, plan, &budget).unwrap();
    let key = TextureKey::from(&first);
    assert_ne!(key, TextureKey::from(&second));
    draw(&mut first, &mut renderer, Color::rgb(2, 3, 4), false).unwrap();
    first.read_pixels().unwrap();
    assert_eq!(key, TextureKey::from(&first));
    first.set_output_revision(7);
    assert_ne!(key, TextureKey::from(&first));
    assert!(first.pixels().unwrap().is_some());
    drop(first);
    let replacement = Texture::native(&mut renderer, plan, &budget).unwrap();
    assert_ne!(key, TextureKey::from(&replacement));
    drop((renderer, second, replacement));
    assert!(device.upgrade().is_none());
    assert_eq!(budget.available(), 384);
    drop(key);
}

#[test]
fn unwinding_native_production_invalidates_pixels_and_restores_the_parent() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let budget = MemoryBudget::new(192);
    let mut texture = Texture::native(&mut renderer, plan, &budget).unwrap();
    draw(&mut texture, &mut renderer, Color::rgb(1, 2, 3), false).unwrap();
    texture.read_pixels().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = texture.render_native(
            &mut renderer,
            Viewport::new(4, 4, Fixed::ONE),
            &mut [],
            &mut |_| panic!("draw interrupted"),
        );
    }));
    assert!(result.is_err());
    assert!(renderer.active.is_none());
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::InvalidTexture)
    ));
    draw(&mut texture, &mut renderer, Color::rgb(4, 5, 6), false).unwrap();
    assert_eq!(
        texture.read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(4, 5, 6)
    );
}
