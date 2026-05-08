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
    pub x: i32,
    pub y: i32,
}

/// Scroll configuration
pub struct ScrollConfig {
    pub direction: ScrollAxis,
    pub elastic: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            direction: ScrollAxis::Vertical,
            elastic: true,
        }
    }
}
