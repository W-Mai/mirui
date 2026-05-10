use super::{Fixed, Point, Rect};

/// Maps between logical pixels (what `LayoutStyle` is written in) and
/// physical pixels (what the framebuffer holds).
///
/// All `screen_w / screen_h / scale` triples in the render pipeline collapse
/// into a single value of this type. `scale = 1` means 1 logical pixel ==
/// 1 physical pixel; `scale = 2` is a typical HiDPI desktop ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoordTransform {
    physical_w: u16,
    physical_h: u16,
    scale: Fixed,
}

impl CoordTransform {
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
            scale,
        }
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
        let w = (Fixed::from(self.physical_w) / self.scale).to_int() as u16;
        let h = (Fixed::from(self.physical_h) / self.scale).to_int() as u16;
        (w, h)
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

    #[inline]
    pub fn point_to_logical(&self, p: Point) -> Point {
        Point {
            x: p.x / self.scale,
            y: p.y / self.scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_scale_is_normalized_to_one() {
        let t = CoordTransform::new(100, 50, Fixed::ZERO);
        assert_eq!(t.scale(), Fixed::ONE);
        assert_eq!(t.logical_size(), (100, 50));
    }

    #[test]
    fn logical_size_divides_physical() {
        let t = CoordTransform::new(200, 100, Fixed::from_int(2));
        assert_eq!(t.logical_size(), (100, 50));
    }

    #[test]
    fn point_roundtrip_within_fixed_precision() {
        let t = CoordTransform::new(200, 100, Fixed::from_int(2));
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
        let t = CoordTransform::new(200, 100, Fixed::from_f32(1.5));
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
}
