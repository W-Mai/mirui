use crate::types::Color;

pub struct Button {
    pub pressed: bool,
    pub normal_color: Color,
    pub pressed_color: Color,
}

impl Button {
    pub fn new(normal: Color, pressed: Color) -> Self {
        Self {
            pressed: false,
            normal_color: normal,
            pressed_color: pressed,
        }
    }

    pub fn current_color(&self) -> Color {
        if self.pressed {
            self.pressed_color
        } else {
            self.normal_color
        }
    }
}
