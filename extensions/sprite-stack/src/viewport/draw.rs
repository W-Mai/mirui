use super::{Camera, ScenePainter};
use crate::{
    Geometry, HeightField, Pose, PreparedSpriteStack, SpriteStackAsset, StackScene,
    raster::{Frame, Projection, SceneCanvas, Workspace},
};
use mirui::{
    render::{
        RenderError,
        command::{CompositeMode, DrawCommand},
    },
    types::{Fixed, Fixed64, Point},
    ui::view::PaintCtx,
};

type V3 = [Fixed; 3];
const BAND: usize = 16;

#[derive(Default)]
pub(super) struct DefaultPainter {
    output: Option<(Frame, Workspace)>,
    revision: Option<u64>,
}

impl ScenePainter for DefaultPainter {
    fn paint(
        &mut self,
        scene: &StackScene,
        camera: Camera,
        revision: u64,
        paint: &mut PaintCtx<'_>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        let (w, h) = (rect.w.ceil().to_int(), rect.h.ceil().to_int());
        if w <= 0 || h <= 0 {
            return Ok(());
        }
        if w > i32::from(u16::MAX) || h > i32::from(u16::MAX) || !camera.scale.is_positive() {
            return Err(RenderError::InvalidGeometry);
        }
        let (w, h) = (w as usize, h as usize);
        if self
            .output
            .as_ref()
            .is_none_or(|(f, _)| (f.width, f.height) != (w, h))
        {
            let mut frame = Frame::new(w, h)?;
            frame.texture.alpha_mode = mirui::render::texture::AlphaMode::Blend;
            self.output = Some((frame, Workspace::new(w, BAND.min(h))?));
            self.revision = None;
        }
        let (frame, work) = self.output.as_mut().expect("viewport storage");
        if self.revision != Some(revision) {
            let projection = ViewProjection::new(camera, camera.center, [w, h]);
            for y in (0..h).step_by(BAND) {
                work.with_rows(frame, y, BAND.min(h - y), |tile| {
                    tile.color.bytes.fill(0);
                    tile.clear_depth();
                    let mut result = Ok(());
                    scene.for_each(|_, geometry, pose| {
                        if result.is_err() {
                            return;
                        }
                        let transform = ObjectProjection::new(&projection, pose);
                        match geometry {
                            Geometry::Sprite(sprite) => {
                                result = draw_atlas(tile, &transform, sprite.get())
                            }
                            Geometry::Prepared(sprite) => draw_sparse(tile, &transform, sprite),
                            _ => {
                                geometry.with_field(|field| draw_field(tile, &transform, field));
                            }
                        }
                    });
                    result
                })?;
            }
            frame.texture.cache_revision = frame.texture.cache_revision.wrapping_add(1);
            self.revision = Some(revision);
        }
        paint.draw(&DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: paint.transform(),
            quad: None,
            texture: &frame.texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
        Ok(())
    }
}

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
struct ViewProjection {
    projection: Projection,
    origin: V3,
    yaw: (Fixed, Fixed),
    pitch: (Fixed, Fixed),
    radius: Fixed,
}
impl ViewProjection {
    fn new(camera: Camera, origin: V3, [w, h]: [usize; 2]) -> Self {
        Self {
            projection: Projection {
                perspective: Fixed64::ZERO,
                slope: Fixed::ZERO,
                scale: camera.scale,
                center: [Fixed::from_int(w as i32) / 2, Fixed::from_int(h as i32) / 2],
            },
            origin,
            yaw: trig(camera.yaw),
            pitch: trig(camera.pitch),
            radius: camera.range,
        }
    }
    fn point(&self, point: V3) -> V3 {
        let [x, y, z] = core::array::from_fn(|i| point[i] - self.origin[i]);
        let (s, c) = self.yaw;
        let (sp, cp) = self.pitch;
        let forward = x * s + z * c;
        let [screen_x, screen_y] = self.projection.screen_point(
            x * c - z * s,
            y * cp + forward * sp,
            self.projection.scale,
        );
        [screen_x, screen_y, forward * cp - y * sp]
    }
}
struct ObjectProjection<'a> {
    camera: &'a ViewProjection,
    pose: Pose,
    yaw: (Fixed, Fixed),
    pitch: (Fixed, Fixed),
    roll: (Fixed, Fixed),
}
impl<'a> ObjectProjection<'a> {
    fn new(camera: &'a ViewProjection, pose: Pose) -> Self {
        Self {
            camera,
            pose,
            yaw: trig(pose.yaw),
            pitch: trig(pose.pitch),
            roll: trig(pose.roll),
        }
    }
    fn point(&self, [x, y, z]: V3) -> V3 {
        let (sr, cr) = self.roll;
        let (sp, cp) = self.pitch;
        let (sy, cy) = self.yaw;
        let (y, z) = (y * cp + z * sp, -y * sp + z * cp);
        let (x, y) = (x * cr - y * sr, x * sr + y * cr);
        self.camera.point([
            self.pose.position[0] + x * cy + z * sy,
            self.pose.position[1] + y,
            self.pose.position[2] + z * cy - x * sy,
        ])
    }
    fn quad(&self, tile: &mut SceneCanvas<'_, '_>, corners: [V3; 4], color: u32) {
        let [a, b, c, d] = corners.map(|p| self.point(p));
        triangle(tile, [a, b, c], color);
        triangle(tile, [a, c, d], color);
    }
}

fn triangle(tile: &mut SceneCanvas<'_, '_>, v: [V3; 3], color: u32) {
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
    let q = |p: V3| p.map(|a| (Fixed64::from(a) * 256).to_int());
    let [a, bp, c] = v.map(q);
    let edge = |p: [i64; 3], q: [i64; 3], x: i64, y: i64| {
        (q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])
    };
    let area = edge(a, bp, c[0], c[1]);
    if area == 0 {
        return;
    }
    for y in t.max(y0)..b.min(y1) {
        for x in l.max(x0)..r.min(x1) {
            let (px, py) = (i64::from(x) * 256 + 128, i64::from(y) * 256 + 128);
            let u = edge(bp, c, px, py);
            let v = edge(c, a, px, py);
            let w = area - u - v;
            if (area > 0 && u.min(v).min(w) < 0) || (area < 0 && u.max(v).max(w) > 0) {
                continue;
            }
            let depth = a[2]
                + ((i128::from(v) * i128::from(bp[2] - a[2])
                    + i128::from(w) * i128::from(c[2] - a[2]))
                    / i128::from(area)) as i64;
            tile.write_visible(x, y, (Fixed64::from_int(depth) / 256).to_fixed(), color);
        }
    }
}

fn top(
    t: &ObjectProjection<'_>,
    tile: &mut SceneCanvas<'_, '_>,
    [x, y, z]: V3,
    dx: Fixed,
    dz: Fixed,
    color: u32,
) {
    t.quad(
        tile,
        [
            [x, y, z],
            [x + dx, y, z],
            [x + dx, y, z + dz],
            [x, y, z + dz],
        ],
        color,
    );
}
/// At zero elevation a layer has one model pixel of thickness. Pick its visible side faces once per instance.
struct SliceProjection<'a, 'b> {
    object: &'a ObjectProjection<'b>,
    edge: Option<[Fixed; 2]>,
}

impl<'a, 'b> SliceProjection<'a, 'b> {
    fn new(object: &'a ObjectProjection<'b>) -> Self {
        let origin = object.point([Fixed::ZERO; 3])[2];
        let depth = |axis: usize| {
            let mut point = [Fixed::ZERO; 3];
            point[axis] = Fixed::ONE;
            object.point(point)[2] - origin
        };
        Self {
            object,
            edge: depth(1).is_zero().then(|| [depth(0), depth(2)]),
        }
    }

    fn cell(&self, tile: &mut SceneCanvas<'_, '_>, at: V3, dx: Fixed, dz: Fixed, color: u32) {
        let Some([depth_x, depth_z]) = self.edge else {
            return top(self.object, tile, at, dx, dz, color);
        };
        let [x, y, z] = at;
        let mut side = |a: [Fixed; 2], b: [Fixed; 2]| {
            self.object.quad(
                tile,
                [
                    [a[0], y - Fixed::HALF, a[1]],
                    [b[0], y - Fixed::HALF, b[1]],
                    [b[0], y + Fixed::HALF, b[1]],
                    [a[0], y + Fixed::HALF, a[1]],
                ],
                color,
            );
        };
        if !depth_x.is_zero() {
            let near = if depth_x.is_negative() { x + dx } else { x };
            side([near, z], [near, z + dz]);
        }
        if !depth_z.is_zero() {
            let near = if depth_z.is_positive() { z + dz } else { z };
            side([x, near], [x + dx, near]);
        }
    }
}

// The camera scale is positive. Checking it first bounds the product for every u16 grid step.
fn fits_pixel(scale: Fixed, step: u16) -> bool {
    scale <= Fixed::ONE && scale * i32::from(step) <= Fixed::ONE
}

#[derive(Clone, Copy)]
struct LayerGrid {
    size: [u16; 2],
    pivot: [Fixed; 2],
}
impl LayerGrid {
    /// Upright quarter turns map each local axis to one world axis. The orthographic
    /// projection is monotone along each of those axes, including fixed-point rounding.
    fn intersects_band(
        self,
        slice: &SliceProjection<'_, '_>,
        mut elevations: [Fixed; 2],
        clip: [i32; 4],
    ) -> bool {
        let t = slice.object;
        if t.pitch != (Fixed::ZERO, Fixed::ONE)
            || t.roll != (Fixed::ZERO, Fixed::ONE)
            || (!t.yaw.0.is_zero() && !t.yaw.1.is_zero())
        {
            return true;
        }
        // Empty parts of a sparse canvas can reach farther than any occupied cell.
        // Only project its corners when every intermediate fits the raster coordinates.
        let extent = self
            .pivot
            .into_iter()
            .chain(t.pose.position)
            .chain(t.camera.origin)
            .map(|v| i64::from(v.to_int()).abs() + 1)
            .sum::<i64>()
            + i64::from(self.size[0])
            + i64::from(self.size[1])
            + elevations
                .into_iter()
                .map(|v| i64::from(v.to_int()).abs())
                .max()
                .unwrap()
            + 4;
        let scale = i64::from(t.camera.projection.scale.to_int()) + 1;
        let [cx, cy] = t.camera.projection.center;
        let center = cx.max(cy).to_int();
        if extent * scale + i64::from(center) >= i64::from(Fixed::MAX.to_int()) {
            return true;
        }
        if slice.edge.is_some() {
            elevations[0] -= Fixed::HALF;
            elevations[1] += Fixed::HALF;
        }
        let mut low = [Fixed::MAX; 2];
        let mut high = [Fixed::MIN; 2];
        for y in elevations {
            for x in [-self.pivot[0], Fixed::from(self.size[0]) - self.pivot[0]] {
                for z in [self.pivot[1], self.pivot[1] - Fixed::from(self.size[1])] {
                    let p = t.point([x, y, z]);
                    for axis in 0..2 {
                        low[axis] = low[axis].min(p[axis]);
                        high[axis] = high[axis].max(p[axis]);
                    }
                }
            }
        }
        high[0] > Fixed::from_int(clip[0])
            && low[0] < Fixed::from_int(clip[2])
            && high[1] > Fixed::from_int(clip[1])
            && low[1] < Fixed::from_int(clip[3])
    }

    fn cell(
        self,
        slice: &SliceProjection<'_, '_>,
        tile: &mut SceneCanvas<'_, '_>,
        at: [u16; 2],
        elevation: Fixed,
        step: u16,
        color: u32,
    ) {
        let [x, z] = at;
        slice.cell(
            tile,
            [
                Fixed::from(x) - self.pivot[0],
                elevation,
                self.pivot[1] - Fixed::from(z),
            ],
            Fixed::from(step.min(self.size[0] - x)),
            -Fixed::from(step.min(self.size[1] - z)),
            color,
        );
    }
}
fn draw_atlas(
    tile: &mut SceneCanvas<'_, '_>,
    t: &ObjectProjection<'_>,
    asset: &SpriteStackAsset,
) -> Result<(), RenderError> {
    let slice = SliceProjection::new(t);
    let d = asset.descriptor();
    let [px, pz] = d.pivot_q8().map(|n| Fixed::from_ratio(n, 256));
    let spacing = Fixed::from_ratio(d.spacing_q8(), 256);
    let (w, h) = d.canvas();
    let grid = LayerGrid {
        size: [w, h],
        pivot: [px, pz],
    };
    let top = spacing * (d.layers().len() - 1) as i32;
    if !grid.intersects_band(&slice, [Fixed::ZERO, top], tile.color.clip) {
        return Ok(());
    }
    if let Some((levels, palette)) = asset.levels {
        let scale = t.camera.projection.scale;
        if let Some(level) = (0..levels.len())
            .rev()
            .filter_map(|i| levels.level(i))
            .find(|l| fits_pixel(scale, l.step()))
        {
            for i in 0..d.layers().len() {
                let elevation = spacing * i as i32;
                if !grid.intersects_band(&slice, [elevation; 2], tile.color.clip) {
                    continue;
                }
                for sample in level.layer(i).expect("validated layer") {
                    let rgba = match sample.color {
                        crate::format::lod::LodColor::Rgba(rgba) => rgba,
                        crate::format::lod::LodColor::Indexed(index) => {
                            let start = usize::from(index) * 4;
                            palette[start..start + 4]
                                .try_into()
                                .expect("validated palette index")
                        }
                    };
                    grid.cell(
                        &slice,
                        tile,
                        [sample.x, sample.y],
                        elevation,
                        level.step(),
                        u32::from_le_bytes(rgba),
                    );
                }
            }
            return Ok(());
        }
    }
    let image = asset.atlas().read_pixels()?;
    for i in 0..d.layers().len() {
        let region = d.layers().get(i).expect("validated layer");
        let elevation = spacing * i as i32;
        if !grid.intersects_band(&slice, [elevation; 2], tile.color.clip) {
            continue;
        }
        for y in 0..region.height() as i32 {
            for x in 0..region.width() as i32 {
                let c = image.get_pixel(region.x() as i32 + x, region.y() as i32 + y);
                if c.a == 0 {
                    continue;
                }
                grid.cell(
                    &slice,
                    tile,
                    [x as u16, y as u16],
                    elevation,
                    1,
                    u32::from_le_bytes([c.r, c.g, c.b, c.a]),
                );
            }
        }
    }
    Ok(())
}
fn draw_sparse(
    tile: &mut SceneCanvas<'_, '_>,
    t: &ObjectProjection<'_>,
    asset: &PreparedSpriteStack<'_>,
) {
    let slice = SliceProjection::new(t);
    let [w, h, layers] = asset.extent();
    let [px, pz, ph] = asset.pivot_q8().map(|v| Fixed::from_ratio(v, 256));
    let spacing = Fixed::from_ratio(asset.spacing_q8(), 256);
    let grid = LayerGrid {
        size: [w, h],
        pivot: [px, pz],
    };
    let level = asset
        .levels()
        .iter()
        .rev()
        .find(|level| fits_pixel(t.camera.projection.scale, level.step()))
        .unwrap_or(&asset.levels()[0]);
    for i in 0..layers {
        let samples = level.layer(usize::from(i)).expect("validated layer");
        if samples.is_empty() {
            continue;
        }
        let elevation = (Fixed::from(i) - ph) * spacing;
        if !grid.intersects_band(&slice, [elevation; 2], tile.color.clip) {
            continue;
        }
        for s in samples {
            let (x, y) = (u16::from(s.x), u16::from(s.y));
            grid.cell(
                &slice,
                tile,
                [x, y],
                elevation,
                level.step(),
                u32::from_le_bytes(asset.palette()[usize::from(s.color)]),
            );
        }
    }
}
fn draw_field(tile: &mut SceneCanvas<'_, '_>, t: &ObjectProjection<'_>, field: &HeightField) {
    let g = field.grid();
    let cell = g.cell_size();
    let (sy, cy) = t.yaw;
    let x = t.camera.origin[0] - t.pose.position[0];
    let z = t.camera.origin[2] - t.pose.position[2];
    let center = [x * cy - z * sy, x * sy + z * cy];
    let radius = t.camera.radius;
    let span = |i: usize| {
        let to_cell = |v: Fixed| {
            ((Fixed64::from(v) * 256).to_int()).div_euclid((Fixed64::from(cell) * 256).to_int())
                as i32
        };
        let mut low = to_cell(center[i] - radius);
        let mut high = to_cell(center[i] + radius);
        if field.has_finite_local_bounds() {
            low = low.max(0);
            high = high.min(i32::from(g.size()[i]) - 1);
        }
        low..=high
    };
    for row in span(1) {
        for col in span(0) {
            let (x, z) = (cell * col, cell * row);
            let Some(index) = field.address_at(x + cell / 2, z + cell / 2) else {
                continue;
            };
            let y = g.elevation(field.levels()[index]);
            let c = field.color(field.materials()[index]);
            let color = u32::from_le_bytes([c.r, c.g, c.b, c.a]);
            top(t, tile, [x, y, z], cell, cell, color);
            let base = g.elevation(0);
            if y != base {
                for (a, b) in [
                    ([x, z], [x + cell, z]),
                    ([x + cell, z], [x + cell, z + cell]),
                    ([x + cell, z + cell], [x, z + cell]),
                    ([x, z + cell], [x, z]),
                ] {
                    t.quad(
                        tile,
                        [
                            [a[0], base, a[1]],
                            [b[0], base, b[1]],
                            [b[0], y, b[1]],
                            [a[0], y, a[1]],
                        ],
                        color,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_culling_preserves_rasterized_cells_at_fractional_and_edge_on_views() {
        let size = [49, 71];
        let grid = LayerGrid {
            size: [9, 5],
            pivot: [Fixed::from_ratio(7, 2), Fixed::from_ratio(5, 4)],
        };
        let elevation = Fixed::from_ratio(9, 4);
        let mut culled = 0;
        for pitch in [-34, 0, 29, 90] {
            let camera = Camera {
                yaw: Fixed::from_int(33),
                pitch: Fixed::from_int(pitch),
                scale: Fixed::from_ratio(17, 4),
                ..Camera::default()
            };
            let view = ViewProjection::new(camera, [Fixed::HALF; 3], size);
            for yaw in [0, 90, 180, 270, 23] {
                let pose = Pose {
                    yaw: Fixed::from_int(yaw),
                    ..Pose::default()
                };
                let object = ObjectProjection::new(&view, pose);
                let slice = SliceProjection::new(&object);
                let mut frame = Frame::new(size[0], size[1]).unwrap();
                let mut work = Workspace::new(size[0], size[1]).unwrap();
                work.with_rows(&mut frame, 0, size[1], |tile| {
                    tile.clear_depth();
                    for y in (0..grid.size[1]).step_by(2) {
                        for x in (0..grid.size[0]).step_by(2) {
                            grid.cell(&slice, tile, [x, y], elevation, 2, u32::MAX);
                        }
                    }
                    for y in 0..size[1] {
                        if !grid.intersects_band(
                            &slice,
                            [elevation; 2],
                            [0, y as i32, size[0] as i32, y as i32 + 1],
                        ) {
                            culled += 1;
                            assert!((0..size[0]).all(|x| tile.get(y * size[0] + x) == 0));
                        }
                    }
                });
            }
        }
        assert!(culled > 0);
    }
}
