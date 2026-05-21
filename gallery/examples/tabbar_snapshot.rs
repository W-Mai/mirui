extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::app::App;
use mirui::components::tab_pages::TabContent;
use mirui::components::tabbar::TabBar;
use mirui::draw::texture::ColorFormat;
use mirui::layout::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let mut args = env::args().skip(1);
    let selected_arg: u8 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let out_path: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let dir = std::env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".".into());
        std::fs::create_dir_all(&dir).ok();
        let mut p = PathBuf::from(dir);
        p.push(format!("tabbar-selected-{selected_arg}.png"));
        p
    });

    let width: u16 = 480;
    let height: u16 = 200;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

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
        ) [
            TabBar::new(3).with_indicator_height(3),
        ] {
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

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content_root (width: 480, height: 160) {
            home_page (
                bg_color: Color::rgb(63, 185, 80),
                text: "Home page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 160,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 0,
                },
            ] {}
            search_page (
                bg_color: Color::rgb(255, 165, 80),
                text: "Search page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 160,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 1,
                },
            ] {}
            profile_page (
                bg_color: Color::rgb(210, 168, 255),
                text: "Profile page",
                text_color: Color::rgb(40, 40, 56),
                width: 480,
                height: 160,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 2,
                },
            ] {}
        }
    };

    if let Some(tb) = app.world.get_mut::<TabBar>(tabs) {
        tb.selected = selected_arg.min(2);
    }

    app.set_root(root);

    use mirui::draw::sw::SwRenderer;
    use mirui::types::Viewport;
    use mirui::widget::render_system;

    // Tick once so tab_pages_system applies the initial Hidden flags +
    // seeds indicator_offset for the chosen selected tab.
    let world = &mut app.world;
    mirui::components::tab_pages::tab_pages_system(world);

    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(world, root, &viewport);
    {
        use mirui::surface::FramebufferAccess;
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(world, root, &viewport, &mut renderer);
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
