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
}

#[cfg(test)]
mod tests {
    use super::*;

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
