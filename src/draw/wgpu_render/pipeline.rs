//! Render pipelines and bind-group layouts. One pipeline per
//! `(shader_kind, surface_format)` pair, lazily built and cached.

#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};
use hashbrown::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    Fill,
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

pub struct PipelineCache {
    // Bind-group layout reused by every pipeline that consumes
    // `(Viewport, Rect)`. Built once.
    pub fill_bgl: wgpu::BindGroupLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
}

impl PipelineCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-fill-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        Self {
            fill_bgl,
            pipelines: HashMap::new(),
        }
    }

    pub fn get_or_build(
        &mut self,
        device: &wgpu::Device,
        key: PipelineKey,
    ) -> &wgpu::RenderPipeline {
        self.pipelines
            .entry(key)
            .or_insert_with(|| build_pipeline(device, &self.fill_bgl, key))
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    fill_bgl: &wgpu::BindGroupLayout,
    key: PipelineKey,
) -> wgpu::RenderPipeline {
    let (label, src) = match key.shader {
        ShaderKind::Fill => ("mirui-fill", include_str!("shader/fill.wgsl")),
    };

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(alloc::borrow::Cow::Borrowed(src)),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mirui-fill-pipeline-layout"),
        bind_group_layouts: &[Some(fill_bgl)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
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
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[allow(dead_code)]
const _: () = {
    // Keep Rust uniform layouts in sync with WGSL structs.
    assert!(core::mem::size_of::<ViewportUniform>() == 16);
    // Must match `Rect` in shader/fill.wgsl. Bumping fields here
    // requires the corresponding WGSL update.
    assert!(core::mem::size_of::<RectUniform>() == 48);
};
