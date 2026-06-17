// Rounded-rect SDF on a 2.5D quad. The vertex stream carries
// `(screen_pos, local_uvw)` where `local_uvw = (lx/w, ly/w, 1/w)` and
// `(lx, ly)` are widget-local pixel coordinates in `[0..size.x, 0..size.y]`.
// Linear interpolation across the screen quad followed by `xy / z` in
// the fragment recovers the perspective-correct local position.
//
// `radius_stroke.y == 0` paints a fill; `> 0` paints a stroke ring of
// that pixel width centred on the rect outline (matching the
// `Canvas::stroke_rect` default that insets by `width / 2`).

struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct QuadSdf {
    size: vec2<f32>,
    _pad0: vec2<f32>,
    color: vec4<f32>,
    radius_stroke: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> quad: QuadSdf;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) local_uvw: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(linear) local_uvw: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let ndc = vec2<f32>(
        (in.pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (in.pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.local_uvw = in.local_uvw;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let local = v.local_uvw.xy / v.local_uvw.z;
    let half_size = quad.size * 0.5;
    let centre = local - half_size;
    let r = min(quad.radius_stroke.x, min(half_size.x, half_size.y));
    let q = abs(centre) - half_size + vec2<f32>(r);
    let dist = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;

    let stroke_w = quad.radius_stroke.y;
    var d: f32;
    if (stroke_w <= 0.0) {
        d = dist;
    } else {
        d = abs(dist + stroke_w * 0.5) - stroke_w * 0.5;
    }

    let coverage = clamp(0.5 - d, 0.0, 1.0);
    return vec4<f32>(quad.color.rgb, quad.color.a * coverage);
}
