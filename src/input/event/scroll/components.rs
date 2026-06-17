use crate::types::Fixed;

/// Scroll direction
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

/// Scroll offset component — any widget with this becomes scrollable
pub struct ScrollOffset {
    pub x: Fixed,
    pub y: Fixed,
}

/// Per-frame `ScrollOffset` increment. Input / inertia systems write
/// it; the dirty walker reads it to plan a framebuffer self-blit and
/// subtracts the integer pixels it consumed, leaving any sub-pixel
/// residue for the next frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollDelta {
    pub dx: Fixed,
    pub dy: Fixed,
}

/// Scroll configuration
pub struct ScrollConfig {
    pub direction: ScrollAxis,
    pub elastic: bool,
    pub content_height: Fixed,
    pub content_width: Fixed,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            direction: ScrollAxis::Vertical,
            elastic: true,
            content_height: Fixed::ZERO,
            content_width: Fixed::ZERO,
        }
    }
}
