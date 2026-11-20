use super::projection::{InstancePose, ObjectProjection, V3, ViewProjection};
use crate::{
    Geometry, HeightField, Object, Pose, PreparedSpriteStack, SpriteStackAsset, StackScene,
    raster::{
        SceneCanvas,
        sprites::{LayerGrid, StackTransform, ViewStack},
    },
};
use mirui::{
    render::RenderError,
    types::{Fixed, Fixed64},
};
/// Per-viewport storage for the visible scene instances. Projection buffers grow only when a newly placed object needs additional storage.
#[derive(Default)]
pub struct ObjectPass {
    objects: alloc::vec::Vec<(Object, InstancePose, Fixed)>,
    sampled: alloc::vec::Vec<ViewStack>,
}

impl ObjectPass {
    /// Heap bytes reserved for instance transforms and sampled sprite layers.
    pub fn storage_bytes(&self) -> usize {
        self.objects.capacity() * core::mem::size_of::<(Object, InstancePose, Fixed)>()
            + self.sampled.capacity() * core::mem::size_of::<ViewStack>()
            + self
                .sampled
                .iter()
                .map(ViewStack::storage_bytes)
                .sum::<usize>()
    }

    /// Drops the layer buffers of removed instances. The object slots remain available for reuse.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.sampled.clear();
    }

    /// Resolves the selected instances once for this frame. Drawing any number of tiles reuses these transforms and the scene-owned geometry.
    pub fn prepare<'a>(
        &'a mut self,
        scene: &'a StackScene,
        camera: ViewProjection,
        mut include: impl FnMut(Object, &Geometry, Pose) -> bool,
    ) -> Result<ProjectedObjects<'a>, RenderError> {
        self.objects.clear();
        let mut sampled = 0;
        let mut result: Result<(), RenderError> = Ok(());
        scene.for_each(|id, geometry, pose| {
            if result.is_err() || !include(id, geometry, pose) {
                return;
            }
            result = (|| {
                let instance = InstancePose::new(pose);
                let transform = ObjectProjection {
                    camera: &camera,
                    instance,
                };
                let mut scale = camera.projection.scale;
                match geometry {
                    Geometry::Sampled(asset) => {
                        if sampled == self.sampled.len() {
                            self.sampled
                                .try_reserve_exact(1)
                                .map_err(|_| RenderError::MissingWorkspace)?;
                            self.sampled.push(ViewStack::default());
                        }
                        let stack = transform.stack(Fixed::from_ratio(asset.spacing_q8(), 256));
                        let stack = LayerGrid::prepared(asset).with_depth_extent(
                            stack,
                            asset.extent()[2],
                            &camera,
                        );
                        self.sampled[sampled].prepare(asset, stack, &camera.projection)?;
                        sampled += 1;
                    }
                    Geometry::Prepared(asset) if !camera.projection.perspective.is_zero() => {
                        scale = LayerGrid::prepared(asset).nearest_scale(
                            &transform,
                            asset.extent()[2],
                            Fixed::from_ratio(asset.spacing_q8(), 256),
                        );
                    }
                    Geometry::Sprite(asset) if !camera.projection.perspective.is_zero() => {
                        let d = asset.get().descriptor();
                        let [x, z] = d.pivot_q8();
                        let (w, h) = d.canvas();
                        scale = LayerGrid::new([w, h], [x, z, 0]).nearest_scale(
                            &transform,
                            d.layers().len() as u16,
                            Fixed::from_ratio(d.spacing_q8(), 256),
                        );
                    }
                    _ => {}
                }
                self.objects
                    .try_reserve(1)
                    .map_err(|_| RenderError::MissingWorkspace)?;
                self.objects.push((id, instance, scale));
                Ok(())
            })();
        });
        result?;
        self.sampled.truncate(sampled);
        Ok(ProjectedObjects {
            scene,
            camera,
            pass: self,
        })
    }
}

/// A borrowed set of projected objects. Every draw tests and updates the supplied scene depth tile.
pub struct ProjectedObjects<'a> {
    scene: &'a StackScene,
    camera: ViewProjection,
    pass: &'a ObjectPass,
}

impl ProjectedObjects<'_> {
    pub fn draw(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        let mut sampled = self.pass.sampled.iter();
        for &(id, instance, scale) in &self.pass.objects {
            let transform = ObjectProjection {
                camera: &self.camera,
                instance,
            };
            self.scene
                .with_object(id, |geometry, _| -> Result<(), RenderError> {
                    match geometry {
                        Geometry::Sprite(sprite) => {
                            draw_atlas(tile, &transform, sprite.get(), scale)?
                        }
                        Geometry::Prepared(sprite) => draw_sparse(tile, &transform, sprite, scale),
                        Geometry::Sampled(sprite) => {
                            if let Some(view) =
                                sampled.next().expect("prepared scene instance").view()
                            {
                                let palette = sprite.palette();
                                view.draw(tile, &self.camera.projection, |s| {
                                    u32::from_le_bytes(palette[usize::from(s.color)])
                                });
                            }
                        }
                        Geometry::Field(field) => draw_field(tile, &transform, field.get()),
                        Geometry::LiveField(field) => {
                            use mirui::core::model::ModelHandle;
                            field.read(|field| draw_field(tile, &transform, field))
                        }
                    }
                    Ok(())
                })
                .expect("borrowed scene retains its instances")?;
        }
        Ok(())
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
        let [right, forward, up] = object.stack(Fixed::ONE).axes;
        Self {
            object,
            edge: up[1].is_zero().then_some([right[1], forward[1]]),
        }
    }

    fn cell(&self, tile: &mut SceneCanvas<'_, '_>, at: V3, dx: Fixed, dz: Fixed, color: u32) {
        let Some([depth_x, depth_z]) = self.edge else {
            return top(self.object, tile, at, dx, dz, color);
        };
        if !self.object.camera.projection.slope.is_zero()
            || self.object.camera.projection.perspective != Fixed64::ZERO
        {
            top(self.object, tile, at, dx, dz, color);
        }
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

impl LayerGrid {
    fn with_depth_extent(
        self,
        mut stack: StackTransform,
        layers: u16,
        view: &ViewProjection,
    ) -> StackTransform {
        if !view.projection.perspective.is_zero() {
            let radius = |pivot: Fixed, length: Fixed| pivot.abs().max((length - pivot).abs());
            let extent = [
                radius(self.pivot[0], self.size[0].into()),
                radius(self.pivot[1], self.size[1].into()),
                radius(self.layer_pivot, Fixed::from(layers.saturating_sub(1)))
                    * stack.layer_spacing
                    + Fixed::HALF,
            ];
            stack.depth_radius = stack
                .axes
                .into_iter()
                .zip(extent)
                .map(|(axis, r)| axis[1].abs() * r)
                .fold(Fixed::ZERO, |a, b| a + b);
        }
        stack
    }
    fn nearest_scale(self, object: &ObjectProjection<'_>, layers: u16, spacing: Fixed) -> Fixed {
        let stack = self.with_depth_extent(object.stack(spacing), layers, object.camera);
        let projection = &object.camera.projection;
        let denominator = Fixed::ONE + projection.apply_perspective(stack.origin[1])
            - projection.apply_perspective(stack.depth_radius).abs();
        projection.scale / denominator.max(crate::raster::Projection::NEAR_DENOMINATOR)
    }

    /// Upright quarter turns map each local axis to one world axis. The orthographic
    /// projection is monotone along each of those axes, including fixed-point rounding.
    fn intersects_band(
        self,
        slice: &SliceProjection<'_, '_>,
        mut elevations: [Fixed; 2],
        clip: [i32; 4],
    ) -> bool {
        let t = slice.object;
        if t.camera.projection.perspective != Fixed64::ZERO
            || t.pitch != (Fixed::ZERO, Fixed::ONE)
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
            .chain(t.position)
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
        let [right, forward] = self.at(Fixed::from(x), Fixed::from(z));
        slice.cell(
            tile,
            [right, elevation, forward],
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
    scale: Fixed,
) -> Result<(), RenderError> {
    let slice = SliceProjection::new(t);
    let d = asset.descriptor();
    let [px, pz] = d.pivot_q8();
    let spacing = Fixed::from_ratio(d.spacing_q8(), 256);
    let (w, h) = d.canvas();
    let grid = LayerGrid::new([w, h], [px, pz, 0]);
    let top = spacing * (d.layers().len() - 1) as i32;
    if !grid.intersects_band(&slice, [Fixed::ZERO, top], tile.color.clip) {
        return Ok(());
    }
    if let Some((levels, palette)) = asset.levels
        && let Some(level) = (0..levels.len())
            .rev()
            .filter_map(|i| levels.level(i))
            .find(|l| crate::raster::sprites::fits_level(scale, l.step(), Fixed::ONE))
    {
        for i in 0..d.layers().len() {
            let elevation = grid.elevation(i as u16, spacing);
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
    let image = asset.atlas().read_pixels()?;
    for i in 0..d.layers().len() {
        let region = d.layers().get(i).expect("validated layer");
        let elevation = grid.elevation(i as u16, spacing);
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
    scale: Fixed,
) {
    let slice = SliceProjection::new(t);
    let [_, _, layers] = asset.extent();
    let spacing = Fixed::from_ratio(asset.spacing_q8(), 256);
    let grid = LayerGrid::prepared(asset);
    let level = crate::raster::sprites::select_level(asset, scale, Fixed::ONE);
    for i in 0..layers {
        let samples = level.layer(usize::from(i)).expect("validated layer");
        if samples.is_empty() {
            continue;
        }
        let elevation = grid.elevation(i, spacing);
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
    let x = t.camera.origin[0] - t.position[0];
    let z = t.camera.origin[2] - t.position[2];
    let mut center = [x * cy - z * sy, x * sy + z * cy];
    let upright = t.pitch == (Fixed::ZERO, Fixed::ONE) && t.roll == (Fixed::ZERO, Fixed::ONE);
    if !upright {
        let y = t.camera.origin[1] - t.position[1];
        let (sr, cr) = t.roll;
        let (sp, cp) = t.pitch;
        let right = center[0];
        center = [
            right * cr + y * sr,
            (y * cr - right * sr) * sp + center[1] * cp,
        ];
    }
    let radius = t.camera.radius;
    if !field.has_finite_local_bounds() && t.camera.pitch == (Fixed::ZERO, Fixed::ONE) && upright {
        use crate::raster::terrain::{TerrainSurface, TerrainView};
        // Ray sampling increases depth toward negative Z. Scene-space forward points toward positive Z.
        struct FieldPlane<'a>(&'a HeightField);
        impl TerrainSurface for FieldPlane<'_> {
            type Sample = usize;
            fn height_at(&self, [x, z]: [Fixed; 2]) -> Option<(Fixed, usize)> {
                TerrainSurface::height_at(&self.0, [x, -z])
            }
            fn colors(&self, [x, z]: [Fixed; 2], height: Fixed, sample: usize) -> [u32; 2] {
                self.0.colors([x, -z], height, sample)
            }
        }
        let (s, c) = t.camera.yaw;
        TerrainView {
            projection: t.camera.projection,
            origin: [center[0], -center[1]],
            altitude: t.camera.origin[1] - t.position[1],
            yaw: (c * sy - s * cy, c * cy + s * sy),
            heights: field.height_bounds(),
            depth: [-radius, radius],
        }
        .draw_layer(tile, &mut FieldPlane(field));
        return;
    }
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
    let cols = span(0);
    let rows = span(1);
    if !field.has_finite_local_bounds() {
        let [x, y, r, b] = tile.color.bounds;
        let area = |span: &core::ops::RangeInclusive<i32>| {
            (i64::from(*span.end()) - i64::from(*span.start()) + 1).max(0)
        };
        // A large repeated field has many cells per pixel. Intersect its visible
        // rays instead of projecting every cell in the entire camera range per tile.
        if area(&cols).saturating_mul(area(&rows)) > i64::from(r - x) * i64::from(b - y) {
            draw_field_rays(tile, t, field, [cols, rows]);
            return;
        }
    }
    for row in rows {
        for col in cols.clone() {
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

fn draw_field_rays(
    tile: &mut SceneCanvas<'_, '_>,
    t: &ObjectProjection<'_>,
    field: &HeightField,
    cells: [core::ops::RangeInclusive<i32>; 2],
) {
    let grid = field.grid();
    let cell = Fixed64::from(grid.cell_size());
    let rays = t.field_rays();
    let [xs, zs] = cells.map(|r| {
        [
            cell * i64::from(*r.start()),
            cell * (i64::from(*r.end()) + 1),
        ]
    });
    let bounds = [xs, field.height_bounds().map(Fixed64::from), zs];
    let [l, top, r, bottom] = tile.color.clip;
    for y in top..bottom {
        for x in l..r {
            let (origin, direction) = rays.through(x, y);
            let depth = [
                rays.depth[0],
                rays.depth[1].min(tile.depth[tile.color.index(x, y)].into()),
            ];
            let Some((z, i)) = field_hit(field, cell, bounds, origin, direction, depth) else {
                continue;
            };
            let c = field.color(field.materials()[i]);
            tile.write_visible(x, y, z, u32::from_le_bytes([c.r, c.g, c.b, c.a]));
        }
    }
}

/// Traverse the columns in front-to-back order. Each cell has a top and four
/// walls down to the grid base. Its underside is open.
fn field_hit(
    field: &HeightField,
    cell: Fixed64,
    bounds: [[Fixed64; 2]; 3],
    origin: [Fixed64; 3],
    direction: [Fixed64; 3],
    [mut near, mut far]: [Fixed64; 2],
) -> Option<(Fixed, usize)> {
    let mut wall_at_entry = false;
    for i in 0..3 {
        let d = direction[i];
        let [low, high] = bounds[i];
        if d.is_zero() {
            if origin[i] < low || origin[i] > high {
                return None;
            }
            continue;
        }
        let a = (low - origin[i]) / d;
        let b = (high - origin[i]) / d;
        let entry = a.min(b);
        if entry > near {
            near = entry;
            wall_at_entry = i != 1;
        } else if entry == near && i != 1 {
            wall_at_entry = true;
        }
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    // At a boundary the entering cell depends on the direction of travel.
    let mut cells = [0_i64; 2];
    let mut steps = [0_i64; 2];
    let mut next = [far; 2];
    for (j, i) in [0, 2].into_iter().enumerate() {
        let d = direction[i];
        let v = origin[i] + near.mul_wide(d);
        let mut index = (v / cell).to_int();
        let edge = cell * index;
        if edge > v || (d.is_negative() && edge == v) {
            index -= 1;
        }
        let lo = (bounds[i][0] / cell).to_int();
        let hi = (bounds[i][1] / cell).to_int() - 1;
        cells[j] = index.clamp(lo, hi);
        if !d.is_zero() {
            steps[j] = if d.is_positive() { 1 } else { -1 };
            let edge = cells[j] + i64::from(d.is_positive());
            next[j] = ((cell * edge - origin[i]) / d).max(near);
        }
    }
    let base = bounds[1][0];
    let mut at = near;
    loop {
        let end = far.min(next[0].min(next[1]));
        let sample = |j: usize| (cell * cells[j] + cell / 2).to_fixed();
        if let Some(i) = field.address_at(sample(0), sample(1)) {
            let height = Fixed64::from(field.grid().elevation(field.levels()[i]));
            let y = origin[1] + at.mul_wide(direction[1]);
            if wall_at_entry && y >= base && y <= height && height > base {
                return Some((at.to_fixed(), i));
            }
            if !direction[1].is_zero() {
                let top = (height - origin[1]) / direction[1];
                if (at..=end).contains(&top) {
                    return Some((top.to_fixed(), i));
                }
            }
            let crosses_wall =
                (steps[0] != 0 && end == next[0]) || (steps[1] != 0 && end == next[1]);
            if crosses_wall && height > base {
                let y = origin[1] + end.mul_wide(direction[1]);
                if y >= base && y <= height {
                    return Some((end.to_fixed(), i));
                }
            }
        }
        if end >= far {
            return None;
        }
        for j in 0..2 {
            if next[j] == end {
                cells[j] += steps[j];
                let i = j * 2;
                let edge = cells[j] + i64::from(steps[j] > 0);
                next[j] = ((cell * edge - origin[i]) / direction[i]).max(end);
            }
        }
        at = end;
        wall_at_entry = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Camera, Pose, TerrainGrid,
        raster::{Frame, Workspace},
    };

    #[test]
    fn many_tilted_repetitions_are_sampled_only_along_visible_rays() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static SAMPLES: AtomicUsize = AtomicUsize::new(0);
        let field = HeightField::from_planes(
            TerrainGrid::repeating([1, 1], Fixed::ONE).unwrap(),
            &[0][..],
            &[99][..],
        )
        .unwrap()
        .with_coordinate_map(|p| {
            SAMPLES.fetch_add(1, Ordering::Relaxed);
            p
        });
        let size = [32, 21];
        let color = u32::from_le_bytes([99, 99, 99, 255]);
        for perspective in [Fixed64::ZERO, Fixed64::from_ratio(1, 100)] {
            let mut camera = ViewProjection::from_camera(
                Camera {
                    range: Fixed::from_int(1800),
                    ..Camera::default()
                },
                size,
            )
            .unwrap();
            camera.projection.perspective = perspective;
            let object = ObjectProjection::new(
                &camera,
                Pose {
                    pitch: Fixed::from_int(17),
                    yaw: Fixed::from_int(24),
                    roll: Fixed::from_int(-9),
                    ..Pose::default()
                },
            );
            let mut frame = Frame::new(size[0], size[1]).unwrap();
            let mut workspace = Workspace::new(size[0], size[1]).unwrap();
            let mut reference = Frame::new(size[0], size[1]).unwrap();
            let mut expected_depth = alloc::vec::Vec::new();
            workspace.with_canvas(&mut reference, |canvas| {
                canvas.clear_depth();
                top(
                    &object,
                    canvas,
                    [(-32).into(), 0.into(), (-32).into()],
                    64.into(),
                    64.into(),
                    color,
                );
                expected_depth.extend_from_slice(canvas.depth);
            });
            SAMPLES.store(0, Ordering::Relaxed);
            workspace.with_canvas(&mut frame, |canvas| {
                canvas.clear_depth();
                draw_field(canvas, &object, &field);
                for (i, expected) in expected_depth.iter().enumerate() {
                    if *expected != Fixed::MAX {
                        assert_eq!(canvas.get(i), color);
                        assert!(
                            (canvas.depth[i] - *expected).abs() <= Fixed::ONE,
                            "index {i}"
                        );
                    }
                }
            });
            let count = SAMPLES.load(Ordering::Relaxed);
            assert!(count > 0 && count <= size[0] * size[1], "{count}");
            let mut tiled = Frame::new(size[0], size[1]).unwrap();
            let mut bands = Workspace::new(size[0], 4).unwrap();
            bands
                .render_rows(&mut tiled, |canvas| -> Result<(), RenderError> {
                    canvas.clear_depth();
                    draw_field(canvas, &object, &field);
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                tiled.texture.read_pixels().unwrap().bytes(),
                frame.texture.read_pixels().unwrap().bytes()
            );
        }
    }

    #[test]
    fn tilted_field_range_is_centered_in_its_own_grid() {
        let field = HeightField::from_planes(
            TerrainGrid::finite([16, 128], Fixed::ONE).unwrap(),
            alloc::vec![0; 16 * 128],
            alloc::vec![99; 16 * 128],
        )
        .unwrap();
        let camera = ViewProjection::from_camera(
            Camera {
                center: [8.into(), 100.into(), 0.into()],
                pitch: 0.into(),
                range: 4.into(),
                ..Camera::default()
            },
            [8, 8],
        )
        .unwrap();
        let object = ObjectProjection::new(
            &camera,
            Pose {
                pitch: 90.into(),
                ..Pose::default()
            },
        );
        let mut frame = Frame::new(8, 8).unwrap();
        let mut work = Workspace::new(8, 8).unwrap();
        work.with_canvas(&mut frame, |tile| {
            tile.clear_depth();
            draw_field(tile, &object, &field);
            assert_eq!(
                tile.get(tile.color.index(4, 4)),
                u32::from_le_bytes([99, 99, 99, 255])
            );
        });
    }

    #[test]
    fn field_rays_intersect_top_and_wall_in_both_directions_and_respect_depth() {
        let f = HeightField::from_planes(
            TerrainGrid::repeating([2, 1], Fixed::ONE).unwrap(),
            &[0, 3][..],
            &[10, 20][..],
        )
        .unwrap();
        let z = Fixed64::ZERO;
        let one = Fixed64::ONE;
        let half = one / 2;
        let bounds = [[-one * 3, one * 3], [z, one * 3], [-one, one]];
        let hit = |origin, dir, end| field_hit(&f, one, bounds, origin, dir, [z, end]);
        assert_eq!(
            hit([half, one, half], [one, z, z], one * 4),
            Some((Fixed::HALF, 1))
        );
        assert_eq!(
            hit([half, one, half], [-one, z, z], one * 4),
            Some((Fixed::HALF, 1))
        );
        assert_eq!(hit([half, one, half], [one, z, z], one / 4), None);
        assert_eq!(hit([one + half, one, half], [z, one, z], one / 4), None);
        assert_eq!(
            hit([one + half, -one, half], [z, one, z], one * 8),
            Some((4.into(), 1))
        );
        assert_eq!(
            hit([half, one * 5, half], [z, -one, z], one * 8),
            Some((5.into(), 0))
        );
    }

    #[test]
    fn band_culling_preserves_rasterized_cells_at_fractional_and_edge_on_views() {
        let size = [49, 71];
        let grid = LayerGrid {
            size: [9, 5],
            pivot: [Fixed::from_ratio(7, 2), Fixed::from_ratio(5, 4)],
            layer_pivot: Fixed::ZERO,
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
            let view = ViewProjection::from_camera(
                Camera {
                    center: [Fixed::HALF; 3],
                    ..camera
                },
                size,
            )
            .unwrap();
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
