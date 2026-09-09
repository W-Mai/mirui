struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct Glyph {
    color: vec4<f32>,
    spread_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> glyph: Glyph;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) uv_bounds: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_bounds: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let ndc = vec2<f32>(
        (in.pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (in.pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.uv_bounds = in.uv_bounds;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let extent = vec2<f32>(textureDimensions(atlas));
    let half_texel = vec2<f32>(0.5) / extent;
    let uv = clamp(in.uv, in.uv_bounds.xy + half_texel, in.uv_bounds.zw - half_texel);
    let coverage = textureSample(atlas, atlas_sampler, uv).r;
    return vec4<f32>(glyph.color.rgb, glyph.color.a * coverage);
}
