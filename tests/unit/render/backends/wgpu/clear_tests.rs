use super::*;
use crate::{render::command::CompositeMode, types::PhysicalRect};
use wgpu::util::DeviceExt;

#[test]
fn clear_replaces_scissored_pixels_with_and_without_multisampling() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        std::eprintln!("SKIP clear checks: no GPU adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut cache = PipelineCache::new(&device);
    let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&ViewportUniform {
            size: [4.0 / 1.5, 3.0 / 1.5],
            _pad: [0.0; 2],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let groups = [
        [200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0, 128.0 / 255.0],
        [0.0; 4],
    ]
    .map(|color| {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&RectUniform {
                size: [4.0 / 1.5, 3.0 / 1.5],
                color,
                ..Default::default()
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cache.fill_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    });
    for samples in [1, 4] {
        let mut descriptor = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 3,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let target = device.create_texture(&descriptor);
        let view = target.create_view(&Default::default());
        descriptor.sample_count = samples;
        descriptor.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let multisampled = (samples > 1).then(|| {
            device
                .create_texture(&descriptor)
                .create_view(&Default::default())
        });
        let pipeline = cache.get_or_build(
            &device,
            PipelineKey {
                shader: ShaderKind::FillReplace,
                format: descriptor.format,
                samples,
                composite: CompositeMode::SourceOver,
            },
        );
        for iteration in 0..4 {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: multisampled.as_ref().unwrap_or(&view),
                        resolve_target: multisampled.as_ref().map(|_| &view),
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: if iteration == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 13.0 / 255.0,
                                    g: 29.0 / 255.0,
                                    b: 47.0 / 255.0,
                                    a: 1.0,
                                })
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &groups[0], &[0]);
                pass.set_scissor_rect(1, 1, 3, 2);
                pass.draw(0..4, 0..1);
                pass.set_bind_group(0, &groups[1], &[0]);
                pass.set_scissor_rect(2, 1, 1, 1);
                pass.draw(0..4, 0..1);
            }
            queue.submit(Some(encoder.finish()));
        }
        let mut readback = super::super::readback::Readback::default();
        let texture = readback
            .read(&device, &queue, &target, PhysicalRect::from_size(4, 3))
            .unwrap();
        let pixels = texture.read_pixels().unwrap();
        for y in 0..3 {
            for x in 0..4 {
                let expected = if x == 2 && y == 1 {
                    [0, 0, 0, 0]
                } else if x >= 1 && y >= 1 {
                    [199, 100, 50, 128]
                } else {
                    [13, 29, 47, 255]
                };
                let offset = (y * 4 + x) * 4;
                assert_eq!(
                    &pixels.bytes()[offset..offset + 4],
                    &expected,
                    "samples={samples}, pixel=({x},{y})"
                );
            }
        }
    }
    std::eprintln!(
        "GPU clear pixel checks completed: {:?}",
        adapter.get_info().backend
    );
}
