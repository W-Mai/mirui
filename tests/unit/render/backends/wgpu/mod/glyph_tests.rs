use super::*;
use crate::render::font::{FontSurfaceId, GlyphSurface};

static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn one_bit_coverage_uses_crisp_sampling() {
    assert_eq!(
        glyph_sampling(ShaderKind::GlyphCoverage, 1),
        GlyphSampling::Nearest
    );
    assert_eq!(
        glyph_sampling(ShaderKind::GlyphCoverage, 8),
        GlyphSampling::Linear
    );
    assert_eq!(
        glyph_sampling(ShaderKind::GlyphSdf, 1),
        GlyphSampling::Linear
    );
}

#[test]
fn uniform_arena_rollover_starts_after_the_last_slot() {
    assert!(!uniform_arena_full(
        UNIFORM_ARENA_SIZE as u32 - UNIFORM_ALIGN
    ));
    assert!(uniform_arena_full(UNIFORM_ARENA_SIZE as u32));
}

fn render_scalar_field(
    shader: ShaderKind,
    spread: u16,
    projective: Transform3D,
) -> Option<alloc::vec::Vec<u8>> {
    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 16;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mirui-glyph-parity-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
        ..Default::default()
    }))
    .ok()?;
    let atlas = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: Some("mirui-glyph-parity-atlas"),
            size: wgpu::Extent3d {
                width: 4,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[0, 64, 192, 255],
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-glyph-parity-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
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
        label: Some("mirui-glyph-parity-msaa"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: TEST_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let viewport = ViewportUniform {
        size: [WIDTH as f32, HEIGHT as f32],
        _pad: [0.0; 2],
    };
    let glyph = glyph_uniform(Color::rgb(255, 255, 255), u8::MAX, spread, projective);
    let viewport_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-glyph-parity-viewport"),
        contents: bytemuck::bytes_of(&viewport),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut glyph_uniforms = [0u8; UNIFORM_ALIGN as usize];
    glyph_uniforms[..core::mem::size_of::<GlyphUniform>()]
        .copy_from_slice(bytemuck::bytes_of(&glyph));
    let glyph_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-glyph-parity-uniform"),
        contents: &glyph_uniforms,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let instances = [GlyphInstance {
        origin: [0.0, 0.0],
        axis_x: [WIDTH as f32, 0.0],
        axis_y: [0.0, HEIGHT as f32],
        uv_bounds: [0.0, 0.0, 1.0, 1.0],
    }];
    let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-glyph-parity-instance"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mut pipelines = PipelineCache::new(&device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mirui-glyph-parity-bind-group"),
        layout: &pipelines.glyph_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &glyph_buf,
                    offset: 0,
                    size: core::num::NonZeroU64::new(core::mem::size_of::<GlyphUniform>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(
                    &atlas.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&device.create_sampler(
                    &wgpu::SamplerDescriptor {
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    },
                )),
            },
        ],
    });
    let pipeline = pipelines.get_or_build(
        &device,
        PipelineKey {
            shader,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-glyph-parity-encoder"),
    });
    {
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-glyph-parity-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&target_view),
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
        pass.set_bind_group(0, &bind_group, &[0]);
        pass.set_vertex_buffer(0, instance_buf.slice(..));
        pass.draw(0..6, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(
        readback_pixels(
            &device,
            &queue,
            &target,
            PhysicalRect::new(0, 0, WIDTH as u16, HEIGHT as u16).unwrap(),
        )
        .expect("glyph target readback must succeed after rendering"),
    )
}

fn alpha_at(bytes: &[u8], x: usize, y: usize) -> u8 {
    bytes[(y * 32 + x) * 4 + 3]
}

#[test]
fn projected_quad_and_reused_uniforms_stay_bounded() {
    let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    const SIZE: u32 = 64;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mirui-quad-parity-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
        ..Default::default()
    })) else {
        return;
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-quad-parity-target"),
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
        label: Some("mirui-quad-parity-msaa"),
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
    let source = Rect::new(0, 0, 32, 32);
    let quad = [
        Point::new(18, 16),
        Point::new(48, 20),
        Point::new(44, 44),
        Point::new(20, 48),
    ];
    let forward = Transform3D::from_quad(source, &quad).unwrap();
    let mut vertices = [QuadSdfVertex::default(); 4];
    for (index, (x, y)) in [(0.0, 0.0), (32.0, 0.0), (32.0, 32.0), (0.0, 32.0)]
        .into_iter()
        .enumerate()
    {
        let inverse_w =
            1.0 / (forward.m20.to_f32() * x + forward.m21.to_f32() * y + forward.m22.to_f32());
        vertices[index] = QuadSdfVertex {
            pos: [quad[index].x.to_f32(), quad[index].y.to_f32()],
            local_uvw: [x * inverse_w, y * inverse_w, inverse_w],
        };
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-quad-parity-vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = [0u16, 1, 2, 0, 2, 3];
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-quad-parity-indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-quad-parity-viewport"),
        contents: bytemuck::bytes_of(&ViewportUniform {
            size: [SIZE as f32, SIZE as f32],
            _pad: [0.0; 2],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-quad-parity-uniform"),
        contents: bytemuck::bytes_of(&QuadSdfUniform {
            size: [32.0, 32.0],
            _pad0: [0.0; 2],
            color: [1.0, 0.0, 0.0, 1.0],
            radius_stroke: [0.0; 4],
        }),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let mut pipelines = PipelineCache::new(&device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mirui-quad-parity-bind-group"),
        layout: &pipelines.fill_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });
    let pipeline = pipelines.get_or_build(
        &device,
        PipelineKey {
            shader: ShaderKind::QuadSdf,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-quad-parity-encoder"),
    });
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-quad-parity-pass"),
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
        pass.set_bind_group(0, &bind_group, &[0]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_scissor_rect(0, 0, SIZE / 2, SIZE);
        pass.draw_indexed(0..6, 0, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    queue.write_buffer(
        &uniform_buffer,
        0,
        bytemuck::bytes_of(&QuadSdfUniform {
            size: [32.0, 32.0],
            _pad0: [0.0; 2],
            color: [0.0, 0.0, 1.0, 1.0],
            radius_stroke: [0.0; 4],
        }),
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-quad-reused-uniform-encoder"),
    });
    {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-quad-reused-uniform-pass"),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[0]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_scissor_rect(SIZE / 2, 0, SIZE / 2, SIZE);
        pass.draw_indexed(0..6, 0, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    let pixels = readback_pixels(
        &device,
        &queue,
        &target,
        PhysicalRect::new(0, 0, SIZE as u16, SIZE as u16).unwrap(),
    )
    .unwrap();
    let alpha = |x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4 + 3];
    let color = |x: usize, y: usize, channel: usize| pixels[(y * SIZE as usize + x) * 4 + channel];
    assert!(color(24, 32, 0) > 220);
    assert!(color(24, 32, 2) < 32);
    assert!(color(40, 32, 2) > 220);
    assert!(color(40, 32, 0) < 32);
    assert!(alpha(32, 32) > 220);
    assert_eq!(alpha(0, 0), 0);
    assert_eq!(alpha(63, 0), 0);
    assert_eq!(alpha(0, 63), 0);
    assert_eq!(alpha(63, 63), 0);
    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            if alpha(x, y) != 0 {
                assert!(
                    (16..=50).contains(&x) && (14..=50).contains(&y),
                    "({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn glyph_buffer_ranges_are_disjoint_and_capacity_is_bounded() {
    let first = GlyphBufferArena::range(0, 1).unwrap();
    let next = GlyphBufferArena::range(1, 2).unwrap();
    assert_eq!(first.end, next.start);
    assert_eq!(
        next.end - next.start,
        2 * core::mem::size_of::<GlyphInstance>() as u64
    );
    assert_eq!(core::mem::size_of::<GlyphInstance>(), 40);

    let mut arena = GlyphBufferArena::new();
    arena.glyph_cursor = GLYPH_BUFFER_CAPACITY - 1;
    assert!(arena.can_fit(1));
    assert!(!arena.can_fit(2));
    arena.reset();
    assert!(arena.can_fit(GLYPH_BUFFER_CAPACITY));
}

#[test]
fn packed_surface_upload_removes_stride_and_expands_alpha() {
    let bytes = [0b1010_0000, 0, 0b0100_0000, 0];
    let surface = GlyphSurface::new(
        &bytes,
        3,
        2,
        2,
        mirx::image::SampleLayout::A1,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(7),
    )
    .unwrap();
    let mut output = alloc::vec::Vec::new();

    crate::render::font::scalar::unpack_surface(surface, &mut output).unwrap();

    assert_eq!(output, [255, 0, 255, 0, 255, 0]);
}

#[test]
fn glyph_instance_keeps_region_bounds_under_transform() {
    let surface = GlyphSurface::new(
        &[0; 64],
        8,
        8,
        8,
        mirx::image::SampleLayout::A8,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(8),
    )
    .unwrap();
    let region = mirx::image::Region::new(2, 1, 4, 3).unwrap();
    let mut instances = alloc::vec::Vec::new();

    append_glyph_instance(
        &mut instances,
        Rect::new(1, 2, 4, 3),
        region,
        surface,
        &crate::types::Transform::translate(Fixed::from_int(5), Fixed::from_int(7)),
    );

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].origin, [6.0, 9.0]);
    assert_eq!(instances[0].axis_x, [4.0, 0.0]);
    assert_eq!(instances[0].axis_y, [0.0, 3.0]);
    assert_eq!(instances[0].uv_bounds, [0.25, 0.125, 0.75, 0.5]);
}

#[test]
fn glyph_instance_rotates_about_its_pose_origin() {
    let surface = GlyphSurface::new(
        &[0; 64],
        8,
        8,
        8,
        mirx::image::SampleLayout::A8,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(9),
    )
    .unwrap();
    let region = mirx::image::Region::new(0, 0, 8, 8).unwrap();
    let mut instances = alloc::vec::Vec::new();
    let pose = Transform {
        m00: Fixed::ZERO,
        m01: -Fixed::ONE,
        tx: Fixed::from_int(12),
        m10: Fixed::ONE,
        m11: Fixed::ZERO,
        ty: Fixed::from_int(1),
    };

    append_glyph_instance(
        &mut instances,
        Rect::new(0, -7, 8, 8),
        region,
        surface,
        &pose,
    );

    assert_eq!(instances[0].origin, [19.0, 1.0]);
    assert_eq!(instances[0].axis_x, [0.0, 8.0]);
    assert_eq!(instances[0].axis_y, [-8.0, 0.0]);
    assert_eq!(
        [
            instances[0].origin[0] + instances[0].axis_x[0] + instances[0].axis_y[0],
            instances[0].origin[1] + instances[0].axis_x[1] + instances[0].axis_y[1],
        ],
        [11.0, 9.0]
    );
}

#[test]
fn coverage_and_sdf_shaders_preserve_scalar_edges() {
    let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for (shader, spread) in [(ShaderKind::GlyphCoverage, 0), (ShaderKind::GlyphSdf, 4)] {
        let Some(pixels) = render_scalar_field(shader, spread, Transform3D::IDENTITY) else {
            return;
        };
        let samples = [
            alpha_at(&pixels, 2, 8),
            alpha_at(&pixels, 10, 8),
            alpha_at(&pixels, 20, 8),
            alpha_at(&pixels, 29, 8),
        ];
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(samples[0] < 32, "{shader:?} left edge {samples:?}");
        assert!(samples[3] > 223, "{shader:?} right edge {samples:?}");
    }
}

#[test]
fn glyph_shader_applies_projective_transform() {
    let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let source = Rect::new(0, 0, 32, 16);
    let target = [
        Point::new(0, 0),
        Point::new(24, 2),
        Point::new(20, 14),
        Point::new(0, 16),
    ];
    let projective = Transform3D::from_quad(source, &target).unwrap();
    let Some(pixels) = render_scalar_field(ShaderKind::GlyphCoverage, 0, projective) else {
        return;
    };

    assert!(alpha_at(&pixels, 18, 8) > 160);
    assert_eq!(alpha_at(&pixels, 28, 8), 0);
}
