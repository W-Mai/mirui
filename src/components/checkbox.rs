use crate::types::Color;

/// Checkbox / toggle component
pub struct Checkbox {
    pub checked: bool,
    pub checked_color: Color,
    pub unchecked_color: Color,
}

impl Checkbox {
    pub fn new(checked_color: Color, unchecked_color: Color) -> Self {
        Self {
            checked: false,
            checked_color,
            unchecked_color,
        }
    }

    pub fn toggle(&mut self) {
        self.checked = !self.checked;
    }

    pub fn current_color(&self) -> Color {
        if self.checked {
            self.checked_color
        } else {
            self.unchecked_color
        }
    }
}
