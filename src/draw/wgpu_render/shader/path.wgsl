// Solid-colour path: tessellated mesh (vertex + index buffer) shaded
// by a uniform tint. lyon hands us the geometry; this shader's only
// job is to flip Y and drop the tint colour into every fragment.

struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct PathTint {
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> tint: PathTint;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOut {
    let ndc = vec2<f32>(
        (pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(_v: VertexOut) -> @location(0) vec4<f32> {
    return tint.color;
}
