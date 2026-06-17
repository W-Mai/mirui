use super::SdlGpuRenderer;
use crate::render::font::Font;
use crate::types::{Color, Point, Rect};

impl SdlGpuRenderer<'_> {
    pub(super) fn draw_label_inner(
        &mut self,
        pos: &Point,
        text: &[u8],
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        let phys_pos = self.viewport.point_to_physical(*pos);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.label_cache.draw(
            self.canvas,
            &phys_pos,
            text,
            font,
            &phys_clip,
            color,
            opa,
            self.viewport.scale(),
        );
    }
}
