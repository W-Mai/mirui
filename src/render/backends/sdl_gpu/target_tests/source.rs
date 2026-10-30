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
        .with_texture(texture, CompositeMode::SourceOver, None, |source, alpha| {
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

pub(crate) fn native_source() -> Texture<'static> {
    native_source_sized(4)
}

pub(crate) fn native_source_sized(size: u16) -> Texture<'static> {
    let mut producer = Producer {
        device: device(),
        color: (255, 0, 0, 128),
    };
    let layout = PlaneLayout::packed(size, size, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(plan.storage_bytes + Texture::native_cpu_bytes(layout).unwrap());
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    texture
        .render_native(
            &mut producer,
            Viewport::new(size, size, Fixed::ONE),
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
    let storage = texture
        .backend_target()
        .unwrap()
        .unwrap()
        .as_any()
        .downcast_ref::<Target<sdl2::surface::Surface<'static>>>()
        .unwrap()
        .storage
        .clone();
    #[cfg(not(feature = "sync"))]
    let sampleable = storage.sampleable;
    #[cfg(feature = "sync")]
    let sampleable = texture
        .backend_target()
        .unwrap()
        .unwrap()
        .as_any()
        .downcast_ref::<Target<sdl2::surface::Surface<'static>>>()
        .unwrap()
        .source
        .sampleable;
    let mut same = TextureCache::new(Rc::clone(&source), 256);
    assert!(texture.pixels().unwrap().is_none());
    assert_eq!(
        draw(&mut same, &source, &texture),
        Color::rgba(128, 0, 0, 128)
    );
    assert_eq!(same.inspect().cache_len(), usize::from(!sampleable));
    assert_eq!(texture.pixels().unwrap().is_none(), sampleable);
    // Uploads only consume resident pixels; prepare the foreign/composited source first.
    drop(texture.read_pixels().unwrap());
    same.with_texture(&texture, CompositeMode::Add, None, |_, alpha| {
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
        with_storage(&storage, |storage| {
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
            Ok(())
        })
        .unwrap();
    }
    for composite in [
        CompositeMode::SourceOver,
        CompositeMode::Add,
        CompositeMode::Multiply,
    ] {
        with_storage(&storage, |storage| {
            storage.set_active(true);
            Ok(())
        })
        .unwrap();
        for cache in [&mut same, &mut foreign] {
            assert!(matches!(
                cache.with_texture(&texture, composite, None, |_, _| panic!(
                    "feedback must not draw"
                )),
                Err(RenderError::InvalidTexture)
            ));
        }
        with_storage(&storage, |storage| {
            storage.set_active(false);
            Ok(())
        })
        .unwrap();
    }
    with_storage(&storage, |storage| {
        storage.set_completed(false);
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        foreign.with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ()),
        Err(RenderError::InvalidTexture)
    ));
    with_storage(&storage, |storage| {
        storage.set_completed(true);
        Ok(())
    })
    .unwrap();
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
        same.with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ()),
        Err(RenderError::BackendFailure)
    ));
    texture.cache_revision = 3;
    assert!(matches!(
        foreign.with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ()),
        Err(RenderError::BackendFailure)
    ));
}

#[test]
fn regional_uploads_do_not_publish_missing_coverage() {
    use crate::render::texture::Region;
    use core::task::{Context, Poll, Waker};
    let texture = native_source();
    let destination = device();
    let mut cache = TextureCache::new(Rc::clone(&destination), 256);
    let left = Region::new(0, 0, 2, 4).unwrap();
    let right = Region::new(2, 0, 2, 4).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(lease)) = texture.poll_pixels_in(left, &mut cx) else {
        panic!("native region");
    };
    assert!(texture.pixels().unwrap().is_none());
    cache
        .with_texture(&texture, CompositeMode::SourceOver, Some(&lease), |_, _| ())
        .unwrap();
    drop(lease);
    assert_eq!(
        cache.with_texture(&texture, CompositeMode::SourceOver, None, |_, _| panic!(
            "missing pixels"
        )),
        Err(RenderError::ResourcePending)
    );
    let Poll::Ready(Ok(lease)) = texture.poll_pixels_in(right, &mut cx) else {
        panic!("native region");
    };
    cache
        .with_texture(&texture, CompositeMode::SourceOver, Some(&lease), |_, _| ())
        .unwrap();
    // The previous CPU region has been replaced, but its device pixels remain valid.
    assert!(texture.pixels_in(left).unwrap().is_none());
    assert_eq!(
        tracked_allocations(|| cache
            .with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ())
            .unwrap()),
        0
    );
    assert_eq!(cache.inspect().cache_len(), 1);
    assert_eq!(cache.inspect().cache_stats().insert_count, 1);
    drop(lease);
    cache
        .with_texture(&texture, CompositeMode::SourceOver, None, |uploaded, _| {
            let mut canvas = destination.canvas().unwrap();
            uploaded.set_blend_mode(sdl2::render::BlendMode::None);
            canvas
                .copy(uploaded, None, Some(sdl2::rect::Rect::new(0, 0, 4, 4)))
                .unwrap();
            let rgba = canvas
                .read_pixels(
                    Some(sdl2::rect::Rect::new(0, 0, 4, 4)),
                    sdl2::pixels::PixelFormatEnum::RGBA32,
                )
                .unwrap();
            assert!(rgba.chunks_exact(4).all(|pixel| pixel == [255, 0, 0, 128]));
        })
        .unwrap();
}

#[test]
fn upload_uses_pinned_region_after_residency_index_replacement() {
    use crate::render::texture::Region;
    use core::task::{Context, Poll, Waker};
    let texture = native_source_sized(16);
    let destination = device();
    let mut cache = TextureCache::new(Rc::clone(&destination), 2048);
    let first = Region::new(0, 0, 1, 1).unwrap();
    let second = Region::new(3, 3, 1, 1).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(lease)) = texture.poll_pixels_in(first, &mut cx) else {
        panic!("first region");
    };
    let Poll::Ready(Ok(_other)) = texture.poll_pixels_in(second, &mut cx) else {
        panic!("second region");
    };
    assert!(texture.pixels_in(first).unwrap().is_none());
    cache
        .with_texture(
            &texture,
            CompositeMode::SourceOver,
            Some(&lease),
            |uploaded, _| {
                let mut canvas = destination.canvas().unwrap();
                uploaded.set_blend_mode(sdl2::render::BlendMode::None);
                canvas
                    .copy(
                        uploaded,
                        Some(sdl2::rect::Rect::new(0, 0, 1, 1)),
                        Some(sdl2::rect::Rect::new(0, 0, 1, 1)),
                    )
                    .unwrap();
                let bytes = canvas
                    .read_pixels(
                        Some(sdl2::rect::Rect::new(0, 0, 1, 1)),
                        sdl2::pixels::PixelFormatEnum::RGBA32,
                    )
                    .unwrap();
                assert_eq!(bytes, [255, 0, 0, 128]);
            },
        )
        .unwrap();
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
    with_storage(&target.storage, |storage| {
        let _busy = storage.texture.borrow_mut();
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
        Ok(())
    })
    .unwrap();
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

#[test]
fn native_sampling_is_scoped_and_releases_its_borrow_after_callback_unwind() {
    let owner = device();
    let foreign = device();
    let erased: Box<dyn core::any::Any> = target(&owner);
    let mut target = *erased
        .downcast::<Target<sdl2::surface::Surface<'static>>>()
        .ok()
        .unwrap();
    // Exercise the borrowing contract independently of the software SDL driver's custom blend support.
    #[cfg(not(feature = "sync"))]
    {
        Rc::get_mut(&mut target.storage).unwrap().sampleable = true;
    }
    #[cfg(feature = "sync")]
    {
        let Target { storage, source } = &mut target;
        storage
            .with(|storage| {
                assert!(storage.source.charge.get().is_none());
                let old = &storage.source;
                let state = alloc::sync::Arc::new(SourceState {
                    descriptor: old.descriptor,
                    sampleable: true,
                    device: old.device.clone(),
                    device_identity: old.device_identity,
                    generation: old.generation,
                    completed: core::sync::atomic::AtomicBool::new(false),
                    active: core::sync::atomic::AtomicBool::new(false),
                    bytes: old.bytes,
                    charge: std::sync::OnceLock::new(),
                });
                storage.source = state.clone();
                *source = state;
                Ok(())
            })
            .unwrap();
    }
    with_storage(&target.storage, |storage| {
        storage.set_completed(true);
        Ok(())
    })
    .unwrap();
    for (destination, direct) in [(&owner, false), (&foreign, true)] {
        assert_eq!(
            target.with_sample_texture(destination, direct, |_| panic!("unavailable sample")),
            Ok(None::<()>)
        );
    }
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..16 {
                assert_eq!(
                    target.with_sample_texture(&owner, true, |_| {
                        assert_eq!(
                            target.with_sample_texture(&owner, true, |_| ()),
                            Err(RenderError::InvalidScope)
                        );
                        42
                    }),
                    Ok(Some(42))
                );
            }
        }),
        0
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = target.with_sample_texture(&owner, true, |_| panic!("sample callback"));
        }))
        .is_err()
    );
    assert_eq!(target.with_sample_texture(&owner, true, |_| 7), Ok(Some(7)));
    assert_eq!(owner.check(), Ok(()));
}

#[cfg(feature = "sync")]
#[test]
fn prepared_cpu_guards_remain_transferable_and_readable_after_owner_close() {
    use std::sync::Barrier;
    let device = device();
    let mut producer = Producer {
        device: device.clone(),
        color: (0, 255, 0, 255),
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
    let pixels = texture.read_pixels().unwrap();
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let pixels = pixels;
            barrier.wait();
            barrier.wait();
            assert_eq!(pixels.get_pixel(0, 0), Color::rgb(0, 255, 0));
        });
        barrier.wait();
        device.close_targets().unwrap();
        barrier.wait();
        worker.join().unwrap();
    });
    assert_eq!(
        texture.read_pixels().unwrap().get_pixel(3, 3),
        Color::rgb(0, 255, 0)
    );
    std::thread::spawn(move || {
        assert_eq!(
            texture.read_pixels().unwrap().get_pixel(3, 3),
            Color::rgb(0, 255, 0)
        );
        drop(texture);
    })
    .join()
    .unwrap();
    drop(producer);
    drop(device);
    assert_eq!(budget.allocated(), 0);
}

#[cfg(feature = "sync")]
#[test]
fn closed_sources_support_regional_preparation_and_pinned_upload_after_cache_replacement() {
    use crate::render::{PreparedDraw, SwRenderer, texture::Region};
    use core::task::{Context, Poll, Waker};
    let owner = device();
    let mut producer = Producer {
        device: owner.clone(),
        color: (0, 255, 0, 255),
    };
    let layout = PlaneLayout::packed(16, 16, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(plan.storage_bytes + Texture::native_cpu_bytes(layout).unwrap());
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    texture
        .render_native(
            &mut producer,
            Viewport::new(16, 16, Fixed::ONE),
            &mut [],
            &mut |_| Ok(()),
        )
        .unwrap();
    texture.set_output_revision(1);
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(region)) = texture.poll_pixels_in(Region::new(0, 0, 3, 3).unwrap(), &mut cx)
    else {
        panic!("initial region");
    };
    drop(region);
    let command = crate::render::DrawCommand::Blit {
        texture: &texture,
        pos: crate::types::Point::ZERO,
        size: crate::types::Point::new(16, 16),
        transform: crate::types::Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    let request = DrawRequest::new(&command, crate::types::Rect::new(0, 0, 1, 1));
    let Poll::Ready(Ok(prepared)) =
        PreparedDraw::poll_upload(request, RenderRoute::Native, Fixed::ONE, &mut cx)
    else {
        panic!("pinned upload");
    };
    let Poll::Ready(Ok(_replacement)) =
        texture.poll_pixels_in(Region::new(3, 3, 1, 1).unwrap(), &mut cx)
    else {
        panic!("replacement region");
    };
    assert!(
        texture
            .pixels_in(prepared.texture_region().unwrap().region())
            .unwrap()
            .is_none()
    );
    owner.close_targets().unwrap();
    assert!(texture.backend_target().unwrap().is_none());
    let mut software = SwRenderer::new(Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap());
    let translated = crate::render::DrawCommand::Blit {
        texture: &texture,
        pos: crate::types::Point::new(-3, -3),
        size: crate::types::Point::new(16, 16),
        transform: crate::types::Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    software
        .draw_request(&DrawRequest::new(
            &translated,
            crate::types::Rect::new(0, 0, 1, 1),
        ))
        .unwrap();
    assert_eq!(
        software.target.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(0, 255, 0)
    );
    let destination = device();
    let mut cache = TextureCache::new(destination.clone(), 4096);
    cache
        .with_texture(
            &texture,
            CompositeMode::SourceOver,
            prepared.texture_region(),
            |uploaded, alpha| {
                assert_eq!(alpha, TargetAlpha::Straight);
                let mut canvas = destination.canvas().unwrap();
                canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
                uploaded.set_blend_mode(sdl2::render::BlendMode::None);
                canvas
                    .copy(uploaded, None, Some(sdl2::rect::Rect::new(0, 0, 16, 16)))
                    .unwrap();
                let mut pixel = [0; 4];
                readback::rgba8(
                    &*canvas,
                    PhysicalRect::new(0, 0, 1, 1).unwrap(),
                    &mut pixel,
                    4,
                )
                .unwrap();
                assert_eq!(pixel, [0, 255, 0, 255]);
            },
        )
        .unwrap();
    assert!(matches!(
        PreparedDraw::poll_upload(request, RenderRoute::Native, Fixed::ONE, &mut cx),
        Poll::Ready(Err(RenderError::InvalidTexture))
    ));
}
