//! Render pipelines and bind-group layouts. One pipeline per
//! `(shader_kind, surface_format)` pair, lazily built and cached.

#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};
use hashbrown::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    Fill,
    Blit,
    Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub shader: ShaderKind,
    pub format: wgpu::TextureFormat,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct ViewportUniform {
    pub size: [f32; 2],
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct RectUniform {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// `[radius, 0, 0, 0]` — wrapping in vec4 keeps the layout 48 bytes
    /// to match the WGSL `Rect` struct. A bare `radius: f32` plus
    /// `[f32; 3]` padding is 48 bytes Rust-side but 64 bytes WGSL-side
    /// because `vec3<f32>` aligns to 16.
    pub radius_pad: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct BlitUniform {
    pub dst_pos: [f32; 2],
    pub dst_size: [f32; 2],
    pub uv: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct PathTintUniform {
    pub color: [f32; 4],
}

pub struct PipelineCache {
    pub fill_bgl: wgpu::BindGroupLayout,
    pub blit_bgl: wgpu::BindGroupLayout,
    pub path_bgl: wgpu::BindGroupLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
}

impl PipelineCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-fill-bgl"),
            entries: &[uniform_entry(0), uniform_entry(1)],
        });

        let path_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-path-bgl"),
            entries: &[uniform_entry(0), uniform_entry(1)],
        });

        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-blit-bgl"),
            entries: &[
                uniform_entry(0),
                uniform_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self {
            fill_bgl,
            blit_bgl,
            path_bgl,
            pipelines: HashMap::new(),
        }
    }

    pub fn get_or_build(
        &mut self,
        device: &wgpu::Device,
        key: PipelineKey,
    ) -> &wgpu::RenderPipeline {
        self.pipelines.entry(key).or_insert_with(|| {
            let bgl = match key.shader {
                ShaderKind::Fill => &self.fill_bgl,
                ShaderKind::Blit => &self.blit_bgl,
                ShaderKind::Path => &self.path_bgl,
            };
            build_pipeline(device, bgl, key)
        })
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    key: PipelineKey,
) -> wgpu::RenderPipeline {
    let (label, src) = match key.shader {
        ShaderKind::Fill => ("mirui-fill", include_str!("shader/fill.wgsl")),
        ShaderKind::Blit => ("mirui-blit", include_str!("shader/blit.wgsl")),
        ShaderKind::Path => ("mirui-path", include_str!("shader/path.wgsl")),
    };

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(alloc::borrow::Cow::Borrowed(src)),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mirui-pipeline-layout"),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    });

    let path_vertex_layout = wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }],
    };

    let (vertex_buffers, topology): (&[wgpu::VertexBufferLayout], _) = match key.shader {
        ShaderKind::Fill | ShaderKind::Blit => (&[], wgpu::PrimitiveTopology::TriangleStrip),
        ShaderKind::Path => (
            core::slice::from_ref(&path_vertex_layout),
            wgpu::PrimitiveTopology::TriangleList,
        ),
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: key.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

/// 4× MSAA — best compromise between quality and bandwidth on the
/// integrated GPUs the wgpu backend targets first.
pub const MSAA_SAMPLES: u32 = 4;

#[allow(dead_code)]
const _: () = {
    // Keep Rust uniform layouts in sync with WGSL structs.
    assert!(core::mem::size_of::<ViewportUniform>() == 16);
    assert!(core::mem::size_of::<RectUniform>() == 48);
    assert!(core::mem::size_of::<BlitUniform>() == 32);
    // Must match `PathTint` in shader/path.wgsl.
    assert!(core::mem::size_of::<PathTintUniform>() == 16);
};
