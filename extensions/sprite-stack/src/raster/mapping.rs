use mirui::types::Fixed;

/// Side of the square sheet that contains the viewport at every roll angle.
pub fn sheet_side(width: usize, height: usize, pivot: Fixed) -> usize {
    let tall = pivot.max(Fixed::from_int(height as i32) - pivot);
    let half = Fixed::from_int(width as i32) / 2;
    ((half * half + tall * tall).sqrt().ceil().to_int() as usize + 2) * 2
}
/// Sampling transform between an output viewport and the clipped source sheet for its roll.
pub struct RollMapping {
    pub size: [usize; 2],
    pub bounds: [i32; 4],
    pub rolled: bool,
    half: Fixed,
    cx: Fixed,
    pivot: Fixed,
    sine: Fixed,
    cosine: Fixed,
}
impl RollMapping {
    pub fn new(
        [width, height]: [usize; 2],
        pivot: Fixed,
        rotation: Option<(Fixed, Fixed)>,
    ) -> Self {
        let rolled = rotation.is_some();
        let cx = Fixed::from_int(width as i32) / 2;
        let (sine, cosine) = rotation.unwrap_or((Fixed::ZERO, Fixed::ONE));
        let mut mapping = Self {
            size: [width, height],
            bounds: [0, 0, width as i32, height as i32],
            rolled,
            half: Fixed::ZERO,
            cx,
            pivot,
            sine,
            cosine,
        };
        if rolled {
            let side = sheet_side(width, height, pivot);
            let half = Fixed::from_int((side / 2) as i32);
            let tall = pivot.max(Fixed::from_int(height as i32) - pivot);
            let x = (cosine.abs() * Fixed::from_int(width as i32) / 2 + sine.abs() * tall).ceil()
                + Fixed::from_int(2);
            let (above, below) = if cosine < Fixed::ZERO {
                (Fixed::from_int(height as i32) - pivot, pivot)
            } else {
                (pivot, Fixed::from_int(height as i32) - pivot)
            };
            let top = (cosine.abs() * above + sine.abs() * Fixed::from_int(width as i32) / 2)
                .ceil()
                + Fixed::from_int(2);
            let bottom = (cosine.abs() * below + sine.abs() * Fixed::from_int(width as i32) / 2)
                .ceil()
                + Fixed::from_int(2);
            mapping.size = [side, side];
            mapping.half = half;
            mapping.bounds = [
                (half - x).max(Fixed::ZERO).to_int(),
                (half - top).max(Fixed::ZERO).to_int(),
                (half + x).min(Fixed::from_int(side as i32)).to_int(),
                (half + bottom).min(Fixed::from_int(side as i32)).to_int(),
            ];
        }
        mapping
    }
    pub fn projection_center(&self, center: [Fixed; 2]) -> [Fixed; 2] {
        if self.rolled {
            [
                self.half + (center[0] - self.cx),
                self.half + (center[1] - self.pivot),
            ]
        } else {
            center
        }
    }
    /// Converts an output point to the nearest source texel.
    pub fn source_at(&self, x: i32, y: i32) -> [i32; 2] {
        if !self.rolled {
            return [x, y];
        }
        let dx = Fixed::from_int(x) - self.cx;
        let dy = Fixed::from_int(y) - self.pivot;
        [
            (self.half + self.cosine * dx - self.sine * dy + Fixed::from_ratio(1, 2)).to_int(),
            (self.half + self.sine * dx + self.cosine * dy + Fixed::from_ratio(1, 2)).to_int(),
        ]
    }
    /// Converts an unrounded source point into output coordinates for effect compositing.
    pub fn unroll(&self, point: [Fixed; 2]) -> [Fixed; 2] {
        if !self.rolled {
            return point;
        }
        let dx = point[0] - self.half;
        let dy = point[1] - self.half;
        [
            self.cx + self.cosine * dx + self.sine * dy,
            self.pivot - self.sine * dx + self.cosine * dy,
        ]
    }
}
