use mirui::prelude::*;
use mirui::render::font::{Font, FontManager};
use mirui::surface::FramebufferAccess;
use mirui::text::{
    FontAccessError, FontId, FontMetrics, ShapeError, ShapeRequest, ShapedGlyph, Typeface,
};
use mirui::ui::widgets::{ParagraphStyle, Text, TextWrap};
use textflow::layout::{LayoutScratch, WrapMode};

const MENU: &[u8] = include_bytes!("../src/gallery/demos/assets/nearby-menu.mirx");
const TITLE: &[u8] = include_bytes!("../src/gallery/demos/assets/nearby-title.mirx");

struct Face<'a>(&'a Font, u16);
impl Typeface for Face<'_> {
    fn id(&self) -> FontId {
        self.0.face_id()
    }
    fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
        Ok(FontMetrics::default())
    }
    fn covers(&self, text: &str) -> Result<bool, FontAccessError> {
        Ok(self.0.covers(text))
    }
    fn shape_into(
        &self,
        request: &ShapeRequest<'_>,
        output: &mut [ShapedGlyph],
    ) -> Result<usize, ShapeError> {
        self.0.shape_into(self.1, request, output)
    }
}

#[test]
fn original_fonts_produce_only_visible_glyphs_and_original_caret_offsets() {
    for (size, bytes) in [(12, MENU), (24, TITLE)] {
        let font = Font::from_mirx("Nearby", size, bytes, &Default::default()).unwrap();
        let face = Face(&font, size);
        let mut scratch = LayoutScratch::<16, 16, 8, 32>::new();
        let text = "A\r\n\nB\n";
        let layout = textflow::TextFlow::new(text, 100_000)
            .with_wrap(WrapMode::NoWrap)
            .with_line_height(usize::from(size) * 256)
            .layout_with_scratch(&[&face], &mut scratch)
            .unwrap();
        assert_eq!(layout.lines().len(), 4);
        assert_eq!(layout.glyphs().len(), 2);
        assert!(
            layout
                .glyphs()
                .iter()
                .all(|glyph| glyph.glyph_id() != font.provider.notdef_glyph().unwrap())
        );
        assert_eq!(layout.lines()[0].advance(), i32::from(size) * 128);
        assert_eq!(layout.lines()[1].advance(), 0);
        assert!(!layout.carets().iter().any(|caret| caret.text_offset == 2));
        assert_eq!(
            layout.carets().last().unwrap().text_offset as usize,
            text.len()
        );
    }
}

#[model]
struct Document {
    #[observe]
    text: &'static str,
}

extern crate alloc;

#[model]
impl Document {
    fn replace(&mut self, text: &'static str) {
        self.text = text;
    }
}

#[compose]
fn label(document: model!(Document), size: u16) -> Entity {
    ui! {
        Text (
            text: ${ document.text() },
            width: 100,
            font_size: size,
            text_color: Color::rgb(255, 255, 255),
            paragraph: ParagraphStyle {
                wrap: TextWrap::NoWrap,
                max_lines: None,
                ..Default::default()
            }
        )
    }
}

#[test]
fn cached_reactive_paragraphs_repaint_without_geometry_subscribers() {
    for (size, bytes) in [(12, MENU), (24, TITLE)] {
        let font = Font::from_mirx("Nearby", size, bytes, &Default::default()).unwrap();
        let mut app = App::headless(128, 160);
        app.with_default_widgets().with_default_systems();
        app.with_fonts(FontManager::new(mirui::core::cache::MaxSize::Unbound, font));
        let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
        let document = app.add_model(Document { text: "A\nB" }).finish();
        app.compose(root, label(document.clone(), size));
        app.render().unwrap();
        let initial = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        let ink = initial
            .chunks_exact(4)
            .filter(|pixel| pixel[0] != 0)
            .count();
        assert!(ink > 0, "initial text must paint visible pixels");
        document.replace("A\r\nB");
        app.tick();
        app.render_dirty().unwrap();
        assert!(
            app.backend.framebuffer().read_pixels().unwrap().bytes() == initial,
            "CRLF must match LF pixels"
        );
        document.replace("A\r\n\r\nB\r\n");
        app.tick();
        app.render_dirty().unwrap();
        let changed = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        assert!(
            changed != initial,
            "inserting a blank line must move visible text"
        );
        app.render().unwrap();
        assert!(
            app.backend.framebuffer().read_pixels().unwrap().bytes() == changed,
            "partial rendering must match full"
        );
        document.replace("\r\n\n");
        app.tick();
        app.render_dirty().unwrap();
        let blank = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        document.replace("");
        app.tick();
        app.render_dirty().unwrap();
        assert!(
            app.backend.framebuffer().read_pixels().unwrap().bytes() == blank,
            "blank lines must not paint missing glyphs"
        );
    }
}
