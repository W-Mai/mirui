//! Renders the theme_swap layout off-screen and saves a PNG so we can
//! eyeball the result without going through SDL.

extern crate alloc;

use mirui::ecs::World;
use mirui::input::event::GestureHandler;
use mirui::input::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::Theme;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::FlexDirection;
use mirui::ui::render_system;
use mirui::ui::theme;
use mirui::ui::widgets::{Button, Checkbox, ProgressBar, Slider, Switch, TabBar, Text, TextInput};

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

const ACCENT: ColorToken = ColorToken::Custom("accent");

fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

fn dummy_handler(_: &mut World, _: mirui::ecs::Entity, _: &GestureEvent) -> bool {
    false
}

fn main() {
    let out_path: PathBuf = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        let dir = env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".local/screenshots".into());
        std::fs::create_dir_all(&dir).ok();
        let mut p = PathBuf::from(dir);
        p.push("theme_swap.png");
        p
    });

    let width: u16 = 480;
    let height: u16 = 320;

    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app: App<_, _> = App::new(backend);
    app.with_theme(dark_with_accent()).with_default_widgets();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            padding: Padding::all(12),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Row (height: 44) {
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(40, 50, 70),
                pressed_color: Color::rgb(20, 25, 35)
            ) [
                GestureHandler::from_fn(dummy_handler),
            ] {
                Text ("Dark")
            }
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(0, 100, 200),
                pressed_color: Color::rgb(0, 70, 150)
            ) [
                GestureHandler::from_fn(dummy_handler),
            ] {
                Text ("Light")
            }
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(255, 105, 180),
                pressed_color: Color::rgb(200, 70, 140)
            ) [
                GestureHandler::from_fn(dummy_handler),
            ] {
                Text ("Custom")
            }
        }
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (grow: 1.0) {
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Slider")
                }
                Slider (
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    grow: 1.0,
                    height: 20
                )
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Switch")
                }
                Switch (width: 56, height: 28)
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Checkbox")
                }
                Checkbox (width: 24, height: 24, border_radius: 4)
            }
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Progress")
                }
                ProgressBar (grow: 1.0, height: 12, border_radius: 6)
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Input")
                }
                TextInput (grow: 1.0, height: 28)
            }
            Row (height: 24, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Tabs")
                }
                TabBar (count: 3, grow: 1.0, height: 24) {
                    View (grow: 1.0)
                    View (grow: 1.0)
                    View (grow: 1.0)
                }
            }
        }
    };

    let pbs: Vec<mirui::ecs::Entity> = app.world.query::<ProgressBar>().collect();
    for pb in pbs {
        if let Some(p) = app.world.get_mut::<ProgressBar>(pb) {
            p.value = 0.6;
        }
    }

    let _ = theme::Theme::dark();
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
