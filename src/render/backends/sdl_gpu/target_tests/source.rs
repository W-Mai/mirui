use super::super::super::texture_cache::TextureCache;
use super::*;
use crate::render::{
    DrawRequest, RenderRoute, Renderer, budget::MemoryBudget, command::CompositeMode,
};
use crate::types::PhysicalRect;
use crate::types::{Color, Fixed};

struct Producer {
    device: Rc<TestDevice>,
    color: (u8, u8, u8, u8),
}

impl Renderer for Producer {
    type TargetState = Binding<sdl2::surface::Surface<'static>>;

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        plan(&self.device, layout)
    }
    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        create(&self.device, plan)
    }
    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        can_reuse(&self.device, target, plan)
    }
    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        _: Viewport,
        update: TargetUpdate,
        _: &mut [u8],
    ) -> Result<Self::TargetState, RenderError> {
        let binding = Binding::begin(&self.device, target, update)?;
        fill(&self.device, sdl2::rect::Rect::new(0, 0, 4, 4), self.color);
        Ok(binding)
    }
    fn restore_target(
        &mut self,
        binding: Self::TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        binding.finish(&self.device, completed)
    }
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Err(RenderError::InvalidScope)
    }
    fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        Err(RenderError::InvalidScope)
    }
    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
}

fn draw(
    cache: &mut TextureCache<sdl2::surface::Surface<'static>>,
    destination: &Rc<TestDevice>,
    texture: &Texture<'_>,
) -> Color {
    let _pixels = if cache
        .can_sample(texture, CompositeMode::SourceOver)
        .unwrap()
    {
        None
    } else {
        Some(texture.read_pixels().unwrap())
    };
    cache
        .with_texture(texture, CompositeMode::SourceOver, |source, alpha| {
            let mut canvas = destination.canvas().unwrap();
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 0));
            canvas.clear();
            if alpha == TargetAlpha::Straight {
                source.set_blend_mode(sdl2::render::BlendMode::Blend);
            }
            source.set_alpha_mod(255);
            source.set_color_mod(255, 255, 255);
            canvas
                .copy(source, None, Some(sdl2::rect::Rect::new(0, 0, 4, 4)))
                .unwrap();
            let mut pixel = [0; 4];
            readback::rgba8(
                &*canvas,
                PhysicalRect::new(1, 1, 1, 1).unwrap(),
                &mut pixel,
                4,
            )
            .unwrap();
            Color::rgba(pixel[0], pixel[1], pixel[2], pixel[3])
        })
        .unwrap()
}

#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub(crate) fn native_source() -> Texture<'static> {
    let mut producer = Producer {
        device: device(),
        color: (255, 0, 0, 128),
    };
    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(plan.storage_bytes + Texture::native_cpu_bytes(layout).unwrap());
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    texture
        .render_native(
            &mut producer,
            Viewport::new(4, 4, Fixed::ONE),
            &mut [],
            &mut |_| Ok(()),
        )
        .unwrap();
    texture.set_output_revision(1);
    texture
}

#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub(crate) fn check_foreign_source(texture: &Texture<'_>, expected: Color) {
    let device = device();
    let mut cache = TextureCache::new(Rc::clone(&device), 256);
    for _ in 0..2 {
        assert_eq!(draw(&mut cache, &device, texture), expected);
    }
    assert_eq!(cache.inspect().cache_len(), 1);
    assert_eq!(cache.inspect().cache_stats().insert_count, 1);
}

#[test]
fn native_sources_bridge_once_and_reject_feedback_before_cached_uploads() {
    let source = device();
    let destination = device();
    let mut producer = Producer {
        device: Rc::clone(&source),
        color: (255, 0, 0, 128),
    };
    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(plan.storage_bytes + Texture::native_cpu_bytes(layout).unwrap());
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    let viewport = Viewport::new(4, 4, Fixed::ONE);
    texture
        .render_native(&mut producer, viewport, &mut [], &mut |_| Ok(()))
        .unwrap();
    texture.set_output_revision(1);
    let storage = Rc::clone(
        &texture
            .backend_target()
            .unwrap()
            .unwrap()
            .as_any()
            .downcast_ref::<Target<sdl2::surface::Surface<'static>>>()
            .unwrap()
            .storage,
    );
    let mut same = TextureCache::new(Rc::clone(&source), 256);
    assert!(texture.pixels().unwrap().is_none());
    assert_eq!(
        draw(&mut same, &source, &texture),
        Color::rgba(128, 0, 0, 128)
    );
    assert_eq!(same.inspect().cache_len(), usize::from(!storage.sampleable));
    assert_eq!(texture.pixels().unwrap().is_none(), storage.sampleable);
    // Uploads only consume resident pixels; prepare the foreign/composited source first.
    drop(texture.read_pixels().unwrap());
    same.with_texture(&texture, CompositeMode::Add, |_, alpha| {
        assert_eq!(alpha, TargetAlpha::Straight);
    })
    .unwrap();
    assert_eq!(same.inspect().cache_len(), 1);
    let mut foreign = TextureCache::new(Rc::clone(&destination), 256);
    assert_eq!(
        draw(&mut foreign, &destination, &texture),
        Color::rgba(128, 0, 0, 128)
    );
    assert_eq!(foreign.inspect().cache_len(), 1);
    {
        let _source_busy = source.canvas().unwrap();
        let _texture_busy = storage.texture.borrow_mut();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..4 {
                    assert_eq!(
                        draw(&mut foreign, &destination, &texture),
                        Color::rgba(128, 0, 0, 128)
                    );
                }
            }),
            0
        );
    }
    for composite in [
        CompositeMode::SourceOver,
        CompositeMode::Add,
        CompositeMode::Multiply,
    ] {
        storage.active.set(true);
        for cache in [&mut same, &mut foreign] {
            assert!(matches!(
                cache.with_texture(&texture, composite, |_, _| panic!("feedback must not draw")),
                Err(RenderError::InvalidTexture)
            ));
        }
        storage.active.set(false);
    }
    storage.completed.set(false);
    assert!(matches!(
        foreign.with_texture(&texture, CompositeMode::SourceOver, |_, _| ()),
        Err(RenderError::InvalidTexture)
    ));
    storage.completed.set(true);
    producer.color = (0, 255, 0, 255);
    texture
        .render_native(&mut producer, viewport, &mut [], &mut |_| Ok(()))
        .unwrap();
    texture.set_output_revision(2);
    assert!(texture.pixels().unwrap().is_none());
    assert_eq!(
        draw(&mut foreign, &destination, &texture),
        Color::rgb(0, 255, 0)
    );
    assert_eq!(foreign.inspect().cache_stats().insert_count, 2);
    source.poison();
    assert!(texture.backend_target().unwrap().is_none());
    assert_eq!(
        draw(&mut foreign, &destination, &texture),
        Color::rgb(0, 255, 0)
    );
    assert!(matches!(
        same.with_texture(&texture, CompositeMode::SourceOver, |_, _| ()),
        Err(RenderError::BackendFailure)
    ));
    texture.cache_revision = 3;
    assert!(matches!(
        foreign.with_texture(&texture, CompositeMode::SourceOver, |_, _| ()),
        Err(RenderError::BackendFailure)
    ));
}

#[test]
fn source_texture_borrows_reject_reentry_without_poisoning_or_allocating() {
    let owner = device();
    let source = target(&owner);
    let binding = Binding::begin(&owner, &*source, TargetUpdate::Rebuild).unwrap();
    binding.finish(&owner, true).unwrap();
    let target = source
        .as_any()
        .downcast_ref::<Target<sdl2::surface::Surface<'static>>>()
        .unwrap();
    let mut destination = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    {
        let _busy = target.storage.texture.borrow_mut();
        assert_eq!(
            source.read_pixels_into(
                source.descriptor().region(),
                &mut destination.pixels_mut().unwrap()
            ),
            Err(RenderError::InvalidScope)
        );
        assert!(matches!(
            Binding::begin(&owner, &*source, TargetUpdate::Rebuild),
            Err(RenderError::InvalidScope)
        ));
    }
    assert_eq!(
        tracked_allocations(|| {
            let binding = Binding::begin(&owner, &*source, TargetUpdate::Incremental).unwrap();
            binding.finish(&owner, true).unwrap();
            source
                .read_pixels_into(
                    source.descriptor().region(),
                    &mut destination.pixels_mut().unwrap(),
                )
                .unwrap();
        }),
        0
    );
    assert!(owner.check().is_ok());
}
