//! Camera-space sampling and coverage for sparse indexed sprite layers.
use super::{Projection, SceneCanvas};
use crate::{PreparedSpriteStack, SparseLevel, StackSample};
use alloc::vec::Vec;
use core::ops::Range;
use mirui::render::RenderError;
use mirui::types::{Fixed, Fixed64, GridBasis};

/// Paints the completed sprite coverage once per pixel, allowing palette lighting, a silhouette outline, and opacity over the scene.
pub struct SpritePaint {
    /// Subtracted from camera depth for both visibility tests and writes.
    pub depth_bias: Fixed,
    /// Uniform opacity of the filled material. Zero skips the entire sprite.
    pub opacity: Fixed,
    /// Opaque, one-pixel silhouette color.
    pub outline: Option<u32>,
}

/// Camera coordinates use right, depth and up. The three unit model axes point right, forward and up in these coordinates. Scale gives world units per model cell.
pub struct StackTransform {
    pub origin: [Fixed; 3],
    pub axes: [[Fixed; 3]; 3],
    pub scale: Fixed,
    pub layer_spacing: Fixed,
    /// Camera-depth radius used to choose level of detail and raster sampling density, in world units.
    pub depth_radius: Fixed,
}
impl StackTransform {
    /// Rotates the object-to-camera offset and the model's unit axes into camera space. Translation applies only to the origin.
    pub fn from_basis(
        offset: [Fixed; 3],
        model_axes: [[Fixed; 3]; 3],
        into_camera: impl Fn([Fixed; 3]) -> [Fixed; 3],
    ) -> Self {
        Self {
            origin: into_camera(offset),
            axes: model_axes.map(into_camera),
            scale: Fixed::ONE,
            layer_spacing: Fixed::ONE,
            depth_radius: Fixed::ZERO,
        }
    }
}

/// Maps MIRX cells and layer indices into local model coordinates for either coverage method.
#[derive(Clone, Copy)]
pub(crate) struct LayerGrid {
    pub(crate) size: [u16; 2],
    pub(crate) pivot: [Fixed; 2],
    pub(crate) layer_pivot: Fixed,
}
impl LayerGrid {
    pub(crate) fn new(size: [u16; 2], pivot: [i32; 3]) -> Self {
        let [x, z, layer_pivot] = pivot.map(|q8| Fixed::from_ratio(q8, 256));
        Self {
            size,
            pivot: [x, z],
            layer_pivot,
        }
    }
    pub(crate) fn prepared(asset: &PreparedSpriteStack<'_>) -> Self {
        let [w, h, _] = asset.extent();
        Self::new([w, h], asset.pivot_q8())
    }
    pub(crate) fn at(self, x: Fixed, y: Fixed) -> [Fixed; 2] {
        [x - self.pivot[0], self.pivot[1] - y]
    }
    pub(crate) fn elevation(self, layer: u16, spacing: Fixed) -> Fixed {
        (Fixed::from(layer) - self.layer_pivot) * spacing
    }
}

/// Returns the coarsest model level whose cell fits within the allowed projected width.
pub fn select_level<'a>(
    asset: &PreparedSpriteStack<'a>,
    scale: Fixed,
    max_pixels: Fixed,
) -> SparseLevel<'a> {
    asset.levels()[select_matching_level(asset, scale, max_pixels, |_| true)]
}

fn select_matching_level(
    asset: &PreparedSpriteStack<'_>,
    scale: Fixed,
    max_pixels: Fixed,
    accepts_step: impl Fn(u16) -> bool,
) -> usize {
    asset
        .levels()
        .iter()
        .rposition(|level| {
            fits_level(scale, level.step(), max_pixels) && accepts_step(level.step())
        })
        .unwrap_or(0)
}

pub(crate) fn fits_level(scale: Fixed, step: u16, max_pixels: Fixed) -> bool {
    scale.is_positive() && scale <= max_pixels / i32::from(step)
}

#[derive(Clone, Copy)]
struct PreparedLayer {
    camera: [Fixed; 3],
    drop: Fixed,
    bounds: Option<[[i16; 4]; 2]>,
}

/// Bounded per-instance work for a sampled sprite. Every tile of a frame borrows the same projection, sample footprint and ordered layers.
pub struct ProjectedStack<'a, const N: usize> {
    core: ProjectionWork,
    level: &'a SparseLevel<'a>,
    layers: [Option<PreparedLayer>; N],
    order: [usize; N],
    offsets: [[Fixed; 2]; 64],
}
impl<'a, const N: usize> ProjectedStack<'a, N> {
    /// Projects a model with up to N layers. A layer offset moves its samples vertically in screen pixels. Returning None hides the layer.
    pub fn new(
        model: &PreparedSpriteStack<'a>,
        transform: StackTransform,
        p: &Projection,
        max_cell_pixels: Fixed,
        layer_offset: impl FnMut(usize, usize) -> Option<Fixed>,
    ) -> Option<Self> {
        let len = usize::from(model.extent()[2]);
        let mut layers = [None; N];
        let mut order = [0; N];
        let mut offsets = [[Fixed::ZERO; 2]; 64];
        let core = ProjectionWork::new(
            model,
            transform,
            p,
            max_cell_pixels,
            ProjectionBuffers {
                layers: layers.get_mut(..len)?,
                order: order.get_mut(..len)?,
                offsets: &mut offsets,
            },
            layer_offset,
        )?;
        Some(Self {
            level: &model.levels()[core.level],
            core,
            layers,
            order,
            offsets,
        })
    }
    pub fn level(&self) -> SparseLevel<'a> {
        *self.level
    }
    /// Screen x, y, reference scale and camera depth of the pivot. A hidden pivot uses the near-plane scale.
    pub fn center(&self) -> [Fixed; 4] {
        self.core.center
    }
    pub fn prepare_regions(&mut self, p: &Projection) {
        self.core
            .prepare_regions(*self.level, &mut self.layers[..self.level.layer_count()], p);
    }
    /// Borrows the prepared instance for drawing any number of tiles.
    pub fn view(&self) -> SpriteView<'_, 'a> {
        let count = self.level.layer_count();
        SpriteView {
            core: &self.core,
            level: self.level,
            layers: &self.layers[..count],
            order: &self.order[..count],
            offsets: SampleOffsets::Wide(&self.offsets[..self.core.samples * self.core.samples]),
        }
    }
}

struct PreparedStack {
    core: Option<ProjectionWork>,
    layers: Range<usize>,
    offsets: Range<usize>,
}

#[derive(Clone, Copy)]
enum SampleOffsets<'a> {
    Wide(&'a [[Fixed; 2]]),
    Packed(&'a [[i16; 2]]),
}

enum OffsetPool {
    Packed(Vec<[i16; 2]>),
    Wide(Vec<[Fixed; 2]>),
}

// A signed sample coordinate with 1/256-pixel precision spans [-128, 128).
const SAMPLE_FRACTIONS: i32 = 256;

impl Default for OffsetPool {
    fn default() -> Self {
        Self::Packed(Vec::new())
    }
}

impl OffsetPool {
    fn append(
        &mut self,
        offsets: &[[Fixed; 2]],
        [min, max]: [[Fixed; 2]; 2],
    ) -> Result<Range<usize>, RenderError> {
        const MIN: Fixed = Fixed::from_int(-128);
        const END: Fixed = Fixed::from_int(128);
        if let Self::Packed(pool) = self {
            if min.iter().all(|&v| v >= MIN) && max.iter().all(|&v| v < END) {
                let start = pool.len();
                pool.try_reserve_exact(offsets.len())
                    .map_err(|_| RenderError::MissingWorkspace)?;
                pool.extend(
                    offsets
                        .iter()
                        .map(|offset| offset.map(|v| (v * SAMPLE_FRACTIONS).to_int() as i16)),
                );
                return Ok(start..pool.len());
            }
            let mut wide = Vec::new();
            let capacity = pool
                .capacity()
                .max(pool.len().saturating_add(offsets.len()));
            wide.try_reserve_exact(capacity)
                .map_err(|_| RenderError::MissingWorkspace)?;
            wide.extend(pool.iter().copied().map(expand_offset));
            *self = Self::Wide(wide);
        }
        let Self::Wide(pool) = self else {
            unreachable!()
        };
        let start = pool.len();
        pool.try_reserve_exact(offsets.len())
            .map_err(|_| RenderError::MissingWorkspace)?;
        pool.extend_from_slice(offsets);
        Ok(start..pool.len())
    }

    fn clear(&mut self) {
        match self {
            Self::Packed(pool) => pool.clear(),
            Self::Wide(pool) => pool.clear(),
        }
    }

    fn offsets(&self, range: Range<usize>) -> SampleOffsets<'_> {
        match self {
            Self::Packed(pool) => SampleOffsets::Packed(&pool[range]),
            Self::Wide(pool) => SampleOffsets::Wide(&pool[range]),
        }
    }

    fn storage_bytes(&self) -> usize {
        match self {
            Self::Packed(pool) => pool.capacity() * size_of::<[i16; 2]>(),
            Self::Wide(pool) => pool.capacity() * size_of::<[Fixed; 2]>(),
        }
    }
}

fn expand_offset(offset: [i16; 2]) -> [Fixed; 2] {
    offset.map(|v| Fixed::from_ratio(i32::from(v), SAMPLE_FRACTIONS))
}

/// Per-viewport projection storage shared by all sampled instances.
#[derive(Default)]
pub(crate) struct ViewStacks {
    stacks: Vec<PreparedStack>,
    layers: Vec<Option<PreparedLayer>>,
    order: Vec<usize>,
    offsets: OffsetPool,
}
impl ViewStacks {
    pub(crate) fn storage_bytes(&self) -> usize {
        self.stacks.capacity() * size_of::<PreparedStack>()
            + self.layers.capacity() * size_of::<Option<PreparedLayer>>()
            + self.order.capacity() * size_of::<usize>()
            + self.offsets.storage_bytes()
    }

    pub(crate) fn reset(&mut self) {
        self.stacks.clear();
        self.offsets.clear();
    }

    pub(crate) fn release_layers(&mut self) {
        self.reset();
        self.layers = Vec::new();
        self.order = Vec::new();
        self.offsets = OffsetPool::default();
    }

    pub(crate) fn push(
        &mut self,
        asset: &PreparedSpriteStack<'_>,
        transform: StackTransform,
        p: &Projection,
    ) -> Result<(), RenderError> {
        let count = usize::from(asset.extent()[2]);
        self.stacks
            .try_reserve_exact(1)
            .map_err(|_| RenderError::MissingWorkspace)?;
        let start = self.stacks.last().map_or(0, |stack| stack.layers.end);
        let end = start
            .checked_add(count)
            .ok_or(RenderError::MissingWorkspace)?;
        if end > self.layers.len() {
            let missing = end - self.layers.len();
            self.layers
                .try_reserve_exact(missing)
                .map_err(|_| RenderError::MissingWorkspace)?;
            self.order
                .try_reserve_exact(missing)
                .map_err(|_| RenderError::MissingWorkspace)?;
            self.layers.resize(end, None);
            self.order.resize(end, 0);
        }
        let range = start..end;
        let layers = &mut self.layers[range.clone()];
        let mut offsets = [[Fixed::ZERO; 2]; 64];
        let core = ProjectionWork::new(
            asset,
            transform,
            p,
            Fixed::ONE,
            ProjectionBuffers {
                layers,
                order: &mut self.order[range.clone()],
                offsets: &mut offsets,
            },
            |_, _| Some(Fixed::ZERO),
        );
        let offsets = if let Some(core) = core.as_ref() {
            core.prepare_regions(asset.levels()[core.level], layers, p);
            let samples = &offsets[..core.samples * core.samples];
            self.offsets.append(samples, core.sample_bounds)?
        } else {
            0..0
        };
        self.stacks.push(PreparedStack {
            core,
            layers: range,
            offsets,
        });
        Ok(())
    }

    pub(crate) fn view<'a>(
        &self,
        index: usize,
        asset: &'a PreparedSpriteStack<'a>,
    ) -> Option<SpriteView<'_, 'a>> {
        let stack = &self.stacks[index];
        let core = stack.core.as_ref()?;
        Some(SpriteView {
            core,
            level: &asset.levels()[core.level],
            layers: &self.layers[stack.layers.clone()],
            order: &self.order[stack.layers.clone()],
            offsets: self.offsets.offsets(stack.offsets.clone()),
        })
    }
}

/// Borrows the prepared sample positions, layer order and visibility for one instance.
pub struct SpriteView<'s, 'a> {
    core: &'s ProjectionWork,
    level: &'s SparseLevel<'a>,
    layers: &'s [Option<PreparedLayer>],
    order: &'s [usize],
    offsets: SampleOffsets<'s>,
}
impl SpriteView<'_, '_> {
    /// Visits each covered sample that passes the scene depth test, in layer order. The callback can shade the fragment directly or record coverage for a subsequent pass.
    pub fn rasterize(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        p: &Projection,
        depth_bias: Fixed,
        mut fragment: impl FnMut(&mut SceneCanvas<'_, '_>, usize, &StackSample, Fixed),
    ) -> [i32; 4] {
        let clip = canvas.color.clip;
        let mut bounds = [clip[2], clip[3], clip[0] - 1, clip[1] - 1];
        self.visit(clip, p, |x, y, sample, depth| {
            let at = canvas.color.index(x, y);
            if canvas.depth[at] >= depth - depth_bias {
                fragment(canvas, at, sample, depth);
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
        });
        bounds
    }

    fn visit(
        &self,
        clip: [i32; 4],
        p: &Projection,
        sample: impl FnMut(i32, i32, &StackSample, Fixed),
    ) {
        mirui::trace_span!("sprite_stack.samples");
        match self.offsets {
            SampleOffsets::Wide(offsets) => {
                self.visit_offsets(clip, p, offsets.iter().copied(), sample)
            }
            SampleOffsets::Packed(offsets) => {
                self.visit_offsets(clip, p, offsets.iter().copied().map(expand_offset), sample)
            }
        }
    }

    fn visit_offsets(
        &self,
        clip: [i32; 4],
        p: &Projection,
        offsets: impl Iterator<Item = [Fixed; 2]> + Clone,
        mut sample: impl FnMut(i32, i32, &StackSample, Fixed),
    ) {
        let center = self.core.center;
        {
            for &index in self.order {
                let Some(layer) = &self.layers[index] else {
                    continue;
                };
                let voxels = self.level.layer(index).unwrap_or_default();
                let voxels = if let Some(bands) = layer.bounds {
                    let [left, top, right, bottom] = clip;
                    let visible = bands.map(|[x0, y0, x1, y1]| {
                        i32::from(x0) < right
                            && (x1 == i16::MAX || i32::from(x1) > left)
                            && i32::from(y0) < bottom
                            && (y1 == i16::MAX || i32::from(y1) > top)
                    });
                    let mid = voxels.len() / 2;
                    match visible {
                        [false, false] => continue,
                        [true, false] => &voxels[..mid],
                        [false, true] => &voxels[mid..],
                        [true, true] => voxels,
                    }
                } else {
                    voxels
                };
                for voxel in voxels {
                    let Some([x, y, m, depth]) = self.core.project(layer, voxel, p) else {
                        continue;
                    };
                    let xs = self.core.sample_ends(x, 0, m);
                    if xs[0].max(xs[1]) < clip[0] || xs[0].min(xs[1]) >= clip[2] {
                        continue;
                    }
                    let ys = self.core.sample_ends(y, 1, m);
                    if ys[0].max(ys[1]) < clip[1] || ys[0].min(ys[1]) >= clip[3] {
                        continue;
                    }
                    // Coincident extrema make that coordinate identical for every offset.
                    let fixed_x = xs[0] == xs[1];
                    let fixed_y = ys[0] == ys[1];
                    let mut mark = |xx, yy| {
                        if xx >= clip[0] && yy >= clip[1] && xx < clip[2] && yy < clip[3] {
                            sample(xx, yy, voxel, depth);
                        }
                    };
                    // Both extrema are sampled. On a single row or column they cover
                    // the entire footprint if they are at most one pixel apart.
                    if (fixed_x && ys[0].abs_diff(ys[1]) <= 1)
                        || (fixed_y && xs[0].abs_diff(xs[1]) <= 1)
                    {
                        mark(xs[0], ys[0]);
                        if !(fixed_x && fixed_y) {
                            mark(xs[1], ys[1]);
                        }
                        continue;
                    }
                    for offset in offsets.clone() {
                        let xx = if fixed_x {
                            xs[0]
                        } else {
                            ((x + offset[0] * m / center[2]).round()).trunc_to_int()
                        };
                        if xx < clip[0] || xx >= clip[2] {
                            continue;
                        }
                        let yy = if fixed_y {
                            ys[0]
                        } else {
                            ((y + offset[1] * m / center[2]).round()).trunc_to_int()
                        };
                        mark(xx, yy);
                    }
                }
            }
        }
    }

    /// Draws opaque fragments in a caller-supplied material, updating shared depth.
    pub fn draw(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: &Projection,
        mut shade: impl FnMut(&StackSample) -> u32,
    ) {
        self.rasterize(
            canvas,
            projection,
            Fixed::ZERO,
            |tile, at, sample, depth| {
                tile.depth[at] = depth;
                tile.put(at, shade(sample));
            },
        );
    }

    /// Shades the nearest nonzero material tag per pixel. Zero tags are holes. Material and outline colors are opaque. Opacity fades the material over the existing scene; submit fading surfaces after opaque geometry in back-to-front order.
    pub fn draw_masked(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: &Projection,
        paint: SpritePaint,
        tag: impl Fn(&StackSample) -> u8,
        mut shade: impl FnMut(u8) -> u32,
    ) {
        let SpritePaint {
            opacity,
            outline,
            depth_bias,
        } = paint;
        if !opacity.is_positive() {
            return;
        }
        canvas.mask.fill(0);
        let [l, t, r, b] = canvas.color.clip;
        let halo = i32::from(outline.is_some());
        let clip = [l - halo, t - halo, r + halo, b + halo];
        let mut bounds = [r, b, l - 1, t - 1];
        let mut missing = [clip[2], clip[3], clip[0], clip[1]];
        self.visit(clip, projection, |x, y, sample, depth| {
            let material = tag(sample);
            if material == 0 {
                return;
            }
            if canvas.color.contains(x, y) {
                let at = canvas.color.index(x, y);
                let z = depth - depth_bias;
                if z <= canvas.depth[at] {
                    canvas.depth[at] = z;
                    canvas.mask[at] = material;
                    bounds = [
                        bounds[0].min(x),
                        bounds[1].min(y),
                        bounds[2].max(x),
                        bounds[3].max(y),
                    ];
                    return;
                }
                if canvas.mask[at] != 0 {
                    return;
                }
            }
            if outline.is_some() {
                missing = [
                    missing[0].min(x),
                    missing[1].min(y),
                    missing[2].max(x + 1),
                    missing[3].max(y + 1),
                ];
            }
        });
        for y in (bounds[1] - halo).max(t)..=(bounds[3] + halo).min(b - 1) {
            for x in (bounds[0] - halo).max(l)..=(bounds[2] + halo).min(r - 1) {
                let at = canvas.color.index(x, y);
                let material = canvas.mask[at];
                if material == 0 {
                    if let Some(outline) = outline {
                        let pitch = canvas.color.pitch;
                        let mut nearest = Fixed::MAX;
                        let mut neighbor = |n: usize| {
                            if canvas.mask[n] != 0 {
                                nearest = nearest.min(canvas.depth[n]);
                            }
                        };
                        if x > l {
                            neighbor(at - 1);
                        }
                        if x + 1 < r {
                            neighbor(at + 1);
                        }
                        if y > t {
                            neighbor(at - pitch);
                        }
                        if y + 1 < b {
                            neighbor(at + pitch);
                        }
                        if nearest < canvas.depth[at] {
                            canvas.depth[at] = nearest;
                            canvas.put(at, outline);
                        }
                    }
                    continue;
                }
                let mut color = shade(material);
                if opacity < Fixed::ONE {
                    let [r, g, b, a] = canvas.get(at).to_le_bytes();
                    let background = mirui::types::Color::rgba(r, g, b, a);
                    let [r, g, b, _] = color.to_le_bytes();
                    let material = mirui::types::Color::rgb(r, g, b);
                    let mixed = if a == 255 {
                        mirui::types::Color::lerp(background, material, opacity)
                    } else {
                        let alpha = (opacity * 255).round().to_int() as u8;
                        mirui::render::texture::blend_straight_source_over(
                            &material,
                            &background,
                            alpha,
                        )
                    };
                    color = u32::from_le_bytes([mixed.r, mixed.g, mixed.b, mixed.a]);
                }
                canvas.put(at, color);
            }
        }
        let Some(outline) = outline.filter(|_| missing[0] < missing[2]) else {
            return;
        };
        mirui::trace_span!("sprite_stack.outline");
        // A clipped or occluded center can still contribute a visible contour on
        // its neighbor. Fully visible sprites already resolved all four neighbors.
        self.visit(missing, projection, |x, y, sample, depth| {
            if canvas.color.contains(x, y) && canvas.mask[canvas.color.index(x, y)] != 0 {
                return;
            }
            if tag(sample) == 0 {
                return;
            }
            let z = depth - depth_bias;
            for (x, y) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if !canvas.color.contains(x, y) {
                    continue;
                }
                let at = canvas.color.index(x, y);
                if canvas.mask[at] == 0 && z < canvas.depth[at] {
                    canvas.depth[at] = z;
                    canvas.put(at, outline);
                }
            }
        });
    }
}
struct ProjectionWork {
    center: [Fixed; 4],
    grid: GridBasis,
    samples: usize,
    sample_bounds: [[Fixed; 2]; 2],
    level: usize,
}

struct ProjectionBuffers<'a> {
    layers: &'a mut [Option<PreparedLayer>],
    order: &'a mut [usize],
    offsets: &'a mut [[Fixed; 2]; 64],
}
impl ProjectionWork {
    fn new(
        model: &PreparedSpriteStack<'_>,
        transform: StackTransform,
        projection: &Projection,
        max_cell_pixels: Fixed,
        ProjectionBuffers {
            layers,
            order,
            offsets,
        }: ProjectionBuffers<'_>,
        mut layer_offset: impl FnMut(usize, usize) -> Option<Fixed>,
    ) -> Option<Self> {
        let [cu, cv, cw] = transform.origin;
        let sc = transform.scale;
        let p = projection;
        let [width, length, layer_count] = model.extent();
        let count = usize::from(layer_count);
        if count != layers.len() || count != order.len() || !sc.is_positive() {
            return None;
        }
        let [right, forward, up] = transform.axes.map(|axis| axis.map(|v| sc * v));
        // A model can cross the near plane even when its pivot is behind it.
        // The pivot scale normalizes the footprint; each cell still clips at its own depth.
        let center = p.project_camera(cu, cw, cv).unwrap_or_else(|| {
            let m = p.scale / Fixed::from_ratio(2, 10);
            let [x, y] = p.screen_point(cu, cw + p.slope * cv, m);
            [x, y, m, cv]
        });
        if !center[2].is_positive() {
            return None;
        }
        let geometry = LayerGrid::prepared(model);
        let spacing = transform.layer_spacing;
        for (i, entry) in order.iter_mut().enumerate() {
            *entry = i;
        }
        if p.perspective.is_zero() {
            if p.slope * up[2] - up[1] < Fixed::ZERO {
                order.reverse();
            }
        } else {
            let distance = -cu * up[0]
                + (-p.divide_perspective(Fixed::ONE) - cv) * up[1]
                + (p.divide_perspective(p.slope) - cw) * up[2];
            let square = sc * sc;
            // Tiny model scales can round the squared scale to zero in Q24.8.
            let plane = if square.is_positive() {
                Fixed64::from(distance / square)
            } else {
                Fixed64::from(distance).div_wide(Fixed64::from(sc).mul_wide(Fixed64::from(sc)))
            };
            order.sort_unstable_by_key(|&i| {
                core::cmp::Reverse(
                    (Fixed64::from(geometry.elevation(i as u16, spacing)) - plane).abs(),
                )
            });
        }
        let big = sc * p.scale
            / (Fixed::ONE + p.apply_perspective(cv) - p.apply_perspective(transform.depth_radius))
                .max(Fixed::from_ratio(2, 10));
        // One sample footprint is shared by every cell. Choose a level that tiles
        // the entire model so an unpadded edge keeps its true width and height.
        let level_index = select_matching_level(model, big, max_cell_pixels, |step| {
            width.is_multiple_of(step) && length.is_multiple_of(step)
        });
        let level = &model.levels()[level_index];
        let cell_scale = Fixed::from(level.step());
        let cell_center = (cell_scale - Fixed::ONE) / 2;
        // Coarse cells reuse the existing grid over a larger footprint at lower density.
        let samples = ((big * Fixed::from_int(2) - Fixed::from_ratio(24, 100))
            .ceil()
            .clamp(Fixed::from_int(2), Fixed::from_int(8)))
        .trunc_to_int() as usize;
        let mut centers = [Fixed::ZERO; 8];
        for (i, center) in centers[..samples].iter_mut().enumerate() {
            *center = ((Fixed::from_int(i as i32) + Fixed::HALF) / samples as i32 - Fixed::HALF)
                * cell_scale;
        }
        for (i, &a) in centers[..samples].iter().enumerate() {
            for (k, &b) in centers[..samples].iter().enumerate() {
                offsets[i * samples + k] = [
                    center[2] * (a * right[0] + b * forward[0]),
                    -center[2]
                        * (a * (right[2] + p.slope * right[1])
                            + b * (forward[2] + p.slope * forward[1])),
                ];
            }
        }
        let mut sample_bounds = [offsets[0]; 2];
        for offset in &offsets[1..samples * samples] {
            for axis in 0..2 {
                sample_bounds[0][axis] = sample_bounds[0][axis].min(offset[axis]);
                sample_bounds[1][axis] = sample_bounds[1][axis].max(offset[axis]);
            }
        }
        let first_slice = (0..count)
            .find(|i| level.layer(*i).is_some_and(|cells| !cells.is_empty()))
            .unwrap_or(count);
        let [grid_x, grid_y] = geometry.at(cell_center, cell_center);
        let grid: [Fixed; 3] = core::array::from_fn(|i| grid_x * right[i] + grid_y * forward[i]);
        for (layer, prepared) in layers.iter_mut().enumerate() {
            let Some(drop) = layer_offset(layer, first_slice) else {
                *prepared = None;
                continue;
            };
            let height = geometry.elevation(layer as u16, spacing);
            let bu = cu + height * up[0];
            let bv = cv + height * up[1];
            let bw = cw + height * up[2];
            *prepared = Some(PreparedLayer {
                camera: [bu + grid[0], bv + grid[1], bw + grid[2]],
                drop,
                bounds: None,
            });
        }
        Some(Self {
            center,
            grid: GridBasis::new(right, forward.map(|step| -step)),
            samples,
            sample_bounds,
            level: level_index,
        })
    }

    /// Computes conservative screen bounds for each half of a layer. Assets can supply baked cell bounds or scan the borrowed samples.
    fn prepare_regions(
        &self,
        level: SparseLevel<'_>,
        layers: &mut [Option<PreparedLayer>],
        p: &Projection,
    ) {
        mirui::trace_span!("sprite_stack.regions");
        for (index, work) in layers.iter_mut().enumerate() {
            let Some(layer) = *work else {
                continue;
            };
            let bounds = level
                .layer_bounds(index)
                .expect("validated layer")
                .map(|coords| {
                    if coords[0][0] > coords[0][1] {
                        Self::EMPTY_BAND
                    } else {
                        self.project_band(&layer, p, coords)
                    }
                });
            *work = Some(PreparedLayer {
                bounds: Some(bounds),
                ..layer
            });
        }
    }
    const EMPTY_BAND: [i16; 4] = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];

    fn project_band(
        &self,
        layer: &PreparedLayer,
        p: &Projection,
        [x, y]: [[u8; 2]; 2],
    ) -> [i16; 4] {
        let axis_range = |axis: usize| {
            self.grid
                .axis_range(layer.camera[axis], axis, x.map(i32::from), y.map(i32::from))
        };
        let depth = axis_range(1);
        let den = depth.map(|d| Fixed::ONE + p.apply_perspective(d));
        let near = Fixed::from_ratio(2, 10);
        if den[1] < near {
            return Self::EMPTY_BAND;
        }
        // A layer crossing the near plane keeps per-voxel clipping.
        if den[0] < near {
            return [i16::MIN, i16::MIN, i16::MAX, i16::MAX];
        }
        let scale = [p.scale / den[1], p.scale / den[0]];
        // Scale stays positive; signs select which endpoint gives each extreme.
        let scaled = |[lo, hi]: [Fixed; 2]| {
            [
                lo * scale[usize::from(lo < Fixed::ZERO)],
                hi * scale[usize::from(hi >= Fixed::ZERO)],
            ]
        };
        let u = scaled(axis_range(0));
        let z = axis_range(2);
        let [s0, s1] = depth.map(|d| p.slope * d);
        let v = scaled([z[0] + s0.min(s1), z[1] + s0.max(s1)]);
        let position = [
            [p.center[0] + u[0], p.center[0] + u[1]],
            [
                p.center[1] - v[1] + layer.drop,
                p.center[1] - v[0] + layer.drop,
            ],
        ];
        let mut bounds = [0; 4];
        for axis in 0..2 {
            let offset = scaled(self.sample_bounds.map(|bound| bound[axis]));
            let ends = core::array::from_fn::<_, 2, _>(|i| {
                (position[axis][i] + offset[i] / self.center[2])
                    .round()
                    .trunc_to_int()
            });
            bounds[axis] = ends[0];
            bounds[axis + 2] = ends[1].saturating_add(1);
        }
        // A saturated upper endpoint stays open when testing a wide viewport.
        bounds.map(|x| x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
    }

    fn project(
        &self,
        layer: &PreparedLayer,
        voxel: &StackSample,
        p: &Projection,
    ) -> Option<[Fixed; 4]> {
        let [bu, bv, bw] = layer.camera;
        let x = i32::from(voxel.x);
        let y = i32::from(voxel.y);
        let depth = self.grid.axis(bv, 1, x, y);
        let den = Fixed::ONE + p.apply_perspective(depth);
        if den < Fixed::from_ratio(2, 10) {
            return None;
        }
        let m = p.scale / den;
        Some([
            p.center[0] + m * self.grid.axis(bu, 0, x, y),
            p.center[1] - m * (self.grid.axis(bw, 2, x, y) + p.slope * depth) + layer.drop,
            m,
            depth,
        ])
    }

    fn sample_ends(&self, position: Fixed, axis: usize, scale: Fixed) -> [i32; 2] {
        self.sample_bounds.map(|bound| {
            (position + bound[axis] * scale / self.center[2])
                .round()
                .trunc_to_int()
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const CELLS: &[StackSample] = &[
        StackSample {
            x: 0,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 4,
            y: 4,
            color: 1,
        },
        StackSample {
            x: 1,
            y: 3,
            color: 0,
        },
        StackSample {
            x: 3,
            y: 2,
            color: 1,
        },
    ];
    const MIP: &[StackSample] = &[
        StackSample {
            x: 0,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 2,
            y: 2,
            color: 1,
        },
    ];
    const LEVELS: &[SparseLevel<'static>] = &[
        SparseLevel::new(1, &[0, 2, 4], CELLS).unwrap(),
        SparseLevel::new(2, &[0, 1, 2], MIP).unwrap(),
    ];
    const MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [6, 6, 2],
        [256, 512, 128],
        256,
        &[[0, 0, 0, 255], [255; 4]],
        LEVELS,
    )
    .unwrap();

    #[test]
    fn viewport_footprints_preserve_wide_and_packed_sample_positions() {
        let mut work = ViewStacks::default();
        let mut first = [[Fixed::ZERO; 2]; 64];
        let mut first_len = 0;
        for pixels in [3, 400] {
            let (p, _) = fixture(Fixed::from_int(pixels), false);
            let transform = || StackTransform {
                origin: [Fixed::ZERO; 3],
                axes: [
                    [Fixed::ONE, Fixed::ZERO, Fixed::ZERO],
                    [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
                    [Fixed::ZERO, Fixed::ONE, Fixed::ZERO],
                ],
                scale: Fixed::ONE,
                layer_spacing: Fixed::ONE,
                depth_radius: Fixed::ZERO,
            };
            let direct = ProjectedStack::<2>::new(&MODEL, transform(), &p, Fixed::ONE, |_, _| {
                Some(Fixed::ZERO)
            })
            .unwrap();
            let index = work.stacks.len();
            work.push(&MODEL, transform(), &p).unwrap();
            let expected = &direct.offsets[..direct.core.samples * direct.core.samples];
            let offsets = work.view(index, &MODEL).unwrap().offsets;
            match offsets {
                SampleOffsets::Packed(offsets) => {
                    assert_eq!(pixels, 3);
                    assert!(
                        offsets
                            .iter()
                            .copied()
                            .map(expand_offset)
                            .eq(expected.iter().copied())
                    );
                    first_len = offsets.len();
                    first[..first_len].copy_from_slice(expected);
                }
                SampleOffsets::Wide(offsets) => {
                    assert_eq!(pixels, 400);
                    assert_eq!(offsets, expected);
                }
            }
        }
        let SampleOffsets::Wide(reused) = work.view(0, &MODEL).unwrap().offsets else {
            panic!("wide pool must retain the prior offsets");
        };
        assert_eq!(reused, &first[..first_len]);
        work.release_layers();
        assert_eq!(work.offsets.storage_bytes(), 0);
    }

    fn fixture(scale: Fixed, perspective: bool) -> (Projection, ProjectedStack<'static, 2>) {
        let p = Projection {
            scale,
            center: [Fixed::from_int(64); 2],
            perspective: if perspective {
                mirui::types::Fixed64::from_ratio(1, 64)
            } else {
                mirui::types::Fixed64::ZERO
            },
            slope: Fixed::from_ratio(2, 3),
        };
        let a = Fixed::from_ratio(3, 5);
        let b = Fixed::from_ratio(4, 5);
        let transform = StackTransform {
            origin: [Fixed::ZERO; 3],
            axes: [
                [a, -b, Fixed::ZERO],
                [b, a, Fixed::ZERO],
                [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
            ],
            scale: Fixed::ONE,
            layer_spacing: Fixed::ONE,
            depth_radius: Fixed::from_int(3),
        };
        let sprite = ProjectedStack::new(&MODEL, transform, &p, Fixed::from_int(3), |_, _| {
            Some(Fixed::ZERO)
        })
        .unwrap();
        (p, sprite)
    }
    #[test]
    fn solid_samples_keep_all_palette_indices_and_respect_scene_depth() {
        use super::super::{Frame, Workspace};
        const CELLS: &[StackSample] = &[
            StackSample {
                x: 0,
                y: 0,
                color: 0,
            },
            StackSample {
                x: 3,
                y: 0,
                color: 255,
            },
        ];
        const INDEX: crate::format::IndexedLevel<'static, 1> =
            SparseLevel::new(1, &[0, 2], CELLS).unwrap().indexed();
        const MODEL: PreparedSpriteStack<'static> =
            PreparedSpriteStack::new([4, 1, 1], [0; 3], 256, &[[255; 4]; 256], &[INDEX.level()])
                .unwrap();
        let projection = Projection {
            scale: Fixed::from_int(3),
            slope: Fixed::ZERO,
            perspective: Fixed64::ZERO,
            center: [Fixed::from_int(4), Fixed::from_int(8)],
        };
        let mut sprite = ProjectedStack::<1>::new(
            &MODEL,
            StackTransform {
                origin: [Fixed::ZERO; 3],
                axes: [
                    [Fixed::ONE, Fixed::ZERO, Fixed::ZERO],
                    [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
                    [Fixed::ZERO, Fixed::ONE, Fixed::ZERO],
                ],
                scale: Fixed::ONE,
                layer_spacing: Fixed::ONE,
                depth_radius: Fixed::ONE,
            },
            &projection,
            Fixed::ONE,
            |_, _| Some(Fixed::ZERO),
        )
        .unwrap();
        sprite.prepare_regions(&projection);
        let mut frame = Frame::new(20, 16).unwrap();
        let mut work = Workspace::new(20, 16).unwrap();
        work.with_rows(&mut frame, 0, 16, |tile| {
            tile.clear_depth();
            let shade = |s: &StackSample| if s.color == 0 { 0xff0000ff } else { 0xff00ff00 };
            sprite.view().draw(tile, &projection, shade);
            let red = tile.color.index(4, 8);
            let green = tile.color.index(13, 8);
            assert_eq!([tile.get(red), tile.get(green)], [0xff0000ff, 0xff00ff00]);
            assert_eq!([tile.depth[red], tile.depth[green]], [Fixed::ZERO; 2]);
            tile.depth[red] = -Fixed::ONE;
            sprite.view().draw(tile, &projection, |_| u32::MAX);
            assert_eq!([tile.get(red), tile.get(green)], [0xff0000ff, u32::MAX]);
        });
    }

    #[test]
    fn masked_surfaces_share_object_depth_and_contours_across_tiles() {
        use super::super::{Frame, Workspace};
        const VISIBLE: u32 = 0xff33_44ff;
        const OTHER: u32 = 0xffee_8833;
        const OUTLINE: u32 = 0xff10_1010;
        const SKY: u32 = 0xff11_2266;
        // The last sample shares a pixel with the first and maps to a hole.
        const SAMPLES: &[StackSample] = &[
            StackSample {
                x: 0,
                y: 0,
                color: 0,
            },
            StackSample {
                x: 2,
                y: 0,
                color: 0,
            },
            StackSample {
                x: 0,
                y: 1,
                color: 1,
            },
        ];
        const SHAPE: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
            [3, 2, 1],
            [0; 3],
            256,
            &[[255; 4]; 2],
            &[SparseLevel::new(1, &[0, 3], SAMPLES).unwrap()],
        )
        .unwrap();
        let projection = Projection {
            scale: Fixed::ONE,
            slope: Fixed::ZERO,
            perspective: Fixed64::ZERO,
            center: [Fixed::from_int(5), Fixed::from_int(5)],
        };
        let sprite = |start: i32, slope: i32| {
            ProjectedStack::<1>::new(
                &SHAPE,
                StackTransform {
                    origin: [Fixed::ZERO, Fixed::from_int(start), Fixed::ZERO],
                    axes: [
                        [Fixed::ONE, Fixed::from_int(slope), Fixed::ZERO],
                        [Fixed::ZERO; 3],
                        [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
                    ],
                    scale: Fixed::ONE,
                    layer_spacing: Fixed::ONE,
                    depth_radius: Fixed::ONE,
                },
                &projection,
                Fixed::ONE,
                |_, _| Some(Fixed::ZERO),
            )
            .unwrap()
        };
        let a = sprite(3, -2);
        let b = sprite(1, 2);
        let render = |rows: usize, reverse: bool| {
            let mut frame = Frame::new(12, 12).unwrap();
            let mut work = Workspace::new(12, rows).unwrap();
            let mut pixels = [(0u32, Fixed::MAX); 144];
            work.render_rows(&mut frame, |tile| {
                tile.clear_depth();
                for at in 0..tile.depth.len() {
                    tile.put(at, SKY);
                }
                // At the outline this third object is nearer. A further opaque
                // fragment drawn after the outline will also have to pass depth.
                tile.write_visible(5, 4, Fixed::from_int(-3), SKY);
                let draw =
                    |tile: &mut SceneCanvas<'_, '_>, s: &ProjectedStack<'_, 1>, color: u32| {
                        s.view().draw_masked(
                            tile,
                            &projection,
                            SpritePaint {
                                depth_bias: Fixed::ZERO,
                                opacity: Fixed::ONE,
                                outline: Some(OUTLINE),
                            },
                            |v| u8::from(v.color == 0),
                            |_| color,
                        );
                    };
                if reverse {
                    draw(tile, &b, OTHER);
                    draw(tile, &a, VISIBLE);
                } else {
                    draw(tile, &a, VISIBLE);
                    draw(tile, &b, OTHER);
                }
                tile.write_visible(7, 6, Fixed::from_int(20), SKY);
                let [l, t, r, b] = tile.color.clip;
                for y in t..b {
                    for x in l..r {
                        let at = tile.color.index(x, y);
                        pixels[y as usize * 12 + x as usize] = (tile.get(at), tile.depth[at]);
                    }
                }
                Ok(())
            })
            .unwrap();
            pixels
        };
        let expected = render(12, false);
        assert_eq!(expected, render(12, true));
        assert_eq!(expected, render(1, false));
        assert_eq!(expected, render(3, true));
        let pixel = |x: usize, y: usize| expected[y * 12 + x];
        assert_eq!(pixel(5, 5), (OTHER, Fixed::ONE));
        assert_eq!(pixel(7, 5), (VISIBLE, -Fixed::ONE));
        assert_eq!(pixel(5, 4), (SKY, Fixed::from_int(-3)));
        assert_eq!(pixel(7, 6), (OUTLINE, -Fixed::ONE));
        assert_eq!(pixel(8, 8), (SKY, Fixed::MAX));

        // A visible, fading surface blends once even on a one-row tile.
        let mut frame = Frame::new(12, 12).unwrap();
        let mut work = Workspace::new(12, 1).unwrap();
        work.with_rows(&mut frame, 5, 1, |tile| {
            tile.clear_depth();
            let at = tile.color.index(5, 5);
            tile.put(at, 0xff00_0000);
            for opacity in [Fixed::ZERO, Fixed::HALF] {
                a.view().draw_masked(
                    tile,
                    &projection,
                    SpritePaint {
                        depth_bias: Fixed::ONE,
                        opacity,
                        outline: None,
                    },
                    |v| u8::from(v.color == 0),
                    |_| u32::MAX,
                );
                if opacity == Fixed::ZERO {
                    assert_eq!((tile.get(at), tile.depth[at]), (0xff00_0000, Fixed::MAX));
                }
            }
            assert_eq!(tile.depth[at], Fixed::from_int(2));
            assert_eq!(tile.get(at), 0xff7f_7f7f);
            tile.clear_depth();
            tile.put(at, 0);
            a.view().draw_masked(
                tile,
                &projection,
                SpritePaint {
                    depth_bias: Fixed::ZERO,
                    opacity: Fixed::HALF,
                    outline: None,
                },
                |v| u8::from(v.color == 0),
                |_| 0xff33_22ff,
            );
            assert_eq!(tile.get(at), 0x8033_22ff);
        });
    }

    #[test]
    fn prepared_regions_cover_all_samples() {
        assert!(core::mem::size_of::<PreparedLayer>() <= 36);
        let mut details = [false; 2];
        for scale in [Fixed::HALF, Fixed::from_int(3), Fixed::from_int(8)] {
            for perspective in [false, true] {
                let (projection, mut prepared) = fixture(scale, perspective);
                details[usize::from(prepared.level().step() > 1)] = true;
                assert!(prepared.layers.iter().flatten().all(|l| l.bounds.is_none()));
                let original = prepared.layers;
                prepared.prepare_regions(&projection);
                for (index, layer) in original.iter().enumerate() {
                    let Some(layer) = layer else { continue };
                    let voxels = prepared.level().layer(index).unwrap();
                    for (i, voxel) in voxels.iter().enumerate() {
                        let Some([x, y, scale, _]) =
                            prepared.core.project(layer, voxel, &projection)
                        else {
                            continue;
                        };
                        let bounds = prepared.layers[index]
                            .expect("visible layer")
                            .bounds
                            .expect("prepared layer")[usize::from(i >= voxels.len() / 2)]
                        .map(i32::from);
                        for offset in
                            &prepared.offsets[..prepared.core.samples * prepared.core.samples]
                        {
                            for (axis, position) in [x, y].into_iter().enumerate() {
                                let at = (position
                                    + offset[axis] * scale / prepared.core.center[2])
                                    .round()
                                    .trunc_to_int();
                                assert!(at >= bounds[axis] && at < bounds[axis + 2]);
                            }
                        }
                    }
                }
            }
        }
        assert!(details.into_iter().all(|seen| seen));
    }
    #[test]
    fn near_plane_rejects_only_wholly_hidden_model_bands() {
        let (mut p, mut jet) = fixture(Fixed::ONE, true);
        p.perspective = mirui::types::Fixed64::from_ratio(1, 64);
        jet.core.grid = GridBasis::new([Fixed::ZERO, Fixed::ONE, Fixed::ZERO], [Fixed::ZERO; 3]);
        let coords = [[0, 16], [0; 2]];
        let bounds = |depth| {
            let layer = PreparedLayer {
                camera: [Fixed::ZERO, Fixed::from_int(depth - 8), Fixed::ZERO],
                drop: Fixed::ZERO,
                bounds: None,
            };
            jet.core.project_band(&layer, &p, coords)
        };
        assert_eq!(bounds(-80), ProjectionWork::EMPTY_BAND);
        assert_eq!(bounds(-51), [i16::MIN, i16::MIN, i16::MAX, i16::MAX]);
        let [left, top, right, bottom] = bounds(0);
        assert!(left < right && top < bottom);
        assert!(left > i16::MIN && right < i16::MAX);
    }
    #[test]
    fn visible_cells_survive_a_hidden_pivot_and_a_wide_virtual_viewport() {
        use super::super::{Frame, Workspace};
        const SAMPLE: &[StackSample] = &[StackSample {
            x: 24,
            y: 0,
            color: 0,
        }];
        const MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
            [25, 1, 1],
            [0; 3],
            256,
            &[[255; 4]],
            &[SparseLevel::new(1, &[0, 1], SAMPLE).unwrap()],
        )
        .unwrap();
        let p = Projection {
            center: [Fixed::from_int(40_000), Fixed::from_int(8)],
            scale: Fixed::ONE,
            perspective: Fixed64::from_ratio(1, 64),
            slope: Fixed::ZERO,
        };
        let t = StackTransform {
            origin: [Fixed::ZERO, -Fixed::from_int(65), Fixed::ZERO],
            axes: [
                [Fixed::ZERO, Fixed::ONE, Fixed::ZERO],
                [Fixed::ONE, Fixed::ZERO, Fixed::ZERO],
                [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
            ],
            scale: Fixed::ONE,
            layer_spacing: Fixed::ONE,
            depth_radius: Fixed::from_int(25),
        };
        let mut sprite =
            ProjectedStack::<1>::new(&MODEL, t, &p, Fixed::ONE, |_, _| Some(Fixed::ZERO)).unwrap();
        sprite.prepare_regions(&p);
        let mut frame = Frame::new(16, 16).unwrap();
        let mut work = Workspace::new(16, 16).unwrap();
        work.with_region(
            &mut frame,
            [50_000, 16],
            [39_992, 0],
            [39_992, 0, 40_008, 16],
            |tile| {
                tile.clear_depth();
                sprite.view().draw(tile, &p, |_| u32::MAX);
                assert_eq!(tile.get(tile.color.index(40_000, 8)), u32::MAX);
            },
        );
    }
    #[test]
    fn sample_bounds_never_reject_a_sample_inside_the_region() {
        for perspective in [false, true] {
            let (_, prepared) = fixture(Fixed::from_int(3), perspective);
            for scale in [1, 63, 256, 512, 1024] {
                let scale = Fixed::from_ratio(scale, 256);
                for axis in 0..2 {
                    for raw in (-32 * 256..160 * 256).step_by(67) {
                        let position = Fixed::from_ratio(raw, 256);
                        for clip in [[0, 0, 32, 32], [31, 47, 63, 79], [127, 127, 128, 128]] {
                            let ends = prepared.core.sample_ends(position, axis, scale);
                            if ends[0] == ends[1] {
                                for offset in &prepared.offsets
                                    [..prepared.core.samples * prepared.core.samples]
                                {
                                    assert_eq!(
                                        (position + offset[axis] * scale / prepared.core.center[2])
                                            .round()
                                            .trunc_to_int(),
                                        ends[0]
                                    );
                                }
                            }
                            if ends[0].max(ends[1]) < clip[axis]
                                || ends[0].min(ends[1]) >= clip[axis + 2]
                            {
                                assert!(
                                    prepared.offsets
                                        [..prepared.core.samples * prepared.core.samples]
                                        .iter()
                                        .all(|offset| {
                                            let at = (position
                                                + offset[axis] * scale / prepared.core.center[2])
                                                .round()
                                                .trunc_to_int();
                                            at < clip[axis] || at >= clip[axis + 2]
                                        })
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
