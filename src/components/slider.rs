use crate::types::{Color, Fixed};

pub struct Slider {
    pub value: Fixed,
    pub min: Fixed,
    pub max: Fixed,
    pub track_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
}

impl Slider {
    pub fn new(min: Fixed, max: Fixed) -> Self {
        Self {
            value: min,
            min,
            max,
            track_color: Color::rgb(60, 60, 80),
            fill_color: Color::rgb(88, 166, 255),
            thumb_color: Color::rgb(255, 255, 255),
        }
    }

    pub fn with_colors(mut self, track: Color, fill: Color, thumb: Color) -> Self {
        self.track_color = track;
        self.fill_color = fill;
        self.thumb_color = thumb;
        self
    }

    pub fn ratio(&self) -> Fixed {
        let range = self.max - self.min;
        if range <= Fixed::ZERO {
            return Fixed::ZERO;
        }
        (self.value - self.min) / range
    }

    pub fn set_ratio(&mut self, ratio: Fixed) {
        let clamped = ratio.clamp(Fixed::ZERO, Fixed::ONE);
        self.value = self.min + clamped * (self.max - self.min);
    }
}
