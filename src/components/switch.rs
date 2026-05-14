use crate::types::Color;

pub struct Switch {
    pub on: bool,
    pub on_color: Color,
    pub off_color: Color,
    pub thumb_color: Color,
}

impl Switch {
    pub fn new() -> Self {
        Self {
            on: false,
            on_color: Color::rgb(63, 185, 80),
            off_color: Color::rgb(80, 80, 100),
            thumb_color: Color::rgb(255, 255, 255),
        }
    }

    pub fn with_colors(mut self, on: Color, off: Color, thumb: Color) -> Self {
        self.on_color = on;
        self.off_color = off;
        self.thumb_color = thumb;
        self
    }

    pub fn toggle(&mut self) {
        self.on = !self.on;
    }

    pub fn track_color(&self) -> Color {
        if self.on {
            self.on_color
        } else {
            self.off_color
        }
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::new()
    }
}
