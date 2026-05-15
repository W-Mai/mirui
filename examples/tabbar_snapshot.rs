//! Headless snapshot for the TabBar demo. Builds the same widget tree
//! tabbar_demo.rs builds, drives the system schedule for a few frames
//! (enough for the indicator tween to settle), then dumps the
//! framebuffer as a PNG into .local/screenshots/.
//!
//! Run with `cargo run --example tabbar_snapshot --features sdl` (sdl
//! pulls std and the layout types we need; the demo itself doesn't
//! open a window).

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::app::App;
use mirui::components::tabbar::TabBar;
use mirui::draw::texture::ColorFormat;
use mirui::layout::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

mirui_macros::animate!(AnimateTabIndicator, |world, entity, value| {
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.indicator_offset = value;
    }
    world.insert(entity, Dirty);
});

fn main() {
    let mut args = env::args().skip(1);
    let selected_arg: u8 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let out_path: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push(".local/screenshots");
        std::fs::create_dir_all(&p).ok();
        p.push(format!("tabbar-selected-{selected_arg}.png"));
        p
    });

    let width: u16 = 480;
    let height: u16 = 80;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);

    app.add_system(mirui::anim::sync_delta_time_ms);
    app.add_system(AnimateTabIndicator::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            ..Default::default()
        })
        .id();

    let tabs = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tabbar (
            bg_color: Color::rgb(40, 40, 56),
            width: 480,
            height: 40
        ) {
            tab0 (
                text: "Home",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab1 (
                text: "Search",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab2 (
                text: "Profile",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
        }
    };

    let tb = TabBar::new(3).with_indicator(Color::rgb(88, 166, 255), 3);
    app.world.insert(tabs, tb);

    // Set selected directly + jump indicator_offset to make this a
    // deterministic still-frame.
    if let Some(tb) = app.world.get_mut::<TabBar>(tabs) {
        tb.selected = selected_arg.min(2);
        tb.indicator_offset = Fixed::from_int(tb.selected as i32);
    }

    app.set_root(root);

    // Run one frame manually: layout + render straight against the
    // framebuffer. Avoids SDL.
    use mirui::draw::sw::SwRenderer;
    use mirui::types::Viewport;
    use mirui::widget::render_system;

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        use mirui::surface::FramebufferAccess;
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&app.world, root, &viewport, &mut renderer);
    }

    // Pull pixels and write a binary PPM. Convert to PNG separately
    // with PIL (Pillow) — bypasses the rabbit hole of writing a
    // hand-rolled BMP/PNG encoder.
    use mirui::surface::FramebufferAccess;
    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;

    let ppm_path = out_path.with_extension("ppm");
    {
        let f = File::create(&ppm_path).expect("create ppm");
        let mut w = BufWriter::new(f);
        use std::io::Write;
        write!(w, "P6\n{width} {height}\n255\n").expect("ppm header");
        // ColorFormat::RGBA8888 stores R G B A in byte order despite
        // the name (see src/draw/sw/rect_fill.rs `[color.r, color.g,
        // color.b, color.a]` write).
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                let r = pixels[i];
                let g = pixels[i + 1];
                let b = pixels[i + 2];
                w.write_all(&[r, g, b]).expect("ppm pixel");
            }
        }
    }

    let status = std::process::Command::new("python3")
        .args([
            "-c",
            &format!(
                "from PIL import Image; Image.open(r'{}').save(r'{}')",
                ppm_path.display(),
                out_path.display()
            ),
        ])
        .status();
    match status {
        Ok(s) if s.success() => {
            let _ = std::fs::remove_file(&ppm_path);
            eprintln!("saved {}", out_path.display());
        }
        _ => {
            eprintln!("PIL conversion failed; PPM left at {}", ppm_path.display());
        }
    }
}
