struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct Glyph {
    color: vec4<f32>,
    spread_pad: vec4<f32>,
    projective_row_0: vec4<f32>,
    projective_row_1: vec4<f32>,
    projective_row_2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> glyph: Glyph;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;

struct VertexIn {
    @location(0) origin: vec2<f32>,
    @location(1) axis_x: vec2<f32>,
    @location(2) axis_y: vec2<f32>,
    @location(3) uv_bounds: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_bounds: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn, @builtin(vertex_index) vertex: u32) -> VertexOut {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vertex];
    let position = in.origin + in.axis_x * corner.x + in.axis_y * corner.y;
    let local = vec3<f32>(position, 1.0);
    let projected = vec3<f32>(
        dot(glyph.projective_row_0.xyz, local),
        dot(glyph.projective_row_1.xyz, local),
        dot(glyph.projective_row_2.xyz, local),
    );
    let ndc = vec2<f32>(
        (projected.x / view.size.x) * 2.0 - projected.z,
        projected.z - (projected.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, projected.z);
    out.uv = mix(in.uv_bounds.xy, in.uv_bounds.zw, corner);
    out.uv_bounds = in.uv_bounds;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let extent = vec2<f32>(textureDimensions(atlas));
    let half_texel = vec2<f32>(0.5) / extent;
    let uv = clamp(in.uv, in.uv_bounds.xy + half_texel, in.uv_bounds.zw - half_texel);
    let encoded = textureSample(atlas, atlas_sampler, uv).r;
    let distance = (encoded * 2.0 - 1.0) * glyph.spread_pad.x;
    let edge_half = max(length(vec2<f32>(dpdx(distance), dpdy(distance))) * 0.5, 1.0 / 255.0);
    let coverage = clamp((distance + edge_half) / (edge_half * 2.0), 0.0, 1.0);
    return vec4<f32>(glyph.color.rgb, glyph.color.a * coverage);
}
