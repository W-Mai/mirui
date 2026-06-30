// Texture blit. Per-rect uniform supplies destination + source UV
// rect; the fragment shader samples the bound texture.

struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct Blit {
    // Destination top-left in physical pixels.
    dst_pos: vec2<f32>,
    // Destination size in physical pixels.
    dst_size: vec2<f32>,
    // Source UV rect [u0, v0, u1, v1].
    uv: vec4<f32>,
    // `.x` is the group opacity factor (Rust-side `alpha.x = opa/255`);
    // `.yzw` reserved.
    alpha: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> blit: Blit;
@group(0) @binding(2) var src_tex: texture_2d<f32>;
@group(0) @binding(3) var src_samp: sampler;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    let lx = f32(idx & 1u);
    let ly = f32((idx >> 1u) & 1u);
    let pixel = blit.dst_pos + blit.dst_size * vec2<f32>(lx, ly);
    let ndc = vec2<f32>(
        (pixel.x / view.size.x) * 2.0 - 1.0,
        1.0 - (pixel.y / view.size.y) * 2.0,
    );

    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = vec2<f32>(
        mix(blit.uv.x, blit.uv.z, lx),
        mix(blit.uv.y, blit.uv.w, ly),
    );
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(src_tex, src_samp, v.uv);
    // Premultiplied output: the host configures the pipeline blend factors
    // assuming `rgb` is already scaled by `a`, so RGB-only composite modes
    // (Source-over / Add / Screen / Multiply / Darken / Lighten) all share
    // this fragment path.
    let a = c.a * blit.alpha.x;
    return vec4<f32>(c.rgb * a, a);
}
