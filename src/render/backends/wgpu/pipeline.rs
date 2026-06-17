//! Render pipelines and bind-group layouts. One pipeline per
//! `(shader_kind, surface_format)` pair, lazily built and cached.

#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    Fill,
    Blit,
    BlitQuad,
    QuadSdf,
    Path,
    Label,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct LabelVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

/// `uvw = (u/w, v/w, 1/w)`; fragment recovers `uv = uvw.xy / uvw.z`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct BlitQuadVertex {
    pub pos: [f32; 2],
    pub uvw: [f32; 3],
}

/// `local_uvw = (lx/w, ly/w, 1/w)` where `(lx, ly)` are widget-local
/// pixels in `[0..size.x, 0..size.y]`; fragment recovers
/// `(lx, ly) = local_uvw.xy / local_uvw.z` for SDF evaluation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct QuadSdfVertex {
    pub pos: [f32; 2],
    pub local_uvw: [f32; 3],
}

/// Same 48-byte layout as `RectUniform` so the fill bind group covers both.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct QuadSdfUniform {
    pub size: [f32; 2],
    pub _pad0: [f32; 2],
    pub color: [f32; 4],
    /// `.x` = corner radius, `.y` = stroke width (0 = fill); `.zw` unused.
    pub radius_stroke: [f32; 4],
}

/// `cache_size = 1` so `Count(PIPELINE_CACHE_LIMIT)` admits every entry
/// without ever picking a victim (`ShaderKind` × `TextureFormat` stays
/// well under the cap).
pub struct CachedPipeline(pub wgpu::RenderPipeline);

impl HasSize for CachedPipeline {
    fn cache_size(&self) -> usize {
        1
    }
}

/// Comfortably above `ShaderKind::COUNT × number-of-swapchain-formats`
/// so eviction never triggers; the cache effectively becomes a
/// `(key) -> pipeline` map with `mirui::cache` plumbing for stats.
const PIPELINE_CACHE_LIMIT: usize = 64;

pub struct PipelineCache {
    pub fill_bgl: wgpu::BindGroupLayout,
    pub blit_bgl: wgpu::BindGroupLayout,
    pub blit_quad_bgl: wgpu::BindGroupLayout,
    pub path_bgl: wgpu::BindGroupLayout,
    pub label_bgl: wgpu::BindGroupLayout,
    pipelines: Cache<PipelineKey, CachedPipeline, Lru, HashLookup<PipelineKey>>,
}

impl PipelineCache {
    pub fn new(device: &wgpu::Device) -> Self {
        // binding 0 = viewport (frame-shared, offset 0); binding 1 =
        // per-draw uniform packed into the frame's ring buffer with
        // a dynamic offset.
        let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-fill-bgl"),
            entries: &[uniform_entry(0), dynamic_uniform_entry(1)],
        });

        let path_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-path-bgl"),
            entries: &[uniform_entry(0), dynamic_uniform_entry(1)],
        });

        let texture_entries = [
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
        ];
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-blit-bgl"),
            entries: &texture_entries,
        });
        let label_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-label-bgl"),
            entries: &texture_entries,
        });

        let blit_quad_entries = [
            uniform_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let blit_quad_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirui-blit-quad-bgl"),
            entries: &blit_quad_entries,
        });

        Self {
            fill_bgl,
            blit_bgl,
            blit_quad_bgl,
            path_bgl,
            label_bgl,
            pipelines: Cache::builder()
                .max_size(MaxSize::Count(PIPELINE_CACHE_LIMIT))
                .build(),
        }
    }

    pub fn get_or_build(
        &mut self,
        device: &wgpu::Device,
        key: PipelineKey,
    ) -> wgpu::RenderPipeline {
        // Split-borrow `self` so `entry` and `bgl` borrow disjoint fields.
        let Self {
            fill_bgl,
            blit_bgl,
            blit_quad_bgl,
            path_bgl,
            label_bgl,
            pipelines,
        } = self;
        let bgl = match key.shader {
            ShaderKind::Fill | ShaderKind::QuadSdf => fill_bgl,
            ShaderKind::Blit => blit_bgl,
            ShaderKind::BlitQuad => blit_quad_bgl,
            ShaderKind::Path => path_bgl,
            ShaderKind::Label => label_bgl,
        };
        let handle = pipelines
            .entry(key)
            .or_insert_with(|| CachedPipeline(build_pipeline(device, bgl, key)));
        handle.0.clone()
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

fn dynamic_uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
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
        ShaderKind::BlitQuad => ("mirui-blit-quad", include_str!("shader/blit_quad.wgsl")),
        ShaderKind::QuadSdf => ("mirui-quad-sdf", include_str!("shader/quad_sdf.wgsl")),
        ShaderKind::Path => ("mirui-path", include_str!("shader/path.wgsl")),
        ShaderKind::Label => ("mirui-label", include_str!("shader/label.wgsl")),
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
    let label_vertex_layout = wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    };
    let blit_quad_vertex_layout = wgpu::VertexBufferLayout {
        array_stride: 20,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 8,
                shader_location: 1,
            },
        ],
    };

    let (vertex_buffers, topology): (&[wgpu::VertexBufferLayout], _) = match key.shader {
        ShaderKind::Fill | ShaderKind::Blit => (&[], wgpu::PrimitiveTopology::TriangleStrip),
        ShaderKind::BlitQuad | ShaderKind::QuadSdf => (
            core::slice::from_ref(&blit_quad_vertex_layout),
            wgpu::PrimitiveTopology::TriangleList,
        ),
        ShaderKind::Path => (
            core::slice::from_ref(&path_vertex_layout),
            wgpu::PrimitiveTopology::TriangleList,
        ),
        ShaderKind::Label => (
            core::slice::from_ref(&label_vertex_layout),
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
    // Must match the `LabelVertex` layout in pipeline.rs and the
    // `VertexIn` struct in shader/label.wgsl.
    assert!(core::mem::size_of::<LabelVertex>() == 16);
    // Must match `VertexIn` in shader/blit_quad.wgsl (vec2 + vec3 = 20).
    assert!(core::mem::size_of::<BlitQuadVertex>() == 20);
    assert!(core::mem::size_of::<QuadSdfVertex>() == 20);
    // Must match `QuadSdf` in shader/quad_sdf.wgsl and stay 48 bytes
    // so the fill bind group's binding-1 binding-size covers it.
    assert!(core::mem::size_of::<QuadSdfUniform>() == 48);
};
