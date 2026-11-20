use crate::{
    Camera, Pose,
    raster::{Projection, SceneCanvas, sprites::StackTransform},
};
use mirui::types::{Fixed, Fixed64};

pub(super) type V3 = [Fixed; 3];
fn trig(angle: Fixed) -> (Fixed, Fixed) {
    let a = Fixed::from_int(angle.to_int().rem_euclid(360)) + angle.fract();
    match a.to_int() {
        0 if a.fract().is_zero() => (Fixed::ZERO, Fixed::ONE),
        90 if a.fract().is_zero() => (Fixed::ONE, Fixed::ZERO),
        180 if a.fract().is_zero() => (Fixed::ZERO, -Fixed::ONE),
        270 if a.fract().is_zero() => (-Fixed::ONE, Fixed::ZERO),
        _ => (Fixed::sin_deg(a), Fixed::cos_deg(a)),
    }
}
/// Maps world X/Y/Z to a screen projection. Y is up.
pub struct ViewProjection {
    pub(super) projection: Projection,
    pub(super) origin: V3,
    pub(super) yaw: (Fixed, Fixed),
    pub(super) pitch: (Fixed, Fixed),
    pub(super) radius: Fixed,
}
impl ViewProjection {
    /// Maps a scene camera with a resolved target to a viewport in pixels.
    pub fn from_camera(
        camera: Camera,
        size: [usize; 2],
    ) -> Result<Self, mirui::render::RenderError> {
        Ok(Self::from_plane(camera, Self::camera_plane(camera, size)?))
    }

    pub(super) fn camera_plane(
        camera: Camera,
        [w, h]: [usize; 2],
    ) -> Result<Projection, mirui::render::RenderError> {
        let invalid = mirui::render::RenderError::InvalidGeometry;
        if !camera.scale.is_positive() {
            return Err(invalid);
        }
        let perspective = match camera.focal_distance {
            Some(distance) if distance.is_positive() => Fixed64::ONE / Fixed64::from(distance),
            Some(_) => return Err(invalid),
            None => Fixed64::ZERO,
        };
        Ok(Projection {
            perspective,
            slope: Fixed::ZERO,
            scale: camera.scale,
            center: [Fixed::from_int(w as i32) / 2, Fixed::from_int(h as i32) / 2],
        })
    }

    pub(super) fn from_plane(camera: Camera, projection: Projection) -> Self {
        Self {
            projection,
            origin: camera.center,
            yaw: trig(camera.yaw),
            pitch: trig(camera.pitch),
            radius: camera.range,
        }
    }
    /// Uses a camera-space projection and its unit forward direction in world X/Z.
    pub fn horizontal(
        projection: Projection,
        origin: V3,
        forward: [Fixed; 2],
        range: Fixed,
    ) -> Self {
        Self {
            projection,
            origin,
            yaw: (forward[0], forward[1]),
            pitch: (Fixed::ZERO, Fixed::ONE),
            radius: range,
        }
    }
    fn camera_point(&self, point: V3) -> V3 {
        self.vector(core::array::from_fn(|i| point[i] - self.origin[i]))
    }
    fn vector(&self, [x, y, z]: V3) -> V3 {
        let (s, c) = self.yaw;
        let (sp, cp) = self.pitch;
        let forward = x * s + z * c;
        [x * c - z * s, forward * cp - y * sp, y * cp + forward * sp]
    }
}
#[derive(Clone, Copy)]
pub(super) struct InstancePose {
    pub(super) position: V3,
    pub(super) yaw: (Fixed, Fixed),
    pub(super) pitch: (Fixed, Fixed),
    pub(super) roll: (Fixed, Fixed),
}
impl InstancePose {
    pub(super) fn new(pose: Pose) -> Self {
        Self {
            position: pose.position,
            yaw: trig(pose.yaw),
            pitch: trig(pose.pitch),
            roll: trig(pose.roll),
        }
    }
}
pub(super) struct ObjectProjection<'a> {
    pub(super) camera: &'a ViewProjection,
    pub(super) instance: InstancePose,
}

/// Object-local rays parameterized by camera depth. The screen plane and the eye direction
/// use the same projection, including its depth-dependent vertical offset.
pub(super) struct FieldRays {
    origin: [Fixed64; 3],
    right: [Fixed64; 3],
    up: [Fixed64; 3],
    forward: [Fixed64; 3],
    pub(super) depth: [Fixed64; 2],
    perspective: Fixed64,
    center: [Fixed; 2],
}

impl FieldRays {
    pub(super) fn through(&self, x: i32, y: i32) -> ([Fixed64; 3], [Fixed64; 3]) {
        let x = Fixed64::from(Fixed::from_int(x) + Fixed::HALF - self.center[0]);
        let y = Fixed64::from(self.center[1] - Fixed::from_int(y) - Fixed::HALF);
        let offset: [_; 3] = core::array::from_fn(|i| self.right[i] * x + self.up[i] * y);
        (
            core::array::from_fn(|i| self.origin[i] + offset[i]),
            core::array::from_fn(|i| self.forward[i] + offset[i] * self.perspective),
        )
    }
}

impl core::ops::Deref for ObjectProjection<'_> {
    type Target = InstancePose;
    fn deref(&self) -> &InstancePose {
        &self.instance
    }
}
impl<'a> ObjectProjection<'a> {
    pub(super) fn field_rays(&self) -> FieldRays {
        // Rounded sine and cosine need not have exactly unit norm.
        let inverse = |v: (Fixed, Fixed)| {
            let (s, c) = (Fixed64::from(v.0), Fixed64::from(v.1));
            let norm = s * s + c * c;
            (s / norm, c / norm)
        };
        let rotate =
            |a: Fixed64, b: Fixed64, (s, c): (Fixed64, Fixed64)| (c * a + s * b, c * b - s * a);
        let (s, c) = self.yaw;
        let yaw = inverse((-s, c));
        let roll = inverse(self.roll);
        let (s, c) = self.pitch;
        let pitch = inverse((-s, c));
        let local = |[x, y, z]: [Fixed64; 3]| {
            let (x, z) = rotate(x, z, yaw);
            let (x, y) = rotate(x, y, roll);
            let (y, z) = rotate(y, z, pitch);
            [x, y, z]
        };
        let camera_pitch = inverse(self.camera.pitch);
        let camera_yaw = inverse(self.camera.yaw);
        let axis = |x, depth, up| {
            let (f, y) = rotate(depth, up, camera_pitch);
            let (x, z) = rotate(x, f, camera_yaw);
            local([x, y, z])
        };
        let p = self.camera.projection;
        let [zero, one] = [Fixed64::ZERO, Fixed64::ONE];
        let scale = one / Fixed64::from(p.scale);
        let mut depth = [Fixed::MIN.into(), Fixed::MAX.into()];
        if !p.perspective.is_zero() {
            depth[usize::from(p.perspective.is_negative())] =
                (Fixed64::from(Projection::NEAR_DENOMINATOR) - one) / p.perspective;
        }
        FieldRays {
            origin: local(core::array::from_fn(|i| {
                Fixed64::from(self.camera.origin[i]) - Fixed64::from(self.position[i])
            })),
            right: axis(scale, zero, zero),
            up: axis(zero, zero, scale),
            forward: axis(zero, one, -Fixed64::from(p.slope)),
            perspective: p.perspective,
            center: p.center,
            depth,
        }
    }

    #[cfg(test)]
    pub(super) fn new(camera: &'a ViewProjection, pose: Pose) -> Self {
        Self {
            camera,
            instance: InstancePose::new(pose),
        }
    }
    fn vector(&self, [x, y, z]: V3) -> V3 {
        let (sr, cr) = self.roll;
        let (sp, cp) = self.pitch;
        let (sy, cy) = self.yaw;
        let (y, z) = (y * cp + z * sp, -y * sp + z * cp);
        let (x, y) = (x * cr - y * sr, x * sr + y * cr);
        [x * cy + z * sy, y, z * cy - x * sy]
    }
    fn camera_point(&self, local: V3) -> V3 {
        let v = self.vector(local);
        self.camera
            .camera_point(core::array::from_fn(|i| self.position[i] + v[i]))
    }
    pub(super) fn point(&self, local: V3) -> V3 {
        let [right, depth, up] = self.camera_point(local);
        let [x, y] = self.camera.projection.screen_point(
            right,
            up + self.camera.projection.slope * depth,
            self.camera.projection.scale,
        );
        [x, y, depth]
    }
    pub(super) fn stack(&self, layer_spacing: Fixed) -> StackTransform {
        let offset = core::array::from_fn(|i| self.position[i] - self.camera.origin[i]);
        let axes = [0, 2, 1].map(|axis| {
            let mut local = [Fixed::ZERO; 3];
            local[axis] = Fixed::ONE;
            self.vector(local)
        });
        StackTransform {
            layer_spacing,
            ..StackTransform::from_basis(offset, axes, |v| self.camera.vector(v))
        }
    }
    pub(super) fn quad(&self, tile: &mut SceneCanvas<'_, '_>, corners: [V3; 4], color: u32) {
        let p = &self.camera.projection;
        if p.perspective == Fixed64::ZERO {
            let [a, b, c, d] = corners.map(|v| self.point(v));
            triangle::<false>(tile, [a, b, c], [1; 3], color);
            triangle::<false>(tile, [a, c, d], [1; 3], color);
            return;
        }
        let vertices = corners.map(|v| self.camera_point(v));
        let mut clipped = [[Fixed::ZERO; 4]; 8];
        let mut count = 0;
        let mut prev = vertices[3];
        let denom = |v: V3| Fixed::ONE + p.apply_perspective(v[1]);
        let mut last = denom(prev);
        let near = Projection::NEAR_DENOMINATOR;
        let mut add = |v: V3, d: Fixed| {
            let [right, depth, up] = v;
            let [x, y] = p.screen_point(right, up + p.slope * depth, p.scale / d);
            clipped[count] = [x, y, depth, d];
            count += 1;
        };
        for current in vertices {
            let d = denom(current);
            if (d < near && last > near) || (d > near && last < near) {
                let t = (Fixed64::from(near) - Fixed64::from(last))
                    / (Fixed64::from(d) - Fixed64::from(last));
                let edge = core::array::from_fn(|i| {
                    (Fixed64::from(prev[i])
                        + (Fixed64::from(current[i]) - Fixed64::from(prev[i])) * t)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Frame, Workspace};

    #[test]
    fn crossing_the_eye_plane_clips_faces_and_preserves_projective_depth() {
        let camera = ViewProjection::horizontal(
            Projection {
                perspective: Fixed64::from_ratio(1, 8),
                slope: Fixed::ZERO,
                scale: Fixed::from_int(6),
                center: [Fixed::from_int(16); 2],
            },
            [Fixed::ZERO; 3],
            [Fixed::ZERO, Fixed::ONE],
            Fixed::from_int(32),
        );
        let object = ObjectProjection::new(&camera, Pose::default());
        let mut frame = Frame::new(32, 32).unwrap();
        let mut work = Workspace::new(32, 32).unwrap();
        work.with_canvas(&mut frame, |tile| {
            tile.clear_depth();
            let q = |v: [i32; 3]| v.map(Fixed::from_int);
            object.quad(
                tile,
                [[-1, -4, -10], [2, -4, 8], [2, 4, 8], [-1, 4, -10]].map(q),
                u32::MAX,
            );
            // At pixel center x=16.5 the ray has x=(1+z/8)/12 and the face has z=6*x-4.
            let depth = tile.sample_depth(16, 16).unwrap();
            assert!(
                (depth - Fixed::from_ratio(-56, 15)).abs() < Fixed::from_ratio(1, 10),
                "{depth:?}"
            );
            let i = tile.color.index(16, 16);
            object.quad(
                tile,
                [[-8, -8, -9], [8, -8, -9], [8, 8, -9], [-8, 8, -9]].map(q),
                0xff0000ff,
            );
            assert_eq!(tile.get(i), u32::MAX);
        });
    }
}
