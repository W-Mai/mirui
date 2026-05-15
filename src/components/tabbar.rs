use crate::types::{Color, Fixed};

/// Horizontal tab bar with N children laid out flex-row.
///
/// `selected` is the discrete tab index; `indicator_offset` is the
/// continuously-animated position (0.0 .. count) used by the renderer
/// to draw the indicator bar. A Tween writes `indicator_offset` from
/// the previous selected value to the new one when the user taps.
pub struct TabBar {
    pub selected: u8,
    pub count: u8,
    pub indicator_offset: Fixed,
    pub indicator_color: Color,
    pub indicator_height: Fixed,
}

impl TabBar {
    pub fn new(count: u8) -> Self {
        Self {
            selected: 0,
            count,
            indicator_offset: Fixed::ZERO,
            indicator_color: Color::rgb(88, 166, 255),
            indicator_height: Fixed::from_int(2),
        }
    }

    pub fn with_indicator(mut self, color: Color, height: impl Into<Fixed>) -> Self {
        self.indicator_color = color;
        self.indicator_height = height.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let tb = TabBar::new(4);
        assert_eq!(tb.count, 4);
        assert_eq!(tb.selected, 0);
        assert_eq!(tb.indicator_offset, Fixed::ZERO);
    }

    #[test]
    fn with_indicator_overrides() {
        let tb = TabBar::new(3).with_indicator(Color::rgb(255, 0, 0), 5);
        assert_eq!(tb.indicator_color.r, 255);
        assert_eq!(tb.indicator_height, Fixed::from_int(5));
    }
}
