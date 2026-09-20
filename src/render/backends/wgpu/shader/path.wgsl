struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

struct PathPaint {
    kind_spread_count: vec4<u32>,
    color: vec4<f32>,
    inverse_row_0: vec4<f32>,
    inverse_row_1: vec4<f32>,
    geometry_0: vec4<f32>,
    geometry_1: vec4<f32>,
    stop_offsets_0: vec4<f32>,
    stop_offsets_1: vec4<f32>,
    stop_colors: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> view: Viewport;
@group(0) @binding(1) var<uniform> paint: PathPaint;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) logical_pos: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOut {
    let ndc = vec2<f32>(
        (pos.x / view.size.x) * 2.0 - 1.0,
        1.0 - (pos.y / view.size.y) * 2.0,
    );
    var out: VertexOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.logical_pos = pos;
    return out;
}

fn stop_offset(index: u32) -> f32 {
    if index < 4u {
        return paint.stop_offsets_0[index];
    }
    return paint.stop_offsets_1[index - 4u];
}

fn apply_spread(value: f32) -> f32 {
    switch paint.kind_spread_count.y {
        case 1u: {
            return value - floor(value);
        }
        case 2u: {
            let period = floor(value);
            let fraction = value - period;
            if (i32(period) & 1) == 0 {
                return fraction;
            }
            return 1.0 - fraction;
        }
        default: {
            return value;
        }
    }
}

fn sample_stops(value: f32) -> vec4<f32> {
    let count = paint.kind_spread_count.z;
    if count == 0u {
        return vec4<f32>(0.0);
    }
    let t = apply_spread(value);
    let first_offset = stop_offset(0u);
    if t < first_offset {
        return paint.stop_colors[0];
    }
    for (var index = 1u; index < 8u; index = index + 1u) {
        if index >= count {
            break;
        }
        let end_offset = stop_offset(index);
        if t < end_offset {
            let start_offset = stop_offset(index - 1u);
            let interval = end_offset - start_offset;
            if interval == 0.0 {
                return paint.stop_colors[index];
            }
            let amount = (t - start_offset) / interval;
            return mix(paint.stop_colors[index - 1u], paint.stop_colors[index], amount);
        }
    }
    return paint.stop_colors[count - 1u];
}

fn paint_point(logical_pos: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        dot(paint.inverse_row_0.xyz, vec3<f32>(logical_pos, 1.0)),
        dot(paint.inverse_row_1.xyz, vec3<f32>(logical_pos, 1.0)),
    );
}

fn linear_color(logical_pos: vec2<f32>) -> vec4<f32> {
    let point = paint_point(logical_pos);
    let start = paint.geometry_0.xy;
    let delta = paint.geometry_0.zw - start;
    let length_squared = dot(delta, delta);
    if length_squared == 0.0 {
        return vec4<f32>(0.0);
    }
    return sample_stops(dot(point - start, delta) / length_squared);
}

fn radial_color(logical_pos: vec2<f32>) -> vec4<f32> {
    let point = paint_point(logical_pos);
    let center = paint.geometry_0.xy;
    let outer_radius = paint.geometry_0.z;
    let inner_radius = paint.geometry_0.w;
    let focal = paint.geometry_1.xy;
    let q = point - focal;
    let center_delta = center - focal;
    let radius_delta = outer_radius - inner_radius;
    let quadratic = dot(center_delta, center_delta) - radius_delta * radius_delta;
    let linear = dot(q, center_delta) + inner_radius * radius_delta;
    let distance = dot(q, q) - inner_radius * inner_radius;
    var t = 0.0;
    if abs(quadratic) < 0.000001 {
        if abs(linear) < 0.000001 {
            if abs(distance) >= 0.000001 {
                return vec4<f32>(0.0);
            }
        } else {
            t = distance / (2.0 * linear);
        }
    } else {
        let discriminant = linear * linear - quadratic * distance;
        if discriminant < 0.0 {
            return vec4<f32>(0.0);
        }
        let root = sqrt(discriminant);
        let first = (linear - root) / quadratic;
        let second = (linear + root) / quadratic;
        let first_valid = inner_radius + radius_delta * first >= 0.0;
        let second_valid = inner_radius + radius_delta * second >= 0.0;
        if first_valid && second_valid {
            t = max(first, second);
        } else if first_valid {
            t = first;
        } else if second_valid {
            t = second;
        } else {
            return vec4<f32>(0.0);
        }
    }
    if inner_radius + radius_delta * t < 0.0 {
        return vec4<f32>(0.0);
    }
    return sample_stops(t);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    switch paint.kind_spread_count.x {
        case 1u: {
            return linear_color(input.logical_pos);
        }
        case 2u: {
            return radial_color(input.logical_pos);
        }
        default: {
            return paint.color;
        }
    }
}
