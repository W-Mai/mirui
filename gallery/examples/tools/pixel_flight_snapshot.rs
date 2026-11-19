mod backend_snapshot_support;

use mirui::{prelude::*, render::RenderError};
use mirui_pixel_flight::{self as pixel_flight, FlightModelHandle, flight::View};
use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn save<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, path: &Path) {
    for _ in 0..32 {
        while app.backend.poll_event().is_some() {}
        match app.snapshot() {
            Ok(Some(texture)) => {
                backend_snapshot_support::write_png(path, &texture);
                return;
            }
            Err(RenderError::SurfaceUnavailable) => {
                app.backend.frame_end().expect("maintain capture surface");
                std::thread::sleep(Duration::from_millis(16));
            }
            Ok(None) => panic!("flight snapshot is unavailable"),
            Err(error) => panic!("flight snapshot failed: {error:?}"),
        }
    }
    panic!("flight surface did not become available");
}
fn advance<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    model: &FlightModelHandle,
    frames: usize,
    size: (u16, u16),
    times: &mut Vec<f64>,
) {
    let first = times.len();
    for _ in 0..frames {
        let start = Instant::now();
        model.advance(16, size.0, size.1, None);

        // Drive the actual backend every frame, not only the final readback.
        match app.render_dirty() {
            Ok(_) => {}
            Err(RenderError::SurfaceUnavailable) => {}
            Err(e) => panic!("flight replay: {e:?}"),
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mean = times[first..].iter().sum::<f64>() / frames as f64;
    println!(
        "target {:?}, {frames} frames: simulation and render_dirty wall mean {mean:.3} ms",
        model.view()
    );
}
fn capture<B: Surface, F: RendererFactory<B>>(
    mut app: App<B, F>,
    output: &Path,
    size: (u16, u16),
    compact: bool,
) {
    use mirui::input::event::input::PhysicalKey;
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let model = if compact {
        pixel_flight::setup_compact_app(&mut app, root)
    } else {
        pixel_flight::setup_app(&mut app, root)
    };
    app.prepare_text_layout()
        .expect("prepare flight hint layout");
    save(&mut app, &output.join("initial.png"));
    if env::var_os("MIRUI_FLIGHT_GEOMETRY").is_some() {
        for (entity, _) in app.world.query::<mirui::ui::widgets::Text>().iter() {
            eprintln!(
                "hint geometry: {:?}",
                app.world
                    .get::<mirui::ui::ComputedRect>(entity)
                    .map(|rect| rect.0)
            );
        }
    }
    let mut times = Vec::with_capacity(2000);
    if env::args().any(|arg| arg == "--auto") {
        model.set_autoplay(true);
        for (name, frames, view) in [
            ("auto-overhead", 500, View::Overhead),
            ("auto-chase", 750, View::Chase),
            ("auto-cockpit", 750, View::Cockpit),
            ("auto-loop", 750, View::Overhead),
        ] {
            advance(&mut app, &model, frames, size, &mut times);
            assert_eq!(model.view(), view, "automatic camera schedule");
            save(&mut app, &output.join(format!("{name}.png")));
        }
        println!("automatic flight completed all views and returned to overhead");
        return;
    }
    for (name, view) in [
        ("overhead", View::Overhead),
        ("chase", View::Chase),
        ("cockpit", View::Cockpit),
    ] {
        model.select_view(view);
        advance(&mut app, &model, 240, size, &mut times);
        save(&mut app, &output.join(format!("{name}.png")));
    }
    model.select_view(View::Overhead);
    advance(&mut app, &model, 20, size, &mut times);
    save(&mut app, &output.join("transition.png"));
    model.select_view(View::Chase);
    advance(&mut app, &model, 8, size, &mut times);
    model.select_view(View::Cockpit);
    advance(&mut app, &model, 8, size, &mut times);
    save(&mut app, &output.join("retarget.png"));
    model.select_view(View::Overhead);
    advance(&mut app, &model, 240, size, &mut times);
    model.press();
    model.release(false);
    advance(&mut app, &model, 24, size, &mut times);
    save(&mut app, &output.join("roll.png"));
    model.key(PhysicalKey::ShiftLeft, true, false);
    advance(&mut app, &model, 90, size, &mut times);
    save(&mut app, &output.join("boost.png"));
    model.key(PhysicalKey::ShiftLeft, false, false);
    model.key(PhysicalKey::KeyF, true, false);
    model.key(PhysicalKey::KeyF, true, false);
    advance(&mut app, &model, 120, size, &mut times);
    save(&mut app, &output.join("wingmen.png"));
    model.key(PhysicalKey::KeyE, true, false);
    advance(&mut app, &model, 60, size, &mut times);
    save(&mut app, &output.join("expanded.png"));
    model.key(PhysicalKey::KeyE, true, false);
    model.key(PhysicalKey::KeyT, true, false);
    model.select_view(View::Chase);
    advance(&mut app, &model, 240, size, &mut times);
    save(&mut app, &output.join("sunset.png"));
    model.key(PhysicalKey::KeyT, true, false);
    advance(&mut app, &model, 60, size, &mut times);
    save(&mut app, &output.join("night.png"));
    times.sort_by(f64::total_cmp);
    let sum: f64 = times.iter().sum();
    println!(
        "{} simulation and render_dirty wall samples: mean {:.3} ms, p95 {:.3} ms, p99 {:.3} ms; CPU resident {} bytes",
        times.len(),
        sum / times.len() as f64,
        times[times.len() * 95 / 100],
        times[times.len() * 99 / 100],
        model.resident_bytes()
    );
    println!("render_dirty includes backend submission and any synchronous display waits");
    if let Some(control) = app.backend.size_control() {
        for target in [(128, 128), (320, 240), (256, 128), (128, 256), size] {
            control
                .request_size(target.0, target.1)
                .expect("request replay window size");
            let deadline = Instant::now() + Duration::from_secs(5);
            while control.state_untracked().requested.is_some() {
                while app.backend.poll_event().is_some() {}
                app.backend.frame_end().unwrap();
                assert!(Instant::now() < deadline, "flight resize did not complete");
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(control.state_untracked().actual, target);
            advance(&mut app, &model, 24, target, &mut times);
            save(
                &mut app,
                &output.join(format!("resize-{}x{}.png", target.0, target.1)),
            );
        }
    }
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let compact = args.iter().any(|s| s == "--compact");
    let output = args
        .iter()
        .find(|s| !s.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/screenshots/pixel-flight")
        });
    let size = args
        .iter()
        .find_map(|s| s.strip_prefix("--size="))
        .map(|value| {
            let (w, h) = value.split_once('x').expect("--size=WIDTHxHEIGHT");
            (
                w.parse::<u16>().expect("width"),
                h.parse::<u16>().expect("height"),
            )
        })
        .unwrap_or(if compact { (128, 128) } else { (1280, 720) });
    #[cfg(feature = "wgpu")]
    {
        let backend =
            mirui::surface::wgpu_surface::WgpuSurface::new("Pixel Flight capture", size.0, size.1)
                .expect("WGPU surface");
        capture(
            App::with_factory(backend, mirui::render::wgpu::WgpuRendererFactory::new()),
            &output,
            size,
            compact,
        );
    }
    #[cfg(all(not(feature = "wgpu"), feature = "sdl-gpu"))]
    {
        let backend =
            mirui::surface::sdl_gpu::SdlGpuSurface::new("Pixel Flight capture", size.0, size.1)
                .expect("SDL GPU surface");
        capture(
            App::with_factory(backend, mirui::render::sdl_gpu::SdlGpuFactory::new()),
            &output,
            size,
            compact,
        );
    }
    #[cfg(all(not(feature = "wgpu"), not(feature = "sdl-gpu"), feature = "sdl"))]
    {
        let backend = mirui::surface::sdl::SdlSurface::new("Pixel Flight capture", size.0, size.1);
        capture(App::new(backend), &output, size, compact);
    }
    #[cfg(not(any(feature = "wgpu", feature = "sdl-gpu", feature = "sdl")))]
    {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        let backend = mirui::surface::framebuf::FramebufSurface::with_format(
            size.0,
            size.1,
            mirui::render::texture::ColorFormat::RGBA8888,
            flush,
        );
        capture(App::new(backend), &output, size, compact);
    }
}
