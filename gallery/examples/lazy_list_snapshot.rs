//! Headless snapshot for LazyList. Renders three scroll positions
//! (top, mid, bottom) to .local/screenshots/.

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::app::App;
use mirui::components::lazy_list::{LazyList, LazyListBinder, LazyListPool, lazy_list_system};
use mirui::draw::texture::ColorFormat;
use mirui::ecs::{Entity, World};
use mirui::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::layout::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::Text;
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

const ROW_H: i32 = 32;
const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 200;
const W: u16 = 320;
const H: u16 = 320;

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        t.0 = label.into_bytes();
    } else {
        world.insert(entity, Text(label.into_bytes()));
    }
}

fn write_png(path: &std::path::Path, pixels: &[u8], stride: usize) {
    let ppm = path.with_extension("ppm");
    {
        let f = File::create(&ppm).expect("ppm");
        let mut w = BufWriter::new(f);
        use std::io::Write;
        write!(w, "P6\n{W} {H}\n255\n").unwrap();
        for y in 0..(H as usize) {
            for x in 0..(W as usize) {
                let i = y * stride + x * 4;
                w.write_all(&pixels[i..i + 3]).unwrap();
            }
        }
    }
    let _ = std::process::Command::new("python3")
        .args([
            "-c",
            &format!(
                "from PIL import Image; Image.open(r'{}').save(r'{}')",
                ppm.display(),
                path.display()
            ),
        ])
        .status();
    let _ = std::fs::remove_file(&ppm);
}

fn main() {
    let scenario = env::args().nth(1).unwrap_or_else(|| "top".into());
    // scroll_y is positive: lazy_list_system computes
    // visible_start = max(scroll_y / item_height, 0).
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
    app.add_system(lazy_list_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();

    let list = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        list (
            bg_color: Color::rgb(28, 28, 40),
            width: W as i32,
            height: H as i32
        ) [
            LazyList::new(ITEM_COUNT, ROW_H, POOL_SIZE as u8),
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(scroll_y_int),
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                row (
                    bg_color: Color::rgb(40, 40, 56),
                    text_color: Color::rgb(220, 220, 230),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: W as i32,
                    height: ROW_H
                ) {}
            }
        }
    };

    let pool: alloc::vec::Vec<Entity> = app
        .world
        .get::<mirui::widget::Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    app.world.insert(list, LazyListPool::new(pool));
    app.world.insert(
        list,
        ScrollConfig {
            direction: ScrollAxis::Vertical,
            elastic: false,
            content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
            content_width: Fixed::ZERO,
        },
    );

    app.set_root(root);

    use mirui::draw::sw::SwRenderer;
    use mirui::surface::FramebufferAccess;
    use mirui::types::Viewport;
    use mirui::widget::render_system;

    lazy_list_system(&mut app.world);

    let viewport = Viewport::new(W, H, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&app.world, root, &viewport, &mut renderer);
    }

    let tex = app.backend.framebuffer();
    write_png(&out, tex.buf.as_slice(), tex.stride);
    eprintln!("saved {}", out.display());
}
