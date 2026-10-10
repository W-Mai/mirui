use crate::types::Fixed;

/// Scroll direction
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

/// Controls whether a pointer drag may be claimed by an ancestor scroller.
///
/// The nearest non-`Auto` value between the hit widget and the scroller wins.
/// Wheel and rotary input are unaffected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TouchAction {
    /// Defer to the nearest ancestor with an explicit policy.
    #[default]
    Auto,
    /// Keep both drag axes available to the widget.
    None,
    /// Allow horizontal scrolling while keeping vertical drags for the widget.
    PanX,
    /// Allow vertical scrolling while keeping horizontal drags for the widget.
    PanY,
}
impl crate::ecs::Component for TouchAction {}

impl TouchAction {
    pub(crate) const fn allows_scroll(self, axis: ScrollAxis) -> bool {
        matches!(
            (self, axis),
            (Self::Auto, _)
                | (Self::PanX, ScrollAxis::Horizontal)
                | (Self::PanY, ScrollAxis::Vertical)
        )
    }
}

/// Scroll offset component — any widget with this becomes scrollable
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollOffset {
    pub x: Fixed,
    pub y: Fixed,
}

impl crate::ecs::Component for ScrollOffset {}

/// Per-frame `ScrollOffset` increment, consumed by the dirty walker. Whole physical-pixel movements can reuse framebuffer pixels; fractional movements repaint without deferring input to another frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollDelta {
    pub dx: Fixed,
    pub dy: Fixed,
}

impl crate::ecs::Component for ScrollDelta {}

/// Scroll configuration.
///
/// A zero content dimension derives that extent from the widget's retained
/// descendants. Virtualized content can provide an explicit dimension.
pub struct ScrollConfig {
    pub direction: ScrollAxis,
    pub elastic: bool,
    pub content_height: Fixed,
    pub content_width: Fixed,
}

impl crate::ecs::Component for ScrollConfig {}

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
