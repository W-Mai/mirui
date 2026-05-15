//! Headless snapshot for TextInput. Runs three states (empty +
//! focused, mid-typing, full-buffer) and saves PNGs to
//! .local/screenshots/.

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::app::App;
use mirui::components::text_input::{Placeholder, TextInput};
use mirui::draw::texture::ColorFormat;
use mirui::layout::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn write_png(out_path: &std::path::Path, pixels: &[u8], width: u16, height: u16, stride: usize) {
    let ppm_path = out_path.with_extension("ppm");
    {
        let f = File::create(&ppm_path).expect("create ppm");
        let mut w = BufWriter::new(f);
        use std::io::Write;
        write!(w, "P6\n{width} {height}\n255\n").expect("ppm header");
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                w.write_all(&pixels[i..i + 3]).expect("ppm pixel");
            }
        }
    }
    let _ = std::process::Command::new("python3")
        .args([
            "-c",
            &format!(
                "from PIL import Image; Image.open(r'{}').save(r'{}')",
                ppm_path.display(),
                out_path.display()
            ),
        ])
        .status();
    let _ = std::fs::remove_file(&ppm_path);
}

fn main() {
    let scenario = env::args().nth(1).unwrap_or_else(|| "empty".into());
    let mut out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out_path.push(".local/screenshots");
    std::fs::create_dir_all(&out_path).ok();
    out_path.push(format!("textinput-{scenario}.png"));

    let width: u16 = 320;
    let height: u16 = 60;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::ARGB8888, |_, _| {});
    let mut app = App::new(backend);

    let mut ti = TextInput::new();
    match scenario.as_str() {
        "empty" => ti.focused = false,
        "focused-empty" => ti.focused = true,
        "typed" => {
            for ch in b"hello world".iter() {
                ti.insert(*ch);
            }
            ti.focused = true;
        }
        "cursor-mid" => {
            for ch in b"hello".iter() {
                ti.insert(*ch);
            }
            ti.move_left();
            ti.move_left();
            ti.focused = true;
        }
        _ => panic!("unknown scenario: {scenario}"),
    }

    let root = WidgetBuilder::new(&mut app.world)
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

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        input (
            bg_color: Color::rgb(40, 40, 56),
            border_color: Color::rgb(80, 80, 100),
            width: 288,
            height: 28
        ) [
            ti,
            Placeholder("type something..."),
        ] {}
    };

    // Deterministic cursor: pin the blink phase ON so the snapshot
    // doesn't flicker by wall-clock timing.
    app.world
        .insert_resource(mirui::event::widget_input::CursorBlinkPhase(true));

    app.set_root(root);

    use mirui::draw::sw::SwRenderer;
    use mirui::surface::FramebufferAccess;
    use mirui::types::Viewport;
    use mirui::widget::render_system;

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&app.world, root, &viewport, &mut renderer);
    }

    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;
    write_png(&out_path, pixels, width, height, stride);
    eprintln!("saved {}", out_path.display());
}
