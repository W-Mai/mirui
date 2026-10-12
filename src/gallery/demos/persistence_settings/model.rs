#[crate::model(persistence(key = "reader_settings"))]
pub struct SettingsModel {
    #[persist]
    font_size: u16,
    #[persist]
    #[observe]
    pub(super) show_details: bool,
    #[observe]
    pub(super) preview_open: bool,
}

impl Default for SettingsModel {
    fn default() -> Self {
        Self {
            font_size: 18,
            show_details: true,
            preview_open: true,
        }
    }
}

#[crate::model]
impl SettingsModel {
    #[observe]
    pub fn preview_size(&self) -> u16 {
        self.font_size.clamp(12, 24)
    }

    pub fn set_font_size(&mut self, size: u16) {
        self.font_size = size.clamp(12, 24);
    }

    pub fn set_show_details(&mut self, show: bool) {
        self.show_details = show;
    }

    pub fn toggle_preview(&mut self) {
        self.preview_open = !self.preview_open;
    }
}
