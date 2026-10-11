//! Headless snapshot for TextInput. Runs three states (empty +
//! focused, mid-typing, full-buffer) and saves PNGs to
//! .local/screenshots/.

extern crate alloc;

use mirui::prelude::*;
use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui::widgets::{Placeholder, TextEditing, TextInput, TextSelection};

fn write_png(out_path: &std::path::Path, pixels: &[u8], width: u16, height: u16, stride: usize) {
    assert_eq!(stride, usize::from(width) * 4);
    let file = BufWriter::new(File::create(out_path).expect("create snapshot"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(pixels).expect("write PNG pixels");
}

#[compose]
fn input_panel(input: TextEditing) -> Entity {
    ui! {
        View (
            bg_color: Color::rgb(40, 40, 56),
            border_color: Color::rgb(80, 80, 100),
            width: 288,
            height: 28
        ) [
            TextInput::new(),
            input,
            Placeholder("type something..."),
        ]
    }
}

fn main() {
    let scenario = env::args().nth(1).unwrap_or_else(|| "empty".into());
    let dir = std::env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".".into());
    std::fs::create_dir_all(&dir).ok();
    let mut out_path = PathBuf::from(dir);
    out_path.push(format!("textinput-{scenario}.png"));

    let width: u16 = 320;
    let height: u16 = 60;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();

    let mut editing = TextEditing::default();
    match scenario.as_str() {
        "empty" | "focused-empty" => {}
        "typed" => editing = editing.with_text("hello world").unwrap(),
        "cursor-mid" => {
            editing = editing.with_text("hello").unwrap();
            editing
                .set_selection(TextSelection {
                    anchor: 3,
                    cursor: 3,
                })
                .unwrap();
        }
        _ => panic!("unknown scenario: {scenario}"),
    }

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            padding: Padding {
                top: Dimension::px(16),
                left: Dimension::px(16),
                right: Dimension::px(16),
                bottom: Dimension::px(16),
            },
            ..Default::default()
        })
        .id();

    let input = app.compose(root, input_panel(editing));
    if scenario != "empty" {
        app.add_resource(mirui::input::event::focus::FocusState {
            focused: Some(input),
        });
    }
    app.tick();

    // Deterministic cursor: pin the blink phase ON so the snapshot
    // doesn't flicker by wall-clock timing.
    app.add_resource(mirui::ui::widgets::text_editing::CursorBlinkPhase(true));

    use mirui::render::sw::SwRenderer;
    use mirui::surface::FramebufferAccess;
    use mirui::types::Viewport;
    use mirui::ui::render_system;

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    }

    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;
    write_png(&out_path, pixels, width, height, stride);
    eprintln!("saved {}", out_path.display());
}
