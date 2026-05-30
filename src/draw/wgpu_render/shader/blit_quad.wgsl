// Perspective-correct quad blit. Each vertex carries (u/w, v/w, 1/w);
// linear interpolation of those three quantities is the textbook
// projective interpolation pattern, and the fragment shader recovers
// (u, v) by dividing xy / z.

struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uvw: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    // `linear` opts out of clip-space perspective division; the host
    // already encoded the homography weight into `uvw`.
    @location(0) @interpolate(linear) uvw: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let ndc = vec2<f32>(
        (in.pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (in.pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uvw = in.uvw;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let uv = v.uvw.xy / v.uvw.z;
    return textureSample(src_tex, src_samp, uv);
}
