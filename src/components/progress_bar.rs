pub struct ProgressBar {
    pub value: f32, // 0.0 ~ 1.0
    pub track_color: crate::types::Color,
    pub fill_color: crate::types::Color,
}

impl ProgressBar {
    pub fn new(fill: crate::types::Color, track: crate::types::Color) -> Self {
        Self {
            value: 0.0,
            track_color: track,
            fill_color: fill,
        }
    }
}
