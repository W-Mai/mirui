//! Render a string with the prebuilt MiSans SDF atlas to PNG.
//!
//! Usage:
//!   cargo run --release -p gallery --example sdf_label_snapshot -- \
//!     [text] [out.png]
//!
//! Defaults to "Hello mirui!" → `.local/screenshots/sdf-hello.png`.
//! The atlas comes from `tests/fixtures/misans_regular_ascii_32_4bit.mirx`,
//! which the gen-mirx font subcommand bakes from MiSans-Regular.ttf.

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::font::sdf::SdfFontProvider;
use mirui::render::font::{Font, FontBackend, FontManager, FontToken};
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui::widgets::Text;
use mirx::{chunk_type, parse_chunk};

const ATLAS_BYTES: &[u8] =
    include_bytes!("../../../tests/fixtures/misans_regular_ascii_32_4bit.mirx");

fn load_misans_font() -> Font {
    let parsed = parse_chunk(ATLAS_BYTES).expect("parse mirx");
    let payload = parsed
        .chunk_payload(ATLAS_BYTES, chunk_type::FONT)
        .expect("FONT chunk");
    let provider = SdfFontProvider::from_mirx_chunk(payload).expect("parse atlas");
    let size = provider.header().source_size;
    Font {
        family: "MiSans-Regular",
        size,
        backend: FontBackend::Custom(alloc::rc::Rc::new(provider)),
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let text: String = args.next().unwrap_or_else(|| "Hello mirui!".into());
    let out_path: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let dir = std::env::var("MIRUI_SNAPSHOT_DIR")
            .unwrap_or_else(|_| format!("{}/../.local/screenshots", env!("CARGO_MANIFEST_DIR")));
        std::fs::create_dir_all(&dir).ok();
        let mut p = PathBuf::from(dir);
        p.push("sdf-hello.png");
        p
    });

    let width: u16 = 480;
    let height: u16 = 80;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    // Register the SDF MiSans atlas under FontToken::Heading so the
    // Text widget asks for it explicitly. FontToken::Default keeps
    // the bundled bitmap path so untagged labels still render.
    {
        let mgr = app.world.resource::<FontManager>().expect("FontManager");
        mgr.add_static(FontToken::Heading.cache_key(), load_misans_font());
    }

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            padding: Padding::all(Dimension::px(16)),
            justify: JustifyContent::Center,
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Text (
            text.as_str(),
            font: FontToken::Heading,
            text_color: Color::rgb(255, 220, 140)
        )
    };

    app.set_root(root);

    use mirui::render::sw::SwRenderer;
    use mirui::types::Viewport;
    use mirui::ui::render_system;

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        use mirui::surface::FramebufferAccess;
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&mut app.world, root, &viewport, &mut renderer);
    }

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
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                w.write_all(&pixels[i..i + 3]).expect("ppm pixel");
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
            eprintln!("PIL not available; PPM left at {}", ppm_path.display());
        }
    }
}
