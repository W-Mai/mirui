use super::*;
use crate::render::texture::ColorFormat;
use mirx::scene::{GradientStop, GradientUnits, LinearGradient, RadialGradient, SpreadMode};

static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());
type TestSurface = crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>;

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn transformed_strokes_plan_fallback_before_device_access() {
    let mut surface = crate::surface::framebuf::FramebufSurface::new(
        64,
        64,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut factory = WgpuRendererFactory::new();
    let renderer = WgpuRenderer {
        factory: &mut factory,
        surface: &mut surface,
        viewport: Viewport::new(64, 64, Fixed::ONE),
        frame: None,
        failure: None,
        physical_clip: None,
    };
    let line = DrawCommand::Line {
        p1: Point::new(8, 8),
        p2: Point::new(24, 16),
        transform: Transform::rotate_deg(8.into()),
        color: Color::rgb(20, 30, 40),
        width: 2.into(),
        opa: 200,
    };
    let arc = DrawCommand::Arc {
        center: Point::new(24, 24),
        radius: 8.into(),
        start_angle: 0.into(),
        end_angle: 270.into(),
        transform: Transform::scale(2.into(), Fixed::ONE),
        color: Color::rgb(20, 30, 40),
        width: 2.into(),
        opa: 200,
    };
    for command in [line, arc] {
        for projection in [
            Transform3D::IDENTITY,
            Transform3D::rotate_y_perspective(12.into(), 400.into()),
        ] {
            let request =
                DrawRequest::new(&command, Rect::new(0, 0, 64, 64)).with_projective(projection);
            renderer.factory.target_edit_budget_bytes = None;
            assert_eq!(renderer.route(&request), Err(RenderError::MissingWorkspace));
            renderer.factory.target_edit_budget_bytes = Some(1);
            assert!(matches!(
                renderer.route(&request),
                Err(RenderError::InsufficientWorkspace { .. })
            ));
            renderer.factory.target_edit_budget_bytes = Some(64 * 64 * 4);
            assert!(matches!(
                renderer.route(&request),
                Ok(RenderRoute::ExactFallback(_))
            ));
            assert!(renderer.factory.target_edit_bytes.is_empty());
            assert!(renderer.frame.is_none());
        }
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn target_edit_storage_reuses_capacity_and_preserves_its_limit_after_trim() {
    let mut factory = WgpuRendererFactory::new();
    assert_eq!(
        factory.reserve_target_edit(4),
        Err(RenderError::MissingWorkspace)
    );
    let mut factory = factory.with_target_edit_budget(128);
    assert!(factory.target_edit_bytes.is_empty());
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            assert_eq!(factory.check_target_edit(128), Ok(128));
            assert!(factory.check_target_edit(129).is_err());
        }),
        0
    );
    assert_eq!(factory.target_edit_bytes.capacity(), 0);
    factory.reserve_target_edit(64).unwrap();
    let address = factory.target_edit_bytes.as_ptr();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for size in [16, 64, 32, 0, 64] {
                factory.reserve_target_edit(size).unwrap();
            }
        }),
        0
    );
    assert_eq!(factory.target_edit_bytes.as_ptr(), address);
    assert_eq!(
        factory.reserve_target_edit(129),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 129,
            capacity_bytes: 128,
        })
    );
    assert_eq!(factory.target_edit_bytes.as_ptr(), address);
    factory.reserve_target_edit(128).unwrap();
    let mut factory = factory.with_target_edit_budget(64);
    assert_eq!(factory.target_edit_bytes.capacity(), 0);
    factory.reserve_target_edit(64).unwrap();
    <WgpuRendererFactory as RendererFactory<TestSurface>>::trim_memory(&mut factory);
    assert_eq!(factory.target_edit_bytes.capacity(), 0);
    assert_eq!(factory.target_edit_budget_bytes, Some(64));
    factory.reserve_target_edit(64).unwrap();
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn target_edits_reject_unprepared_capacity_before_accessing_the_device() {
    let mut surface = crate::surface::framebuf::FramebufSurface::new(
        2,
        2,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut factory = WgpuRendererFactory::new().with_target_edit_budget(15);
    let mut renderer = WgpuRenderer {
        factory: &mut factory,
        surface: &mut surface,
        viewport: Viewport::new(2, 2, Fixed::ONE),
        frame: None,
        failure: None,
        physical_clip: None,
    };
    assert_eq!(
        renderer.edit_region(
            FallbackRegion::rgba8(PhysicalRect::from_size(2, 2), PlaneRequirements::CPU).unwrap(),
            &mut |_| panic!("over budget"),
        ),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 16,
            capacity_bytes: 15,
        })
    );
    assert_eq!(renderer.factory.target_edit_bytes.capacity(), 0);
    assert!(renderer.frame.is_none());
}

impl WgpuTarget for crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)> {
    fn state(&self) -> Option<&crate::surface::wgpu_surface::WgpuState> {
        None
    }

    fn state_mut(&mut self) -> Option<&mut crate::surface::wgpu_surface::WgpuState> {
        None
    }

    fn frame_texture(&self) -> Option<&wgpu::SurfaceTexture> {
        None
    }
}

#[test]
fn source_preparation_error_survives_later_draw_submission_and_flush() {
    let mut target = crate::surface::framebuf::FramebufSurface::new(
        2,
        2,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut factory = WgpuRendererFactory::new();
    let mut renderer = WgpuRenderer {
        factory: &mut factory,
        surface: &mut target,
        viewport: Viewport::new(2, 2, Fixed::ONE),
        frame: None,
        failure: None,
        physical_clip: None,
    };
    let source = Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap();
    assert!(renderer.blit_source_view(&source, None).is_none());
    assert_eq!(renderer.failure, Some(RenderError::SurfaceUnavailable));
    let clip = Rect::new(0, 0, 2, 2);
    let command = DrawCommand::Fill {
        area: clip,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(20, 30, 40),
        radius: Fixed::ZERO,
        opa: 255,
    };
    renderer.draw(&command, &clip, None);
    let request = DrawRequest::new(&command, clip);
    assert_eq!(
        renderer.submit_routed(&request, RenderRoute::Native, None),
        Err(RenderError::SurfaceUnavailable)
    );
    assert_eq!(
        Renderer::flush(&mut renderer),
        Err(RenderError::SurfaceUnavailable)
    );
    assert!(renderer.frame.is_none());
}

fn render_radial_path() -> Option<alloc::vec::Vec<u8>> {
    const SIZE: u32 = 64;
    let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mirui-path-gradient-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
        ..Default::default()
    }))
    .ok()?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-path-gradient-target"),
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
        label: Some("mirui-path-gradient-msaa"),
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
    let path = Path::rect((-1).into(), (-1).into(), 2.into(), 2.into());
    let paint = Paint::RadialGradient(RadialGradient {
        center: Point::new(Fixed::HALF, Fixed::HALF).into(),
        radius: Fixed::HALF.into(),
        focal: Point::new(Fixed::HALF, Fixed::HALF).into(),
        focal_radius: Fixed::ZERO.into(),
        stops: alloc::borrow::Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(255, 255, 255),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(0, 0, 0),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let transform = Transform::translate(Fixed::from_int(32), Fixed::from_int(32))
        .compose(&Transform::scale(Fixed::from_int(32), Fixed::from_int(32)));
    let uniform = path_paint_uniform(&path, transform, &paint, 255).ok()?;
    let viewport = ViewportUniform {
        size: [SIZE as f32, SIZE as f32],
        _pad: [0.0; 2],
    };
    let viewport_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-path-gradient-viewport"),
        contents: bytemuck::bytes_of(&viewport),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-path-gradient-uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let vertices = [[0.0f32, 0.0], [64.0, 0.0], [64.0, 64.0], [0.0, 64.0]];
    let indices = [0u32, 1, 2, 0, 2, 3];
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-path-gradient-vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mirui-path-gradient-indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let mut pipelines = PipelineCache::new(&device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mirui-path-gradient-bind-group"),
        layout: &pipelines.path_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size: core::num::NonZeroU64::new(
                        core::mem::size_of::<PathPaintUniform>() as u64
                    ),
                }),
            },
        ],
    });
    let pipeline = pipelines.get_or_build(
        &device,
        PipelineKey {
            shader: ShaderKind::Path,
            format: wgpu::TextureFormat::Rgba8Unorm,
            samples: TEST_SAMPLES,
            composite: CompositeMode::SourceOver,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mirui-path-gradient-pass"),
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
        pass.set_vertex_buffer(0, vertex_buf.slice(..));
        pass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(
        readback_pixels(
            &device,
            &queue,
            &target,
            PhysicalRect::new(0, 0, SIZE as u16, SIZE as u16).unwrap(),
        )
        .expect("path target readback must succeed after rendering"),
    )
}

#[test]
fn radial_path_gradient_varies_across_fragments() {
    let Some(pixels) = render_radial_path() else {
        return;
    };
    let sample = |x: usize, y: usize| pixels[(y * 64 + x) * 4];
    assert!(sample(32, 32) > 240);
    assert!(sample(4, 32) < 64);
}

#[test]
fn frame_target_metadata_follows_owned_attachments() {
    let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        std::eprintln!("SKIP attachment checks: no GPU adapter");
        return;
    };
    let (device, _) =
        pollster::block_on(adapter.request_device(&Default::default())).expect("attachment device");
    for (format, samples, width, height) in [
        (wgpu::TextureFormat::Rgba8Unorm, 1, 20, 12),
        (wgpu::TextureFormat::Bgra8Unorm, 4, 8, 6),
    ] {
        let mut descriptor = wgpu::TextureDescriptor {
            label: Some("mirui-frame-attachment"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        let view = device
            .create_texture(&descriptor)
            .create_view(&Default::default());
        descriptor.sample_count = samples;
        let msaa_view = (samples > 1).then(|| {
            device
                .create_texture(&descriptor)
                .create_view(&Default::default())
        });
        let target = FrameTarget {
            view,
            msaa_view,
            has_committed_pass: true,
            native: false,
        };
        for shader in [
            ShaderKind::Fill,
            ShaderKind::Blit,
            ShaderKind::BlitReplace,
            ShaderKind::BlitQuad,
            ShaderKind::GlyphCoverage,
            ShaderKind::GlyphSdf,
            ShaderKind::QuadSdf,
            ShaderKind::Path,
        ] {
            assert_eq!(
                target.pipeline_key(shader, CompositeMode::SourceOver),
                PipelineKey {
                    shader,
                    format,
                    samples,
                    composite: CompositeMode::SourceOver
                }
            );
        }
        let viewport = Viewport::new(100, 100, Fixed::from_int(2));
        assert_eq!(target.texture().sample_count(), 1);
        assert_eq!(
            target.clip_rect(viewport, Rect::new(1, 1, 100, 100)),
            PhysicalRect::new(2, 2, width as u16 - 2, height as u16 - 2),
        );
        assert_eq!(target.clip_rect(viewport, Rect::new(50, 50, 10, 10)), None);
        assert!(target.has_committed_pass);
    }
    std::eprintln!("attachment device checks completed");
}

#[test]
fn target_edits_replace_pixels() {
    assert_eq!(
        crate::render::backends::wgpu::pipeline::blend_state_for(
            ShaderKind::BlitReplace,
            CompositeMode::SourceOver,
        ),
        wgpu::BlendState::REPLACE
    );
}

#[test]
fn texture_upload_accepts_both_rgb565_orders_and_row_padding() {
    let mut native = Texture::from_ref(&[0x00, 0xf8, 0, 0, 0xe0, 0x07], 1, 2, ColorFormat::RGB565)
        .expect("valid pixel storage");
    native.stride = 4;
    assert_eq!(
        native
            .read_pixels()
            .unwrap()
            .rgba8_upload_pixels()
            .unwrap()
            .as_ref(),
        &[255, 0, 0, 255, 0, 255, 0, 255]
    );

    let swapped = Texture::from_ref(&[0xf8, 0x00], 1, 1, ColorFormat::RGB565Swapped)
        .expect("valid pixel storage");
    assert_eq!(
        swapped
            .read_pixels()
            .unwrap()
            .rgba8_upload_pixels()
            .unwrap()
            .as_ref(),
        &[255, 0, 0, 255]
    );

    assert!(matches!(
        Texture::from_ref(&[0xf8], 1, 1, ColorFormat::RGB565Swapped),
        Err(RenderError::InvalidTexture)
    ));
}

#[test]
fn projected_quads_are_checked_before_gpu_submission() {
    let area = Rect::new(8, 8, 40, 24);
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(65), Fixed::from_int(320));
    let quad = projection.apply_rect(area).expect("visible projected quad");
    assert!(quad_projection_valid(area.w, area.h, &quad));

    let collapsed = [Point::new(8, 8); 4];
    assert!(!quad_projection_valid(area.w, area.h, &collapsed));
    let fill = DrawCommand::Fill {
        area,
        transform: Transform::IDENTITY,
        quad: Some(collapsed),
        color: Color::rgb(20, 30, 40),
        radius: Fixed::ZERO,
        opa: 255,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&fill, area)),
        Err(RenderError::InvalidGeometry)
    );
}

#[test]
fn solid_stroke_with_full_style_has_a_native_route() {
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(16),
        Fixed::from_int(12),
    );
    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let dash = [Fixed::from_int(3), Fixed::from_int(2)];
    let stroke = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::rotate_deg(Fixed::from_int(20)),
        paint: &paint,
        width: Fixed::from_int(2),
        opa: 255,
        line_cap: crate::render::raster::LineCap::Round,
        line_join: crate::render::raster::LineJoin::Bevel,
        miter_limit: Fixed::from_int(4),
        dash: &dash,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(
            &stroke,
            Rect::new(0, 0, 32, 32)
        )),
        Ok(RenderRoute::Native)
    );
}

#[test]
fn shared_stroke_mesh_keeps_round_caps_inside_local_bounds() {
    let mut path = Path::new();
    path.move_to(Point::new(0, 0)).line_to(Point::new(40, 0));
    let mut scratch = StrokeScratch::new();
    let outline = scratch.outline(
        path.commands(),
        None,
        StrokeSpec {
            width: Fixed::from_int(8),
            cap: crate::render::raster::LineCap::Round,
            join: crate::render::raster::LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
            dash_scale: Fixed::ONE,
        },
    );
    let mut tessellator = FillTessellator::new();
    let (vertices, indices) = tessellator.fill(outline, None, FillRule::NonZero).unwrap();
    assert!(!indices.is_empty());
    assert!(
        vertices
            .iter()
            .all(|point| { (-8.0..=48.0).contains(&point.x) && (-8.0..=8.0).contains(&point.y) })
    );
}

#[test]
fn shared_stroke_mesh_keeps_the_center_of_a_closed_path_empty() {
    let path = Path::rect(
        Fixed::from_int(10),
        Fixed::from_int(10),
        Fixed::from_int(30),
        Fixed::from_int(20),
    );
    let mut scratch = StrokeScratch::new();
    let outline = scratch.outline(
        path.commands(),
        None,
        StrokeSpec {
            width: Fixed::from_int(4),
            cap: crate::render::raster::LineCap::Butt,
            join: crate::render::raster::LineJoin::Miter,
            miter_limit: Fixed::from_int(4),
            dash: &[],
            dash_scale: Fixed::ONE,
        },
    );
    let mut tessellator = FillTessellator::new();
    let (vertices, indices) = tessellator.fill(outline, None, FillRule::NonZero).unwrap();
    let covers = |x: f32, y: f32| {
        indices.chunks_exact(3).any(|triangle| {
            let points = [
                vertices[triangle[0] as usize],
                vertices[triangle[1] as usize],
                vertices[triangle[2] as usize],
            ];
            let edge = |a: lyon::math::Point, b: lyon::math::Point| {
                (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
            };
            let signs = [
                edge(points[0], points[1]),
                edge(points[1], points[2]),
                edge(points[2], points[0]),
            ];
            signs.iter().all(|value| *value >= 0.0) || signs.iter().all(|value| *value <= 0.0)
        })
    };
    assert!(covers(10.0, 20.0));
    assert!(!covers(25.0, 20.0));
}

#[test]
fn rejects_commands_that_current_gpu_dispatch_ignores_or_reduces() {
    let clip = Rect::new(0, 0, 32, 32);
    let path = Path::new();
    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let push = DrawCommand::PushClip {
        path: &path,
        transform: Transform::IDENTITY,
        fill_rule: FillRule::EvenOdd,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&push, clip)),
        Err(RenderError::Unsupported(RenderFeature::PathClip))
    );

    let fill = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: FillRule::NonZero,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&fill, clip)),
        Ok(RenderRoute::Native)
    );

    let gradient_path = Path::rect(0.into(), 0.into(), 10.into(), 10.into());
    let gradient = Paint::LinearGradient(LinearGradient {
        start: Point::ZERO.into(),
        end: Point::new(10, 0).into(),
        stops: alloc::borrow::Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(0, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(255, 255, 255),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: Transform::IDENTITY.into(),
    });
    let gradient_fill = DrawCommand::FillPath {
        path: &gradient_path,
        transform: Transform::IDENTITY,
        paint: &gradient,
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&gradient_fill, clip)),
        Ok(RenderRoute::Native)
    );

    let texture = Texture::owned(2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    let blit = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ONE,
        composite: CompositeMode::SourceOver,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&blit, clip)),
        Ok(RenderRoute::Native)
    );
    let projected = DrawRequest::new(&blit, clip).with_projective(
        Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(120)),
    );
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&projected),
        Ok(RenderRoute::Native)
    );

    let rounded_quad = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: Transform::IDENTITY,
        quad: Some([
            Point::new(0, 0),
            Point::new(2, 0),
            Point::new(2, 2),
            Point::new(0, 2),
        ]),
        texture: &texture,
        opa: 255,
        radius: Fixed::ONE,
        composite: CompositeMode::SourceOver,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&rounded_quad, clip)),
        Ok(RenderRoute::Native)
    );

    let difference = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::Difference,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&difference, clip)),
        Err(RenderError::Unsupported(RenderFeature::Composite(
            CompositeMode::Difference
        )))
    );
}

#[test]
fn accepts_native_fill_and_rejects_unrouted_affine_line() {
    let clip = Rect::new(0, 0, 32, 32);
    let fill = DrawCommand::Fill {
        area: clip,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(20, 30, 40),
        radius: Fixed::ZERO,
        opa: 255,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&fill, clip)),
        Ok(RenderRoute::Native)
    );

    let line = DrawCommand::Line {
        p1: Point::ZERO,
        p2: Point::new(10, 10),
        transform: Transform::rotate_deg(Fixed::from_int(20)),
        color: Color::rgb(20, 30, 40),
        width: Fixed::ONE,
        opa: 255,
    };
    assert_eq!(
        WgpuRenderer::<TestSurface>::classify_request(&DrawRequest::new(&line, clip)),
        Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
    );
}
