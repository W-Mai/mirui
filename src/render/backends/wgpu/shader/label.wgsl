// Label rendering: per-quad vertex buffer (4 verts × N glyphs), each
// vertex carries pos + atlas UV. Fragment samples the R8 atlas and
// uses the alpha as a coverage mask over the uniform tint colour.

struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct LabelTint {
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> tint: LabelTint;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_samp: sampler;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let ndc = vec2<f32>(
        (v.pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (v.pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(atlas, atlas_samp, v.uv).r;
    return vec4<f32>(tint.color.rgb, tint.color.a * mask);
}
