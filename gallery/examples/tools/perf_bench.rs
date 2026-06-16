//! Benchmarks the SDL CPU vs SDL GPU backend on a standard UI scene.
//!
//! Run twice:
//!   cargo run --release --features sdl      --example perf_bench  # CPU path
//!   cargo run --release --features sdl-gpu  --example perf_bench  # GPU path
//!
//! The scene has 30 solid-fill rectangles, 5 rounded rectangles with
//! thick borders, 10 text labels, and 2 image blits — roughly the shape
//! of a settings screen. Vsync is forced off so frame rate reflects the
//! real per-frame cost; runs for 10 seconds and prints avg FPS and per-
//! frame render time.

use mirui::components::Image;
use mirui::components::assets::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::widget::{Children, Parent};

const W: u16 = 800;
const H: u16 = 600;

fn main() {
    #[cfg(feature = "sdl-gpu")]
    let mut app = {
        use mirui::surface::sdl_gpu::{SdlGpuFactory, SdlGpuSurface};
        let backend = SdlGpuSurface::new_with_vsync("mirui perf_bench — SDL GPU", W, H, false);
        let mut app = App::with_factory(backend, SdlGpuFactory::new());
        app.with_default_widgets();
        app
    };

    #[cfg(all(feature = "sdl", not(feature = "sdl-gpu")))]
    let mut app = {
        use mirui::surface::sdl::SdlSurface;
        let backend = SdlSurface::new_with_vsync("mirui perf_bench — SDL CPU", W, H, false);
        let mut app = App::new(backend);
        app.with_default_widgets();
        app
    };

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(18, 18, 28, 255))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            ..Default::default()
        })
        .id();

    let mut children: alloc::vec::Vec<mirui::ecs::Entity> = alloc::vec::Vec::new();

    for i in 0..30 {
        let shade = 40 + (i * 5) as u8;
        let c = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgba(shade, 80, 200, 255))
            .layout(LayoutStyle {
                width: Dimension::px(60),
                height: Dimension::px(24),
                ..Default::default()
            })
            .id();
        children.push(c);
    }

    for i in 0..5 {
        let c = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgba(240, 120, 60, 200))
            .border(Color::rgba(255, 255, 255, 255), 3)
            .border_radius(14)
            .layout(LayoutStyle {
                width: Dimension::px(120),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();
        children.push(c);
        let _ = i;
    }

    let labels = [
        "WIDGET A",
        "WIDGET B",
        "SETTINGS",
        "HELLO WORLD",
        "MIRUI GPU",
        "PERFORMANCE",
        "BENCHMARK",
        "FPS TEST",
        "DEMO SCENE",
        "RUSTACEAN",
    ];
    for &s in &labels {
        let c = WidgetBuilder::new(&mut app.world)
            .text(s)
            .text_color(Color::rgba(230, 230, 255, 255))
            .layout(LayoutStyle {
                width: Dimension::px(120),
                height: Dimension::px(20),
                ..Default::default()
            })
            .id();
        children.push(c);
    }

    for _ in 0..2 {
        let c = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                width: Dimension::px(IMG_THUMBS_UP.width as i32),
                height: Dimension::px(IMG_THUMBS_UP.height as i32),
                ..Default::default()
            })
            .id();
        app.world.insert(c, Image::new("thumbs_up"));
        children.push(c);
    }

    for child in &children {
        app.world.insert(*child, Parent(root));
        if let Some(cs) = app.world.get_mut::<Children>(root) {
            cs.0.push(*child);
        }
    }

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default())
        .add_plugin(mirui::plugins::ImageResourcesPlugin::default());

    use mirui::surface::{InputEvent, Surface};
    let start = std::time::Instant::now();
    let duration = std::time::Duration::from_secs(10);
    let mut frames: u64 = 0;
    loop {
        if start.elapsed() > duration {
            break;
        }
        let mut quit = false;
        loop {
            match app.backend.poll_event() {
                Some(InputEvent::Quit) => {
                    quit = true;
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }
        if quit {
            break;
        }
        app.render();
        frames += 1;
    }
    let elapsed = start.elapsed();
    let fps = frames as f64 / elapsed.as_secs_f64();
    println!(
        "\n=== perf_bench results ===\nframes: {}\nelapsed: {:.2}s\nfps: {:.1}",
        frames,
        elapsed.as_secs_f64(),
        fps
    );
}

extern crate alloc;
