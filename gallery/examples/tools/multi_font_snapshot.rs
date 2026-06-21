//! Renders the multi_font bundle demo off-screen and saves a PNG so we
//! can eyeball the pixel-vs-SDF size routing without going through SDL.

extern crate alloc;

use mirui::prelude::*;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::FlexDirection;
use mirui::ui::render_system;

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn main() {
    let out_path: PathBuf = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        let dir = env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".local/screenshots".into());
        std::fs::create_dir_all(&dir).ok();
        let mut p = PathBuf::from(dir);
        p.push("multi_font.png");
        p
    });

    let width: u16 = 480;
    let height: u16 = 320;

    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app: App<_, _> = App::new(backend);
    app.with_default_widgets();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            ..Default::default()
        })
        .id();

    mirui::gallery::demos::multi_font::register_font(&mut app.world);
    mirui::gallery::demos::multi_font::build_widgets(&mut app.world, root);
    app.set_root(root);

    let world = &mut app.world;
    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(world, root, &viewport);
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(world, root, &viewport, &mut renderer);
    }

    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    let stride = tex.stride;

    let ppm_path = out_path.with_extension("ppm");
    {
        let f = File::create(&ppm_path).expect("create ppm");
        let mut w = BufWriter::new(f);
        write!(w, "P6\n{width} {height}\n255\n").expect("ppm header");
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                w.write_all(&[pixels[i], pixels[i + 1], pixels[i + 2]])
                    .expect("ppm pixel");
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
