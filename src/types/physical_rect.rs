/// A target-clipped rectangle in integer physical pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PhysicalRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl PhysicalRect {
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Option<Self> {
        if x.checked_add(width).is_none() || y.checked_add(height).is_none() {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub const fn from_size(width: u16, height: u16) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn right(self) -> u16 {
        self.x + self.width
    }

    pub const fn bottom(self) -> u16 {
        self.y + self.height
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left >= right || top >= bottom {
            return None;
        }
        Self::new(left, top, right - left, bottom - top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_is_half_open_and_bounded() {
        let target = PhysicalRect::from_size(8, 6);
        assert_eq!(
            target.intersection(PhysicalRect::new(5, 4, 10, 10).unwrap()),
            PhysicalRect::new(5, 4, 3, 2)
        );
        assert_eq!(
            target.intersection(PhysicalRect::new(8, 0, 2, 6).unwrap()),
            None
        );
        assert_eq!(target.intersection(PhysicalRect::EMPTY), None);
        let edge = PhysicalRect::new(u16::MAX - 1, 0, 1, 1).unwrap();
        assert_eq!(edge.intersection(edge), Some(edge));
    }

    #[test]
    fn rejects_edges_outside_the_u16_target_domain() {
        assert!(PhysicalRect::new(u16::MAX, 0, 1, 1).is_none());
        assert!(PhysicalRect::new(0, u16::MAX, 1, 1).is_none());
        assert_eq!(
            PhysicalRect::new(3, 5, 7, 11),
            Some(PhysicalRect {
                x: 3,
                y: 5,
                width: 7,
                height: 11,
            })
        );
    }
}
