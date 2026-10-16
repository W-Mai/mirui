use super::*;
use crate::render::{DrawRequest, RenderRoute, Renderer};

struct SourceProducer {
    context: Arc<WgpuDeviceContext>,
    active: Option<Rc<Storage>>,
    color: wgpu::Color,
}

impl Renderer for SourceProducer {
    type TargetState = Option<Rc<Storage>>;

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        plan(&self.context, 1, layout)
    }
    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        create(&self.context, 1, plan)
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
        assert!(!storage.active.replace(true));
        Ok(self.active.replace(Rc::clone(storage)))
    }
    fn restore_target(
        &mut self,
        parent: Self::TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        let storage = self.active.take().unwrap();
        storage.active.set(false);
        storage.completed.set(completed);
        self.active = parent;
        Ok(())
    }
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Err(RenderError::InvalidScope)
    }
    fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        Err(RenderError::InvalidScope)
    }
    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
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
    let budget =
        MemoryBudget::new(plan.storage_bytes + plan.readback_bytes + layout.required_bytes());
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
            .downcast_ref::<Target>()
            .unwrap()
            .storage,
    );
    let mut pool = new_pool(128);
    let source = source_view(&mut pool, 128, &source_device, &texture).unwrap();
    assert_eq!(source.view.texture(), storage.frame.texture());
    assert_eq!(source.alpha, TargetAlpha::Premultiplied);
    assert_eq!(pool.len(), 0);
    assert_eq!(budget.allocated(), plan.storage_bytes);
    assert!(texture.pixels().unwrap().is_none());
    let uploaded = source_view(&mut pool, 128, &destination, &texture).unwrap();
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
        let _readback_busy = storage.readback.borrow_mut();
        let reused = source_view(&mut pool, 128, &destination, &texture).unwrap();
        assert_eq!(reused.view.texture(), uploaded.view.texture());
    }
    storage.active.set(true);
    for device in [&source_device, &destination] {
        assert!(matches!(
            source_view(&mut pool, 128, device, &texture),
            Err(RenderError::InvalidTexture)
        ));
    }
    storage.active.set(false);
    storage.completed.set(false);
    assert!(matches!(
        source_view(&mut pool, 128, &destination, &texture),
        Err(RenderError::InvalidTexture)
    ));
    storage.completed.set(true);
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
    let changed = source_view(&mut pool, 128, &destination, &texture).unwrap();
    assert_ne!(changed.view.texture(), uploaded.view.texture());
    assert_eq!(
        texture.pixels().unwrap().unwrap().get_pixel(0, 0),
        Color::rgb(0, 255, 0)
    );
    source_device.invalidate();
    assert!(texture.backend_target().unwrap().is_none());
    let retained = source_view(&mut pool, 128, &destination, &texture).unwrap();
    assert_eq!(retained.view.texture(), changed.view.texture());
    assert_eq!(sample_color(&destination, &retained), Color::rgb(0, 255, 0));
    texture.cache_revision = 3;
    assert!(matches!(
        source_view(&mut pool, 128, &destination, &texture),
        Err(RenderError::BackendFailure)
    ));
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
    storage.completed.set(true);
    let mut result = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    target
        .read_pixels_into(&mut result.pixels_mut().unwrap())
        .unwrap();
    result.read_pixels().unwrap().get_pixel(2, 2)
}
