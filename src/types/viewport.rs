use super::{Fixed, PhysicalRect, Point, Rect, Transform};

/// Mapping from a widget's logical coordinate space to the physical
/// pixels of the backing surface — DPI scale today, with rotation and
/// sub-region projection reserved for future extension.
///
/// `scale = 1` means 1 logical pixel == 1 physical pixel; `scale = 2` is
/// a typical HiDPI desktop ratio. Widget-level 2D affine transforms are
/// a separate concern — see [`super::Transform`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    physical_w: u16,
    physical_h: u16,
    logical_w: u16,
    logical_h: u16,
    scale: Fixed,
}

#[inline]
fn inferred_logical_axis(physical: u16, scale: Fixed) -> u16 {
    (Fixed::from(physical) / scale)
        .to_int()
        .clamp(0, i32::from(u16::MAX)) as u16
}

impl Viewport {
    /// Construct. `scale <= 0` is normalized to 1 so downstream consumers
    /// never have to guard against a zero scale.
    #[inline]
    pub fn new(physical_w: u16, physical_h: u16, scale: Fixed) -> Self {
        let scale = if scale <= Fixed::ZERO {
            Fixed::ONE
        } else {
            scale
        };
        Self {
            physical_w,
            physical_h,
            logical_w: inferred_logical_axis(physical_w, scale),
            logical_h: inferred_logical_axis(physical_h, scale),
            scale,
        }
    }

    /// Keep the backend's authoritative logical size when physical pixels
    /// and a quantized scale cannot reconstruct it exactly.
    #[inline]
    pub fn with_logical_size(mut self, width: u16, height: u16) -> Self {
        self.logical_w = width;
        self.logical_h = height;
        self
    }

    #[inline]
    pub fn scale(&self) -> Fixed {
        self.scale
    }

    #[inline]
    pub fn physical_size(&self) -> (u16, u16) {
        (self.physical_w, self.physical_h)
    }

    #[inline]
    pub fn logical_size(&self) -> (u16, u16) {
        (self.logical_w, self.logical_h)
    }

    #[inline]
    pub fn point_to_physical(&self, p: Point) -> Point {
        Point {
            x: p.x * self.scale,
            y: p.y * self.scale,
        }
    }

    #[inline]
    pub fn rect_to_physical(&self, r: Rect) -> Rect {
        Rect {
            x: r.x * self.scale,
            y: r.y * self.scale,
            w: r.w * self.scale,
            h: r.h * self.scale,
        }
    }

    /// Convert a logical-pixel Rect to an integer physical-pixel bound
    /// `(x0, y0, x1, y1)`. Top-left floors, bottom-right ceils so the
    /// returned region fully contains the source.
    #[inline]
    pub fn rect_to_physical_pixel_bounds(&self, r: Rect) -> (i32, i32, i32, i32) {
        let x0 = (r.x * self.scale).to_int();
        let y0 = (r.y * self.scale).to_int();
        let x1 = ((r.x + r.w) * self.scale).ceil().to_int();
        let y1 = ((r.y + r.h) * self.scale).ceil().to_int();
        (x0, y0, x1, y1)
    }

    pub fn physical_rect(&self, logical: Rect) -> Option<PhysicalRect> {
        let (width, height) = self.physical_size();
        self.physical_rect_in(logical, u32::from(width), u32::from(height))
    }

    pub(crate) fn logical_cover(&self, physical: PhysicalRect) -> Rect {
        if physical.is_empty() {
            return Rect::ZERO;
        }
        if self.scale == Fixed::ONE {
            return Rect::new(
                physical.x(),
                physical.y(),
                physical.width(),
                physical.height(),
            );
        }
        let upper = |edge| {
            let edge = Fixed::from(edge);
            let value = (edge / self.scale).ceil();
            if value * self.scale < edge {
                value + Fixed::ONE
            } else {
                value
            }
        };
        let left = (Fixed::from(physical.x()) / self.scale).floor();
        let top = (Fixed::from(physical.y()) / self.scale).floor();
        Rect::new(
            left,
            top,
            upper(physical.right()) - left,
            upper(physical.bottom()) - top,
        )
    }

    pub(crate) fn physical_rect_in(
        &self,
        logical: Rect,
        target_width: u32,
        target_height: u32,
    ) -> Option<PhysicalRect> {
        if logical.w <= Fixed::ZERO || logical.h <= Fixed::ZERO {
            return None;
        }
        let (x0, y0, x1, y1) = self.rect_to_physical_pixel_bounds(logical);
        let width = target_width.min(u32::from(self.physical_w)) as i32;
        let height = target_height.min(u32::from(self.physical_h)) as i32;
        let left = x0.clamp(0, width);
        let top = y0.clamp(0, height);
        let right = x1.clamp(0, width);
        let bottom = y1.clamp(0, height);
        if right <= left || bottom <= top {
            return None;
        }
        PhysicalRect::new(
            u16::try_from(left).ok()?,
            u16::try_from(top).ok()?,
            u16::try_from(right - left).ok()?,
            u16::try_from(bottom - top).ok()?,
        )
    }

    #[inline]
    pub fn point_to_logical(&self, p: Point) -> Point {
        Point {
            x: p.x / self.scale,
            y: p.y / self.scale,
        }
    }

    #[inline]
    pub fn as_transform(&self) -> Transform {
        Transform::scale(self.scale, self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_mapping_preserves_identity_and_scaled_rounding() {
        for scale in [
            Fixed::ZERO,
            Fixed::HALF,
            Fixed::ONE,
            Fixed::from_ratio(3, 2),
            Fixed::from_int(2),
        ] {
            let viewport = Viewport::new(128, 96, scale).with_logical_size(127, 95);
            let scale = viewport.scale();
            for x in [-1001, -6, -1, 0, 1, 6, 255] {
                for y in [-7, 0, 1, 129] {
                    let point = Point::new(Fixed::from_ratio(x, 4), Fixed::from_ratio(y, 4));
                    assert_eq!(
                        viewport.point_to_physical(point),
                        Point::new(point.x * scale, point.y * scale)
                    );
                    assert_eq!(
                        viewport.point_to_logical(point),
                        Point::new(point.x / scale, point.y / scale)
                    );
                    for width in [Fixed::ZERO, Fixed::HALF, Fixed::from_ratio(41, 2)] {
                        let rect = Rect::new(point.x, point.y, width, Fixed::from_ratio(31, 2));
                        assert_eq!(
                            viewport.rect_to_physical(rect),
                            Rect::new(
                                rect.x * scale,
                                rect.y * scale,
                                rect.w * scale,
                                rect.h * scale
                            )
                        );
                        assert_eq!(
                            viewport.rect_to_physical_pixel_bounds(rect),
                            (
                                (rect.x * scale).to_int(),
                                (rect.y * scale).to_int(),
                                ((rect.x + rect.w) * scale).ceil().to_int(),
                                ((rect.y + rect.h) * scale).ceil().to_int(),
                            )
                        );
                    }
                }
            }
            for rect in [
                PhysicalRect::EMPTY,
                PhysicalRect::new(1, 3, 17, 11).unwrap(),
                PhysicalRect::from_size(128, 96),
            ] {
                let logical = viewport.logical_cover(rect);
                if rect.is_empty() {
                    assert_eq!(logical, Rect::ZERO);
                } else if scale == Fixed::ONE {
                    assert_eq!(
                        logical,
                        Rect::new(rect.x(), rect.y(), rect.width(), rect.height())
                    );
                    assert_eq!(viewport.physical_rect(logical), Some(rect));
                } else {
                    let (left, top, right, bottom) =
                        viewport.rect_to_physical_pixel_bounds(logical);
                    assert!(left <= i32::from(rect.x()) && top <= i32::from(rect.y()));
                    assert!(right >= i32::from(rect.right()) && bottom >= i32::from(rect.bottom()));
                }
            }
        }
    }

    #[test]
    fn zero_scale_is_normalized_to_one() {
        let t = Viewport::new(100, 50, Fixed::ZERO);
        assert_eq!(t.scale(), Fixed::ONE);
        assert_eq!(t.logical_size(), (100, 50));
    }

    #[test]
    fn logical_size_divides_physical() {
        let t = Viewport::new(200, 100, Fixed::from_int(2));
        assert_eq!(t.logical_size(), (100, 50));
    }

    #[test]
    fn explicit_logical_size_survives_quantized_scale() {
        let scale = Fixed::from_ratio(766, 256);
        let inferred = Viewport::new(1436, 958, scale);
        assert_eq!(inferred.logical_size(), (479, 320));
        let explicit = inferred.with_logical_size(480, 320);
        assert_eq!(explicit.logical_size(), (480, 320));
        assert_eq!(explicit.physical_size(), (1436, 958));
        assert_eq!(explicit.scale(), scale);
    }

    #[test]
    fn logical_size_saturates_instead_of_wrapping() {
        let t = Viewport::new(u16::MAX, u16::MAX, Fixed::from_ratio(1, 2));
        assert_eq!(t.logical_size(), (u16::MAX, u16::MAX));
    }

    #[test]
    fn point_roundtrip_within_fixed_precision() {
        let t = Viewport::new(200, 100, Fixed::from_int(2));
        let p = Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(20),
        };
        let phys = t.point_to_physical(p);
        assert_eq!(phys.x, Fixed::from_int(20));
        assert_eq!(phys.y, Fixed::from_int(40));
        let back = t.point_to_logical(phys);
        assert_eq!(back, p);
    }

    #[test]
    fn rect_bounds_ceil_bottom_right() {
        let t = Viewport::new(200, 100, Fixed::from_f32(1.5));
        let r = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        };
        let (x0, y0, x1, y1) = t.rect_to_physical_pixel_bounds(r);
        assert_eq!((x0, y0), (0, 0));
        assert_eq!((x1, y1), (15, 15));
    }

    #[test]
    fn clipped_readback_rect_uses_whole_physical_pixels() {
        let viewport = Viewport::new(12, 12, Fixed::from_f32(1.5));
        let logical = Rect::new(
            Fixed::from_f32(1.25),
            Fixed::ZERO,
            Fixed::from_f32(2.5),
            Fixed::ONE,
        );
        assert_eq!(
            viewport.physical_rect_in(logical, 12, 12),
            PhysicalRect::new(1, 0, 5, 2)
        );
        assert_eq!(
            viewport.physical_rect_in(Rect::new(-2, 6, 5, 4), 12, 8),
            None
        );
    }

    #[test]
    fn physical_rect_clips_negative_fractional_edges_once() {
        let viewport = Viewport::new(20, 12, Fixed::from_f32(1.5));
        assert_eq!(
            viewport.physical_rect(Rect::new(-1.25, 1.25, 4.0, 2.5)),
            PhysicalRect::new(0, 1, 5, 5)
        );
    }

    #[test]
    fn physical_rect_quantization_is_stable_across_scales() {
        let logical = Rect::new(
            Fixed::from_ratio(-1, 4),
            Fixed::from_ratio(5, 4),
            Fixed::from_ratio(7, 2),
            Fixed::from_ratio(9, 4),
        );
        let cases = [
            (
                Viewport::new(20, 20, Fixed::ONE),
                PhysicalRect::new(0, 1, 4, 3),
            ),
            (
                Viewport::new(30, 30, Fixed::from_ratio(3, 2)),
                PhysicalRect::new(0, 1, 5, 5),
            ),
            (
                Viewport::new(40, 40, Fixed::from_int(2)),
                PhysicalRect::new(0, 2, 7, 5),
            ),
        ];
        for (viewport, expected) in cases {
            assert_eq!(viewport.physical_rect(logical), expected);
        }
    }

    #[test]
    fn empty_logical_regions_never_expand_to_physical_pixels() {
        for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
            let viewport = Viewport::new(32, 32, scale);
            for (width, height) in [(0, 4), (4, 0), (-1, 4), (4, -1)] {
                let region = Rect::new(
                    Fixed::from_ratio(1, 4),
                    Fixed::from_ratio(1, 4),
                    width,
                    height,
                );
                assert_eq!(viewport.physical_rect(region), None);
            }
        }
    }
}
