//! Headless snapshot for LazyList. Renders three scroll positions
//! (top, mid, bottom) to .local/screenshots/.

extern crate alloc;

use mirui::prelude::*;
use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::ecs::SystemScheduler;
use mirui::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::lazy_list::{CreateCell, LazyList, lazy_list_system};

const ROW_H: i32 = 32;
const ITEM_COUNT: u32 = 200;
const W: u16 = 320;
const H: u16 = 320;

#[event]
fn create_row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! {
        Text (
            bg_color: Color::rgb(40, 40, 56),
            text_color: Color::rgb(220, 220, 230),
            text: ${ alloc::format!("Row {}", item.index()) }
        )
    }
}

fn write_png(path: &std::path::Path, pixels: &[u8], stride: usize) {
    use std::io::Write;
    let output = BufWriter::new(File::create(path).expect("snapshot output"));
    let mut encoder = png::Encoder::new(output, u32::from(W), u32::from(H));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    let mut stream = writer.stream_writer().expect("PNG stream");
    for y in 0..usize::from(H) {
        let start = y * stride;
        stream
            .write_all(&pixels[start..start + usize::from(W) * 4])
            .expect("PNG row");
    }
    stream.finish().expect("PNG finish");
}

#[compose]
fn list_panel(scroll_y_int: i32) {
    ui! {
        View (
            bg_color: Color::rgb(28, 28, 40),
            width: W as i32,
            height: H as i32
        ) [
            LazyList::new(ITEM_COUNT, ROW_H),
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(scroll_y_int),
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                ..Default::default()
            },
        ] on LazyList::CreateCell(create_row::callback())
    };
}

fn main() {
    let scenario = env::args().nth(1).unwrap_or_else(|| "top".into());
    // scroll_y is positive: lazy_list_system computes
    // visible_start = max(scroll_y / item_extent, 0).
    let scroll_y_int: i32 = match scenario.as_str() {
        "top" => 0,
        "mid" => 1600,                                // row 50 visible
        "bottom" => ROW_H * (ITEM_COUNT as i32 - 10), // last 10 rows
        _ => panic!("unknown: {scenario}"),
    };

    let dir = std::env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".".into());
    std::fs::create_dir_all(&dir).ok();
    let mut out = PathBuf::from(dir);
    out.push(format!("lazylist-{scenario}.png"));

    let backend = FramebufSurface::with_format(W, H, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();

    app.compose(root, list_panel(scroll_y_int));

    use mirui::render::sw::SwRenderer;
    use mirui::surface::FramebufferAccess;
    use mirui::types::Viewport;
    use mirui::ui::render_system;

    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    scheduler.run_all(&mut app.world).unwrap();

    let viewport = Viewport::new(W, H, Fixed::ONE);
    render_system::try_update_layout(&mut app.world, root, &viewport)
        .expect("prepare snapshot layout");
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    }

    let tex = app.backend.framebuffer();
    write_png(&out, tex.buf.as_slice(), tex.stride);
    eprintln!("saved {}", out.display());
}
