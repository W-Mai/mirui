use crate::prelude::*;
use crate::ui::widgets::TextAlign;

fn landscape(width: Fixed, height: Fixed) -> bool {
    width > height + Fixed::from_int(16)
}

pub(super) fn shell_direction(width: Fixed, height: Fixed) -> FlexDirection {
    if landscape(width, height) {
        FlexDirection::Row
    } else {
        FlexDirection::Column
    }
}

pub(super) fn header_direction(width: Fixed, height: Fixed) -> FlexDirection {
    if landscape(width, height) {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    }
}

pub(super) fn padding(width: Fixed, height: Fixed, narrow: i32, regular: i32) -> Padding {
    Padding::all(if width.min(height) < Fixed::from_int(112) {
        narrow
    } else {
        regular
    })
}

pub(super) fn header_width(width: Fixed, height: Fixed, landscape_width: i32) -> Dimension {
    if landscape(width, height) {
        Dimension::px(landscape_width)
    } else {
        Dimension::percent(100)
    }
}

pub(super) fn header_height(width: Fixed, height: Fixed) -> Dimension {
    if landscape(width, height) {
        Dimension::percent(100)
    } else {
        Dimension::px(14)
    }
}

pub(super) fn content_width(width: Fixed, height: Fixed) -> Dimension {
    if landscape(width, height) {
        Dimension::Auto
    } else {
        Dimension::percent(100)
    }
}

pub(super) fn content_height(width: Fixed, height: Fixed) -> Dimension {
    if landscape(width, height) {
        Dimension::percent(100)
    } else {
        Dimension::Auto
    }
}

pub(super) fn marker_width(width: Fixed, height: Fixed) -> Dimension {
    Dimension::px(if landscape(width, height) { 10 } else { 4 })
}

pub(super) fn marker_height(width: Fixed, height: Fixed) -> Dimension {
    Dimension::px(if landscape(width, height) { 4 } else { 10 })
}

pub(super) fn title_width(width: Fixed, height: Fixed) -> Dimension {
    if landscape(width, height) {
        Dimension::percent(100)
    } else {
        Dimension::Auto
    }
}

pub(super) fn title_height(width: Fixed, height: Fixed) -> Dimension {
    if landscape(width, height) {
        Dimension::Auto
    } else {
        Dimension::px(14)
    }
}

pub(super) fn mode_width(width: Fixed, height: Fixed, portrait_width: i32) -> Dimension {
    if landscape(width, height) {
        Dimension::percent(100)
    } else {
        Dimension::px(portrait_width)
    }
}

pub(super) fn mode_height(width: Fixed, height: Fixed) -> Dimension {
    Dimension::px(if landscape(width, height) { 12 } else { 14 })
}

pub(super) fn text_align(width: Fixed, height: Fixed) -> TextAlign {
    if landscape(width, height) {
        TextAlign::Center
    } else {
        TextAlign::Start
    }
}

pub(super) fn mode_align(width: Fixed, height: Fixed) -> TextAlign {
    if landscape(width, height) {
        TextAlign::Center
    } else {
        TextAlign::End
    }
}
