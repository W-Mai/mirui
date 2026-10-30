use super::*;
use crate::render::{DrawRequest, RenderRoute, Renderer};

struct SourceProducer {
    context: Arc<WgpuDeviceContext>,
    active: Option<Shared<Storage>>,
    color: wgpu::Color,
}

impl Renderer for SourceProducer {
    type TargetState = Option<Shared<Storage>>;

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        plan(&self.context, 1, layout)
    }
    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        create(&self.context, 1, plan)
    }
    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        can_reuse(&self.context, 1, target, plan)
    }
    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        _: Viewport,
        update: TargetUpdate,
        _: &mut [u8],
    ) -> Result<Self::TargetState, RenderError> {
        assert_eq!(update, TargetUpdate::Rebuild);
        completed(target, self.color);
        let storage = &target.as_any().downcast_ref::<Target>().unwrap().storage;
        assert!(!storage.swap_active(true));
        Ok(self.active.replace(Shared::clone(storage)))
    }
    fn restore_target(
        &mut self,
        parent: Self::TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        let storage = self.active.take().unwrap();
        storage.set_active(false);
        storage.set_completed(completed);
        self.active = parent;
        Ok(())
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

#[cfg(feature = "sync-cache")]
#[test]
fn native_blit_views_keep_the_source_storage_and_readback_charge_after_texture_drop() {
    use super::super::super::texture_pool::{new_pool, source_view};
    use crate::render::budget::MemoryBudget;
    use crate::render::target::TargetStorage;
    let Some(context) = context() else { return };
    let mut producer = SourceProducer {
        context: context.clone(),
        active: None,
        color: wgpu::Color::RED,
    };
    let layout = PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let limit =
        plan.storage_bytes + plan.readback_bytes + Texture::native_cpu_bytes(layout).unwrap();
    let budget = MemoryBudget::new(limit);
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    texture
        .render_native(
            &mut producer,
            Viewport::new(2, 2, Fixed::ONE),
            &mut [],
            &mut |_| Ok(()),
        )
        .unwrap();
    let source = source_view(&mut new_pool(0), 0, &context, &texture, None).unwrap();
    let weak = Shared::downgrade(source.storage.as_ref().unwrap());
    drop(texture);
    assert!(weak.upgrade().is_some());
    assert_eq!(
        budget.allocated(),
        plan.storage_bytes + TargetStorage::OWNER_BYTES
    );
    assert_eq!(
        budget.available(),
        limit - plan.storage_bytes - plan.readback_bytes - TargetStorage::OWNER_BYTES
    );
    drop(source);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.allocated(), 0);
    assert_eq!(budget.available(), limit);
}

#[test]
fn native_texture_sources_share_views_or_bridge_through_the_upload_cache() {
    use super::super::super::texture_pool::{new_pool, source_view};
    use crate::render::budget::MemoryBudget;

    let (Some(source_device), Some(destination)) = (context(), context()) else {
        return;
    };
    let mut producer = SourceProducer {
        context: Arc::clone(&source_device),
        active: None,
        color: wgpu::Color {
            r: 0.25,
            g: 0.0,
            b: 0.0,
            a: 0.5,
        },
    };
    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(
        plan.storage_bytes + plan.readback_bytes + Texture::native_cpu_bytes(layout).unwrap(),
    );
    let mut texture = Texture::native(&mut producer, plan, &budget).unwrap();
    let viewport = Viewport::new(4, 4, Fixed::ONE);
    texture
        .render_native(&mut producer, viewport, &mut [], &mut |_| Ok(()))
        .unwrap();
    texture.set_output_revision(1);
    let storage = Shared::clone(
        &texture
            .backend_target()
            .unwrap()
            .unwrap()
            .as_any()
            .downcast_ref::<Target>()
            .unwrap()
            .storage,
    );
    let mut pool = new_pool(128);
    let source = source_view(&mut pool, 128, &source_device, &texture, None).unwrap();
    assert_eq!(source.view.texture(), storage.frame.texture());
    assert_eq!(source.alpha, TargetAlpha::Premultiplied);
    assert_eq!(pool.len(), 0);
    assert_eq!(
        budget.allocated(),
        plan.storage_bytes + Texture::native_state_bytes()
    );
    assert!(texture.pixels().unwrap().is_none());
    assert!(matches!(
        source_view(&mut pool, 128, &destination, &texture, None),
        Err(RenderError::ResourcePending)
    ));
    drop(texture.read_pixels().unwrap());
    let uploaded = source_view(&mut pool, 128, &destination, &texture, None).unwrap();
    assert!(
        uploaded
            .view
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::COPY_DST)
    );
    assert_eq!(uploaded.alpha, TargetAlpha::Straight);
    assert_eq!(pool.len(), 1);
    assert_eq!(
        sample_color(&source_device, &source),
        Color::rgba(128, 0, 0, 128)
    );
    assert_eq!(
        sample_color(&destination, &uploaded),
        Color::rgba(128, 0, 0, 128)
    );
    assert_eq!(
        texture.pixels().unwrap().unwrap().get_pixel(0, 0),
        Color::rgba(128, 0, 0, 128)
    );
    {
        #[cfg(not(feature = "sync-cache"))]
        let _readback_busy = storage.readback.borrow_mut();
        #[cfg(feature = "sync-cache")]
        let _readback_busy = storage.readback.try_take().unwrap();
        let reused = source_view(&mut pool, 128, &destination, &texture, None).unwrap();
        assert_eq!(reused.view.texture(), uploaded.view.texture());
    }
    storage.set_active(true);
    for device in [&source_device, &destination] {
        assert!(matches!(
            source_view(&mut pool, 128, device, &texture, None),
            Err(RenderError::InvalidTexture)
        ));
    }
    storage.set_active(false);
    storage.set_completed(false);
    assert!(matches!(
        source_view(&mut pool, 128, &destination, &texture, None),
        Err(RenderError::InvalidTexture)
    ));
    storage.set_completed(true);
    producer.color = wgpu::Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    texture
        .render_native(&mut producer, viewport, &mut [], &mut |_| Ok(()))
        .unwrap();
    texture.set_output_revision(2);
    assert!(texture.pixels().unwrap().is_none());
    drop(texture.read_pixels().unwrap());
    let changed = source_view(&mut pool, 128, &destination, &texture, None).unwrap();
    assert_ne!(changed.view.texture(), uploaded.view.texture());
    assert_eq!(
        texture.pixels().unwrap().unwrap().get_pixel(0, 0),
        Color::rgb(0, 255, 0)
    );
    source_device.invalidate();
    assert!(texture.backend_target().unwrap().is_none());
    let retained = source_view(&mut pool, 128, &destination, &texture, None).unwrap();
    assert_eq!(retained.view.texture(), changed.view.texture());
    assert_eq!(sample_color(&destination, &retained), Color::rgb(0, 255, 0));
    texture.cache_revision = 3;
    assert!(matches!(
        source_view(&mut pool, 128, &destination, &texture, None),
        Err(RenderError::BackendFailure)
    ));
}

#[cfg(feature = "sdl-gpu")]
#[test]
fn sdl_and_wgpu_sources_bridge_without_consumer_specific_readback() {
    use super::super::super::texture_pool::{new_pool, source_view};
    use crate::render::backends::sdl_gpu::{check_foreign_source, native_source};
    use crate::render::budget::MemoryBudget;

    let Some(context) = context() else {
        return;
    };
    let source = native_source();
    assert!(source.pixels().unwrap().is_none());
    drop(source.read_pixels().unwrap());
    let mut pool = new_pool(256);
    for _ in 0..2 {
        let uploaded = source_view(&mut pool, 256, &context, &source, None).unwrap();
        assert_eq!(uploaded.alpha, TargetAlpha::Straight);
        assert_eq!(
            sample_color(&context, &uploaded),
            Color::rgba(255, 0, 0, 128)
        );
    }
    assert_eq!(pool.len(), 1);
    assert_eq!(pool.stats().insert_count, 1);
    assert_eq!(
        source.pixels().unwrap().unwrap().get_pixel(0, 0),
        Color::rgba(255, 0, 0, 128)
    );

    let mut producer = SourceProducer {
        context: Arc::clone(&context),
        active: None,
        color: wgpu::Color {
            r: 0.25,
            g: 0.0,
            b: 0.0,
            a: 0.5,
        },
    };
    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let plan = producer.plan_target(layout).unwrap().unwrap();
    let budget = MemoryBudget::new(
        plan.storage_bytes + plan.readback_bytes + Texture::native_cpu_bytes(layout).unwrap(),
    );
    let mut source = Texture::native(&mut producer, plan, &budget).unwrap();
    source
        .render_native(
            &mut producer,
            Viewport::new(4, 4, Fixed::ONE),
            &mut [],
            &mut |_| Ok(()),
        )
        .unwrap();
    source.set_output_revision(1);
    assert!(source.pixels().unwrap().is_none());
    drop(producer);
    drop(context);
    check_foreign_source(&source, Color::rgba(64, 0, 0, 128));
    assert_eq!(
        source.pixels().unwrap().unwrap().get_pixel(0, 0),
        Color::rgba(128, 0, 0, 128)
    );
    std::eprintln!("SDL software renderer / WGPU source bridges passed in both directions");
}

fn sample_color(
    context: &Arc<WgpuDeviceContext>,
    source: &super::super::super::texture_pool::BlitSource,
) -> Color {
    use super::super::super::pipeline::{
        BlitUniform, PipelineCache, PipelineKey, ShaderKind, ViewportUniform,
    };
    use crate::render::command::CompositeMode;
    use wgpu::util::DeviceExt;

    let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
    let plan = plan(context, 1, layout).unwrap().unwrap();
    let target = create(context, 1, &plan).unwrap();
    let storage = &target.as_any().downcast_ref::<Target>().unwrap().storage;
    let device = &context.device;
    let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&ViewportUniform {
            size: [4.0; 2],
            _pad: [0.0; 2],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&BlitUniform {
            dst_pos: [0.0; 2],
            dst_size: [4.0; 2],
            uv: [0.0, 0.0, 1.0, 1.0],
            alpha: [
                1.0,
                0.0,
                if source.alpha == TargetAlpha::Premultiplied {
                    1.0
                } else {
                    0.0
                },
                0.0,
            ],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let sampler = device.create_sampler(&Default::default());
    let mut pipelines = PipelineCache::new(device);
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipelines.blit_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&source.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline = pipelines.get_or_build(
        device,
        PipelineKey {
            shader: ShaderKind::Blit,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: 1,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &storage.frame.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..4, 0..1);
    }
    context.queue.submit(Some(encoder.finish()));
    storage.set_completed(true);
    let mut result = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    target
        .read_pixels_into(
            target.descriptor().region(),
            &mut result.pixels_mut().unwrap(),
        )
        .unwrap();
    result.read_pixels().unwrap().get_pixel(2, 2)
}

#[cfg(feature = "sdl-gpu")]
#[test]
fn partial_uploads_share_storage_and_require_complete_sample_coverage() {
    use super::super::super::texture_pool::{new_pool, source_view};
    use crate::render::texture::Region;
    use core::task::{Context, Poll, Waker};
    let Some(context) = context() else {
        return;
    };
    let source = crate::render::backends::sdl_gpu::native_source();
    let left = Region::new(0, 0, 2, 4).unwrap();
    let right = Region::new(2, 0, 2, 4).unwrap();
    let mut pool = new_pool(256);
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(lease)) = source.poll_pixels_in(left, &mut cx) else {
        panic!("native region");
    };
    let first = source_view(&mut pool, 256, &context, &source, Some(&lease)).unwrap();
    drop(lease);
    assert!(source.pixels().unwrap().is_none());
    assert!(matches!(
        source_view(&mut pool, 256, &context, &source, None),
        Err(RenderError::ResourcePending)
    ));
    let Poll::Ready(Ok(lease)) = source.poll_pixels_in(right, &mut cx) else {
        panic!("native region");
    };
    let second = source_view(&mut pool, 256, &context, &source, Some(&lease)).unwrap();
    assert_eq!(first.view.texture(), second.view.texture());
    drop(lease);
    assert!(source.pixels_in(left).unwrap().is_none());
    let complete = source_view(&mut pool, 256, &context, &source, None).unwrap();
    assert_eq!(complete.view.texture(), first.view.texture());
    assert_eq!(
        sample_color(&context, &complete),
        Color::rgba(255, 0, 0, 128)
    );
    assert_eq!(pool.stats().insert_count, 1);
}

#[cfg(feature = "sdl-gpu")]
#[test]
fn regional_upload_consumes_lease_after_source_cache_replacement() {
    use super::super::super::texture_pool::{new_pool, source_view};
    use crate::render::texture::Region;
    use core::task::{Context, Poll, Waker};
    let Some(context) = context() else {
        return;
    };
    let source = crate::render::backends::sdl_gpu::native_source_sized(16);
    let first = Region::new(0, 0, 2, 2).unwrap();
    let second = Region::new(2, 2, 2, 2).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(lease)) = source.poll_pixels_in(first, &mut cx) else {
        panic!("first region");
    };
    let Poll::Ready(Ok(_other)) = source.poll_pixels_in(second, &mut cx) else {
        panic!("second region");
    };
    assert!(source.pixels_in(first).unwrap().is_none());
    let mut pool = new_pool(2048);
    let uploaded = source_view(&mut pool, 2048, &context, &source, Some(&lease)).unwrap();
    assert_eq!(uploaded.view.texture().width(), 16);
    assert!(matches!(
        source_view(&mut pool, 2048, &context, &source, None),
        Err(RenderError::ResourcePending)
    ));
    assert_eq!(pool.stats().insert_count, 1);
}
