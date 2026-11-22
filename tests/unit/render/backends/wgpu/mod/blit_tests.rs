use super::*;

#[test]
fn blit_paths_mask_corners_and_premultiply_alpha() {
    for premultiplied in [false, true] {
        for opacity in [1.0, 0.5] {
            check_blit_alpha(premultiplied, opacity);
        }
    }
}

fn check_blit_alpha(premultiplied: bool, opacity: f32) {
    const SIZE: u32 = 32;
    let source_encoding = if premultiplied { 1.0 } else { 0.0 };
    let expected_alpha = (128.0 * opacity) as i16;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) else {
        std::eprintln!("SKIP blit alpha checks: no GPU adapter");
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mirui-rounded-blit-test-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
        ..Default::default()
    })) else {
        std::eprintln!("SKIP blit alpha checks: device request failed");
        return;
    };
    let source = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: Some("mirui-rounded-blit-test-source"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[if premultiplied { 128 } else { 255 }, 0, 0, 128],
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-rounded-blit-test-target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let msaa = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-rounded-blit-test-msaa"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: TEST_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-rounded-blit-test-viewport"),
        contents: bytemuck::bytes_of(&ViewportUniform {
            size: [SIZE as f32, SIZE as f32],
            _pad: [0.0; 2],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-rounded-blit-test-uniform"),
        contents: bytemuck::bytes_of(&BlitUniform {
            dst_pos: [0.0, 0.0],
            dst_size: [SIZE as f32, SIZE as f32],
            uv: [0.0, 0.0, 1.0, 1.0],
            alpha: [opacity, 10.0, source_encoding, 0.0],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let mut cache = PipelineCache::new(&device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mirui-rounded-blit-test-bind-group"),
        layout: &cache.blit_bgl,
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
                resource: wgpu::BindingResource::TextureView(
                    &source.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline = cache.get_or_build(
        &device,
        PipelineKey {
            shader: ShaderKind::Blit,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-rounded-blit-test-encoder"),
    });
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-rounded-blit-test-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&view),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = readback_pixels(
        &device,
        &queue,
        &target,
        PhysicalRect::new(0, 0, SIZE as u16, SIZE as u16).unwrap(),
    )
    .expect("rounded blit target could not be read");
    let alpha = |x: usize, y: usize| bytes[(y * SIZE as usize + x) * 4 + 3];
    assert_eq!(alpha(0, 0), 0);
    for (x, y) in [(16, 0), (0, 16), (16, 16)] {
        assert!((i16::from(alpha(x, y)) - expected_alpha).abs() <= 1);
        assert!((i16::from(bytes[(y * SIZE as usize + x) * 4]) - 255).abs() <= 1);
    }

    let replace_pipeline = cache.get_or_build(
        &device,
        PipelineKey {
            shader: ShaderKind::BlitReplace,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut replace_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-target-replace-test-encoder"),
    });
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = replace_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-target-replace-test-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&view),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&replace_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
    queue.submit(Some(replace_encoder.finish()));
    let replaced = readback_pixels(
        &device,
        &queue,
        &target,
        PhysicalRect::new(16, 16, 1, 1).unwrap(),
    )
    .expect("replacement pixel should be readable");
    assert!((i16::from(replaced[3]) - expected_alpha).abs() <= 1);

    let vertices = [
        BlitQuadVertex {
            pos: [0.0, 0.0],
            uvw: [0.0, 0.0, 1.0],
            params: [opacity, 10.0, 32.0, 32.0],
            premultiplied: source_encoding,
        },
        BlitQuadVertex {
            pos: [32.0, 0.0],
            uvw: [0.5, 0.0, 0.5],
            params: [opacity, 10.0, 32.0, 32.0],
            premultiplied: source_encoding,
        },
        BlitQuadVertex {
            pos: [32.0, 32.0],
            uvw: [0.5, 0.5, 0.5],
            params: [opacity, 10.0, 32.0, 32.0],
            premultiplied: source_encoding,
        },
        BlitQuadVertex {
            pos: [0.0, 32.0],
            uvw: [0.0, 1.0, 1.0],
            params: [opacity, 10.0, 32.0, 32.0],
            premultiplied: source_encoding,
        },
    ];
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-translucent-quad-test-vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = [0u16, 1, 2, 0, 2, 3];
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-translucent-quad-test-indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let quad_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mirui-translucent-quad-test-bind-group"),
        layout: &cache.blit_quad_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &source.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let quad_pipeline = cache.get_or_build(
        &device,
        PipelineKey {
            shader: ShaderKind::BlitQuad,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-translucent-quad-test-encoder"),
    });
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-translucent-quad-test-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&view),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&quad_pipeline);
        pass.set_bind_group(0, &quad_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = readback_pixels(
        &device,
        &queue,
        &target,
        PhysicalRect::new(0, 0, SIZE as u16, SIZE as u16).unwrap(),
    )
    .expect("translucent quad target could not be read");
    let center = &bytes[(16 * SIZE as usize + 16) * 4..][..4];
    assert_eq!(bytes[3], 0);
    assert!((i16::from(center[0]) - 255).abs() <= 1);
    assert!((i16::from(center[3]) - expected_alpha).abs() <= 1);
}
