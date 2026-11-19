//! Fixed-point scene projection and reusable color, coverage and depth tiles.
mod mapping;
mod visibility;
pub use mapping::{RollMapping, sheet_side};
pub use visibility::{DepthSamples, fully_covered};

use alloc::vec::Vec;
use mirui::render::{
    renderer::RenderError,
    texture::{ColorFormat, Texture},
};
use mirui::types::{Fixed, Fixed64};

/// Camera-space projection. A zero reciprocal camera distance gives an orthographic view.
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub perspective: Fixed64,
    pub slope: Fixed,
    pub scale: Fixed,
    pub center: [Fixed; 2],
}
impl Projection {
    pub fn apply_perspective(&self, value: Fixed) -> Fixed {
        (self.perspective * Fixed64::from_fixed(value)).to_fixed()
    }
    pub fn divide_perspective(&self, value: Fixed) -> Fixed {
        (Fixed64::from_fixed(value) / self.perspective).to_fixed()
    }
    /// Projects right, up and forward camera coordinates into x, y, pixel scale and depth.
    pub fn project_camera(&self, right: Fixed, up: Fixed, depth: Fixed) -> Option<[Fixed; 4]> {
        let denominator = Fixed::ONE + self.apply_perspective(depth);
        if denominator < Fixed::from_ratio(12, 100) {
            return None;
        }
        let scale = self.scale / denominator;
        Some([
            self.center[0] + scale * right,
            self.center[1] - scale * (up + self.slope * depth),
            scale,
            depth,
        ])
    }
}
/// Owned output texture. Canvas methods require an RGBA8888 frame.
pub struct Frame {
    pub texture: Texture<'static>,
    pub width: usize,
    pub height: usize,
}
/// Depth, coverage and span limits for the largest tile. Reused across frames.
pub struct Workspace {
    depth: Vec<Fixed>,
    mask: Vec<u8>,
    columns: Vec<i32>,
}
#[doc = " A short mutable borrow. No texture or allocator borrow survives the call."]
pub struct Canvas<'a> {
    pub bytes: &'a mut [u8],
    pub width: usize,
    pub height: usize,
    pub bounds: [i32; 4],
    pub clip: [i32; 4],
    pub pitch: usize,
    pub origin: [i32; 2],
    extent: [usize; 2],
}
/// Scoped color, depth and coverage for the same region of a virtual viewport.
pub struct SceneCanvas<'a, 'b> {
    pub color: &'a mut Canvas<'b>,
    pub depth: &'a mut [Fixed],
    pub mask: &'a mut [u8],
    pub columns: &'a mut [i32],
}
impl Frame {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        Self::with_format(width, height, ColorFormat::RGBA8888)
    }
    pub fn with_format(
        width: usize,
        height: usize,
        format: ColorFormat,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            texture: Texture::owned(
                u16::try_from(width).map_err(|_| RenderError::InvalidGeometry)?,
                u16::try_from(height).map_err(|_| RenderError::InvalidGeometry)?,
                format,
            )?
            .with_cache_revision(0),
            width,
            height,
        })
    }
    pub fn with_canvas<R>(&mut self, f: impl FnOnce(&mut Canvas<'_>) -> R) -> R {
        self.with_region(
            [self.width, self.height],
            [0, 0],
            [0, 0, self.width as i32, self.height as i32],
            f,
        )
    }
    pub fn with_region<R>(
        &mut self,
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        f: impl FnOnce(&mut Canvas<'_>) -> R,
    ) -> R {
        self.with_extent([self.width, self.height], virtual_size, origin, bounds, f)
    }
    /// Reinterprets the packed prefix of the backing storage as a tile at the virtual origin.
    /// The extent must fit the frame and any associated workspace.
    pub fn with_extent<R>(
        &mut self,
        extent: [usize; 2],
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        f: impl FnOnce(&mut Canvas<'_>) -> R,
    ) -> R {
        assert_eq!(self.texture.format(), ColorFormat::RGBA8888);
        assert!(
            extent[0]
                .checked_mul(extent[1])
                .is_some_and(|n| n <= self.width * self.height)
        );
        let mut pixels = self.texture.pixels_mut().expect("owned scene frame");
        let bytes = &mut pixels.bytes_mut()[..extent[0] * extent[1] * 4];
        let mut canvas = Canvas {
            bytes,
            width: virtual_size[0],
            height: virtual_size[1],
            bounds,
            clip: bounds,
            pitch: extent[0],
            origin,
            extent,
        };
        canvas.set_bounds(bounds);
        f(&mut canvas)
    }
    pub fn bytes(&self) -> usize {
        self.width * self.height * self.texture.format().bytes_per_pixel()
    }
}
fn reserved<T: Clone>(len: usize, value: T) -> Result<Vec<T>, RenderError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| RenderError::MissingWorkspace)?;
    values.resize(len, value);
    Ok(values)
}

impl Workspace {
    /// Borrows coverage storage between draws for foreground coverage tests.
    pub fn coverage_scratch(&mut self) -> &mut [u8] {
        &mut self.mask
    }
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        if width == 0 || height == 0 || width > u16::MAX as usize || height > u16::MAX as usize {
            return Err(RenderError::InvalidGeometry);
        }
        let count = width
            .checked_mul(height)
            .ok_or(RenderError::InvalidGeometry)?;
        Ok(Self {
            depth: reserved(count, Fixed::MAX)?,
            mask: reserved(count, 0)?,
            columns: reserved(width, height as i32)?,
        })
    }
    pub fn bytes(&self) -> usize {
        self.depth.capacity() * 4 + self.mask.capacity() + self.columns.capacity() * 4
    }
    pub fn with_canvas<R>(
        &mut self,
        frame: &mut Frame,
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        self.with_region(
            frame,
            [frame.width, frame.height],
            [0, 0],
            [0, 0, frame.width as i32, frame.height as i32],
            draw,
        )
    }
    pub fn with_region<R>(
        &mut self,
        frame: &mut Frame,
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        self.with_extent(
            frame,
            [frame.width, frame.height],
            virtual_size,
            origin,
            bounds,
            draw,
        )
    }
    /// Borrows consecutive output rows in place and reuses the depth tile for those rows.
    pub fn with_rows<R>(
        &mut self,
        frame: &mut Frame,
        y: usize,
        height: usize,
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        assert_eq!(frame.texture.format(), ColorFormat::RGBA8888);
        assert!(y <= frame.height && height <= frame.height - y);
        let width = frame.width;
        let count = height * width;
        let start = y * width * 4;
        let mut pixels = frame.texture.pixels_mut().expect("owned scene frame");
        let mut color = Canvas {
            bytes: &mut pixels.bytes_mut()[start..start + count * 4],
            width,
            height: frame.height,
            bounds: [0, 0, width as i32, frame.height as i32],
            clip: [0, y as i32, width as i32, (y + height) as i32],
            pitch: width,
            origin: [0, y as i32],
            extent: [width, height],
        };
        draw(&mut SceneCanvas {
            color: &mut color,
            depth: &mut self.depth[..count],
            mask: &mut self.mask[..count],
            columns: &mut self.columns[..width],
        })
    }

    /// Reinterprets the packed prefix of the backing storage as a tile at the virtual origin.
    /// The extent must fit the frame and any associated workspace.
    pub fn with_extent<R>(
        &mut self,
        frame: &mut Frame,
        extent: [usize; 2],
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        let count = extent[0] * extent[1];
        frame.with_extent(extent, virtual_size, origin, bounds, |color| {
            draw(&mut SceneCanvas {
                color,
                depth: &mut self.depth[..count],
                mask: &mut self.mask[..count],
                columns: &mut self.columns[..extent[0]],
            })
        })
    }
}
impl Canvas<'_> {
    pub fn index(&self, x: i32, y: i32) -> usize {
        (y - self.origin[1]) as usize * self.pitch + (x - self.origin[0]) as usize
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.clip[0] && y >= self.clip[1] && x < self.clip[2] && y < self.clip[3]
    }
    pub fn set_bounds(&mut self, bounds: [i32; 4]) {
        self.bounds = bounds;
        self.clip = [
            bounds[0].max(self.origin[0]).max(0),
            bounds[1].max(self.origin[1]).max(0),
            bounds[2]
                .min(self.origin[0].saturating_add(self.extent[0] as i32))
                .min(self.width as i32),
            bounds[3]
                .min(self.origin[1].saturating_add(self.extent[1] as i32))
                .min(self.height as i32),
        ];
    }
    pub fn get(&self, index: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[index * 4..index * 4 + 4]
                .try_into()
                .expect("RGBA pixel"),
        )
    }
    pub fn put(&mut self, index: usize, color: u32) {
        self.bytes[index * 4..index * 4 + 4].copy_from_slice(&color.to_le_bytes());
    }
    pub fn point(&mut self, x: i32, y: i32, color: u32) {
        if self.contains(x, y) {
            self.put(self.index(x, y), color);
        }
    }
}
impl SceneCanvas<'_, '_> {
    pub fn get(&self, index: usize) -> u32 {
        self.color.get(index)
    }
    pub fn put(&mut self, index: usize, color: u32) {
        self.color.put(index, color);
    }
    /// Writes a packed RGBA sample when it is closer than the last sample at the same pixel.
    pub fn write_visible(&mut self, x: i32, y: i32, depth: Fixed, color: u32) -> bool {
        if color >> 24 == 0 || !self.color.contains(x, y) {
            return false;
        }
        let index = self.color.index(x, y);
        if depth >= self.depth[index] {
            return false;
        }
        self.depth[index] = depth;
        self.put(index, color);
        true
    }
    /// Returns None while another tile owns the sample. Outside the virtual bounds the depth is unoccupied.
    pub fn sample_depth(&self, x: i32, y: i32) -> Option<Fixed> {
        let [left, top, right, bottom] = self.color.bounds;
        if x < left || y < top || x >= right || y >= bottom {
            Some(Fixed::MAX)
        } else if !self.color.contains(x, y) {
            None
        } else {
            Some(self.depth[self.color.index(x, y)])
        }
    }
    #[inline]
    pub fn fill_depth(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed) {
        self.fill_span(x, y0, y1, color, depth, false);
    }
    /// Fills unoccupied samples in a vertical span, optionally alternating pixels in a checker pattern.
    #[inline]
    pub fn fill_span(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed, thin: bool) {
        let [left, top, right, bottom] = self.color.clip;
        let mut first = y0.max(top);
        let end = y1.min(bottom);
        if x >= right.max(0) as usize || x < left.max(0) as usize || first >= end {
            return;
        }
        if thin && (x ^ first as usize) & 1 == 0 {
            first += 1;
            if first == end {
                return;
            }
        }
        let mut at = self.color.index(x as i32, first);
        let pixels = self.color.bytes.as_chunks_mut::<4>().0;
        let mut remaining = (end - first - 1) as usize >> usize::from(thin);
        loop {
            if self.depth[at] == Fixed::MAX {
                pixels[at] = color.to_le_bytes();
                self.depth[at] = depth;
            }
            if remaining == 0 {
                break;
            }
            remaining -= 1;
            at += self.color.pitch << usize::from(thin);
        }
    }
    pub fn clear_depth(&mut self) {
        if self.color.clip[0] >= self.color.clip[2] || self.color.clip[1] >= self.color.clip[3] {
            return;
        }
        for y in self.color.clip[1]..self.color.clip[3] {
            let start = self.color.index(self.color.clip[0], y);
            let end = start + (self.color.clip[2] - self.color.clip[0]) as usize;
            self.depth[start..end].fill(Fixed::MAX);
        }
        self.columns.fill(self.color.clip[3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiled_samples_share_depth_and_reject_outside_writes() {
        let mut frame = Frame::new(4, 4).unwrap();
        let mut workspace = Workspace::new(4, 4).unwrap();
        let bytes = workspace.bytes();
        // A tall strip uses the same packed storage as a square tile.
        workspace.with_extent(&mut frame, [2, 8], [8, 8], [3, 0], [1, 1, 7, 7], |tile| {
            tile.clear_depth();
            assert_eq!(tile.sample_depth(1, 1), None);
            assert_eq!(tile.sample_depth(0, 0), Some(Fixed::MAX));
            let near = Fixed::ONE;
            let far = Fixed::from_int(2);
            assert!(tile.write_visible(3, 2, far, 0xff010203));
            assert!(tile.write_visible(3, 2, near, 0xff040506));
            assert!(!tile.write_visible(3, 2, far, 0xff010203));
            assert!(!tile.write_visible(3, 2, Fixed::ZERO, 0));
            tile.fill_depth(3, 1, 7, 0xff070809, far);
            assert_eq!(tile.sample_depth(3, 2), Some(near));
            assert_eq!(tile.get(tile.color.index(3, 2)), 0xff040506);
            assert!(!tile.write_visible(2, 2, near, 0xffffffff));
            tile.clear_depth();
            assert_eq!(tile.sample_depth(3, 2), Some(Fixed::MAX));
        });
        assert_eq!(workspace.bytes(), bytes);
    }

    #[test]
    fn checker_spans_keep_global_parity_at_clipped_tile_edges() {
        let mut frame = Frame::new(3, 5).unwrap();
        let mut workspace = Workspace::new(3, 5).unwrap();
        let end = i32::MAX;
        workspace.with_region(
            &mut frame,
            [10, end as usize],
            [2, end - 5],
            [1, end - 4, 4, end],
            |tile| {
                let old = 0xff00ff00;
                let new = 0xff0000ff;
                assert!(tile.write_visible(2, end - 2, Fixed::ONE, old));
                for x in [usize::MAX, 1, 2, 3, 4] {
                    tile.fill_span(x, end - 7, end, new, Fixed::from_int(2), true);
                }
                for x in 2..5 {
                    for y in end - 5..end {
                        let expected = if (x, y) == (2, end - 2) {
                            old
                        } else if tile.color.contains(x, y) && (x ^ y) & 1 != 0 {
                            new
                        } else {
                            0
                        };
                        assert_eq!(tile.get(tile.color.index(x, y)), expected);
                    }
                }
            },
        );
    }

    #[test]
    fn projection_shares_orthographic_and_perspective_coordinates() {
        let mut p = Projection {
            perspective: Fixed64::ZERO,
            slope: Fixed::ZERO,
            scale: Fixed::from_int(2),
            center: [Fixed::from_int(4); 2],
        };
        let v = Fixed::ONE;
        assert_eq!(
            p.project_camera(v, v, v),
            Some([
                Fixed::from_int(6),
                Fixed::from_int(2),
                Fixed::from_int(2),
                v
            ])
        );
        p.perspective = Fixed64::ONE;
        assert_eq!(
            p.project_camera(v, v, v),
            Some([Fixed::from_int(5), Fixed::from_int(3), v, v])
        );
        assert!(p.project_camera(v, v, -v).is_none());
    }
}
