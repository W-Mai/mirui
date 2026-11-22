use core::num::NonZeroU16;
use mirui::types::{Fixed, Point, Rect};

/// Output pixel dimensions and their placement in a scene viewport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSize {
    pixels: [u16; 2],
    scale: Option<NonZeroU16>,
}

/// Selects the raster dimensions independently of layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Resolution {
    /// Render at one pixel per logical viewport unit.
    #[default]
    Native,
    /// Render at an integer scale within the maximum frame extent.
    Integer {
        limit: [NonZeroU16; 2],
        minimum: [u16; 2],
        scale: NonZeroU16,
    },
}

impl Resolution {
    /// Resolves the frame pixels and placement for this viewport.
    pub fn fit(self, viewport: [u16; 2]) -> FrameSize {
        let Self::Integer {
            limit,
            minimum,
            scale,
        } = self
        else {
            return FrameSize::stretch(viewport);
        };
        let [max_width, max_height] = limit.map(NonZeroU16::get);
        let [width, height] = viewport;
        let scale = scale
            .get()
            .max(width.div_ceil(max_width))
            .max(height.div_ceil(max_height));
        let pixels = [
            width.div_ceil(scale).max(minimum[0].min(max_width)),
            height.div_ceil(scale).max(minimum[1].min(max_height)),
        ];
        FrameSize::integer(
            pixels,
            NonZeroU16::new(scale).expect("positive placement scale"),
        )
    }
}

impl FrameSize {
    pub const fn stretch(pixels: [u16; 2]) -> Self {
        Self {
            pixels,
            scale: None,
        }
    }

    pub const fn integer(pixels: [u16; 2], scale: NonZeroU16) -> Self {
        Self {
            pixels,
            scale: Some(scale),
        }
    }

    pub const fn pixels(self) -> [u16; 2] {
        self.pixels
    }

    pub const fn integer_scale(self) -> Option<NonZeroU16> {
        self.scale
    }

    pub(super) fn placed(self, viewport: Rect) -> Rect {
        let Some(scale) = self.scale else {
            return viewport;
        };
        let [w, h] = self
            .pixels
            .map(|side| Fixed::from(side) * Fixed::from(scale.get()));
        Rect::new(
            viewport.x + ((viewport.w - w) / 2).round(),
            viewport.y + ((viewport.h - h) / 2).round(),
            w,
            h,
        )
    }

    /// Maps a point in the viewport's coordinate space into the frame.
    pub fn frame_point(self, viewport: Rect, point: Point) -> Option<Point> {
        if !viewport.w.is_positive() || !viewport.h.is_positive() || self.pixels.contains(&0) {
            return None;
        }
        let rect = self.placed(viewport);
        let delta = Point::new(point.x - rect.x, point.y - rect.y);
        let [sx, sy] = if let Some(scale) = self.scale {
            [Fixed::from(scale.get()); 2]
        } else {
            [
                rect.w / Fixed::from(self.pixels[0]),
                rect.h / Fixed::from(self.pixels[1]),
            ]
        };
        if !sx.is_positive() || !sy.is_positive() {
            return None;
        }
        Some(Point::new(delta.x / sx, delta.y / sy))
    }
}
