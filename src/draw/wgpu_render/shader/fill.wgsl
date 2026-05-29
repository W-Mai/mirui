// Solid-fill rounded-rect shader. One draw call per rect; 4 vertices
// rendered as a triangle strip; rounded corners produced by an SDF
// mask in the fragment shader.

struct Viewport {
    // Window size in physical pixels.
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct Rect {
    // Top-left in physical pixels.
    pos: vec2<f32>,
    // Width / height in physical pixels.
    size: vec2<f32>,
    // Premultiplied... no, straight RGBA. The fragment shader folds
    // alpha at the end.
    color: vec4<f32>,
    // Corner radius in physical pixels. 0 = square.
    radius: f32,
    _pad: vec3<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> rect: Rect;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Triangle strip:
    //   idx 0: top-left      (0, 0)
    //   idx 1: top-right     (1, 0)
    //   idx 2: bottom-left   (0, 1)
    //   idx 3: bottom-right  (1, 1)
    let lx = f32(idx & 1u);
    let ly = f32((idx >> 1u) & 1u);
    let pixel = rect.pos + rect.size * vec2<f32>(lx, ly);

    // Flip Y: mirui uses screen-space (Y down), NDC is Y up.
    let ndc = vec2<f32>(
        (pixel.x / view.size.x) * 2.0 - 1.0,
        1.0 - (pixel.y / view.size.y) * 2.0,
    );

    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.local = vec2<f32>(lx, ly);
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let half_size = rect.size * 0.5;
    let pixel_local = v.local * rect.size;
    // Vector from rect centre to current pixel.
    let centre = pixel_local - half_size;
    // Clamp the requested radius so it never exceeds half the smaller side.
    let r = min(rect.radius, min(half_size.x, half_size.y));
    // SDF for a rounded rectangle (inigo quilez style):
    //   |centre| - half + r, then collapse outside-corner distance.
    let q = abs(centre) - half_size + vec2<f32>(r);
    let dist = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;

    // 1-pixel-wide anti-alias edge.
    let coverage = clamp(0.5 - dist, 0.0, 1.0);

    return vec4<f32>(rect.color.rgb, rect.color.a * coverage);
}
