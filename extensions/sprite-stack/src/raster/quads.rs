//! Depth-tested camera-space quads for opaque surfaces.

use super::{Projection, SceneCanvas};
use mirui::types::{Fixed, Fixed64};

type V3 = [Fixed; 3];

pub(crate) fn draw_quad(
    tile: &mut SceneCanvas<'_, '_>,
    p: &Projection,
    corners: [V3; 4],
    color: u32,
) {
    if p.perspective == Fixed64::ZERO {
        let [a, b, c, d] = corners.map(|[right, depth, up]| {
            let [x, y] = p.screen_point(right, up + p.slope * depth, p.scale);
            [x, y, depth]
        });
        triangle::<false>(tile, [a, b, c], [1; 3], color);
        triangle::<false>(tile, [a, c, d], [1; 3], color);
        return;
    }
    let mut clipped = [[Fixed::ZERO; 4]; 8];
    let mut count = 0;
    let mut prev = corners[3];
    let denom = |v: V3| Fixed::ONE + p.apply_perspective(v[1]);
    let mut last = denom(prev);
    let near = Projection::NEAR_DENOMINATOR;
    let mut add = |v: V3, d: Fixed| {
        let [right, depth, up] = v;
        let [x, y] = p.screen_point(right, up + p.slope * depth, p.scale / d);
        clipped[count] = [x, y, depth, d];
        count += 1;
    };
    for current in corners {
        let d = denom(current);
        if (d < near && last > near) || (d > near && last < near) {
            let t = (Fixed64::from(near) - Fixed64::from(last))
                / (Fixed64::from(d) - Fixed64::from(last));
            let edge = core::array::from_fn(|i| {
                (Fixed64::from(prev[i]) + (Fixed64::from(current[i]) - Fixed64::from(prev[i])) * t)
                    .to_fixed()
            });
            add(edge, near);
        }
        if d >= near {
            add(current, d);
        }
        prev = current;
        last = d;
    }
    for i in 2..count {
        let points = [clipped[0], clipped[i - 1], clipped[i]];
        // Multiply by the other two denominators to interpolate camera depth in perspective.
        let d = points.map(|v| (Fixed64::from(v[3]) * 256).to_int());
        triangle::<true>(
            tile,
            points.map(|[x, y, z, _]| [x, y, z]),
            [
                i128::from(d[1]) * i128::from(d[2]),
                i128::from(d[0]) * i128::from(d[2]),
                i128::from(d[0]) * i128::from(d[1]),
            ],
            color,
        );
    }
}

fn triangle<const PERSPECTIVE: bool>(
    tile: &mut SceneCanvas<'_, '_>,
    v: [V3; 3],
    weights: [i128; 3],
    color: u32,
) {
    let [x0, y0, x1, y1] = tile.color.clip;
    let minmax = |axis: usize| {
        let c = v.map(|p| p[axis]);
        [
            c[0].min(c[1]).min(c[2]).to_int(),
            c[0].max(c[1]).max(c[2]).ceil().to_int(),
        ]
    };
    let [l, r] = minmax(0);
    let [t, b] = minmax(1);
    if r <= x0 || l >= x1 || b <= y0 || t >= y1 {
        return;
    }
    let q = |p: V3| p.map(|v| (Fixed64::from(v) * 256).to_int());
    let [a, bp, c] = v.map(q);
    let edge = |p: [i64; 3], q: [i64; 3], x: i64, y: i64| {
        (q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])
    };
    let area = edge(a, bp, c[0], c[1]);
    if area == 0 {
        return;
    }
    let flat = (a[2] == bp[2] && bp[2] == c[2]).then_some(v[0][2]);
    let left = l.max(x0);
    let top = t.max(y0);
    let x = i64::from(left) * 256 + 128;
    let y = i64::from(top) * 256 + 128;
    let mut row_u = edge(bp, c, x, y);
    let mut row_v = edge(c, a, x, y);
    let dx = [(bp[1] - c[1]) * 256, (c[1] - a[1]) * 256];
    let dy = [(c[0] - bp[0]) * 256, (a[0] - c[0]) * 256];
    for y in top..b.min(y1) {
        let (mut next_u, mut next_v) = (row_u, row_v);
        row_u += dy[0];
        row_v += dy[1];
        for x in left..r.min(x1) {
            let (u, v) = (next_u, next_v);
            next_u += dx[0];
            next_v += dx[1];
            let w = area - u - v;
            if (area > 0 && u.min(v).min(w) < 0) || (area < 0 && u.max(v).max(w) > 0) {
                continue;
            }
            let depth = flat.unwrap_or_else(|| {
                let (v, w, sum) = if PERSPECTIVE {
                    let [u, v, w] = [
                        i128::from(u) * weights[0],
                        i128::from(v) * weights[1],
                        i128::from(w) * weights[2],
                    ];
                    (v, w, u + v + w)
                } else {
                    (i128::from(v), i128::from(w), i128::from(area))
                };
                let z = a[2]
                    + ((v * i128::from(bp[2] - a[2]) + w * i128::from(c[2] - a[2])) / sum) as i64;
                (Fixed64::from_int(z) / 256).to_fixed()
            });
            tile.write_visible(x, y, depth, color);
        }
    }
}
