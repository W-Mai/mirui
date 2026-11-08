use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use mirui::core::perf;
use mirui::input::event::input::{InputEvent, PointerKind};
use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
use mirui::prelude::*;

static TIME: AtomicU64 = AtomicU64::new(0);
const W: u16 = 760;
const H: u16 = 520;

fn clock() -> u64 {
    TIME.load(Ordering::Relaxed)
}

pub(super) fn run() {
    let backend = std::env::args().nth(1).unwrap();
    let frames = std::env::args()
        .nth(2)
        .map_or(120, |v| v.parse::<usize>().unwrap());
    assert!((1..=3600).contains(&frames));
    match backend.as_str() {
        "--glass-sw" => collect(App::headless(W, H), frames),
        "--glass-sdl" => collect(
            App::new(mirui::surface::sdl::SdlSurface::new_with_vsync(
                "Glass perf / SDL",
                W,
                H,
                false,
            )),
            frames,
        ),
        #[cfg(feature = "sdl-gpu")]
        "--glass-sdl-gpu" => {
            use mirui::surface::sdl_gpu::{SdlGpuConfig, SdlGpuFactory, SdlGpuSurface};
            let surface = SdlGpuSurface::with_config(
                "Glass perf / SDL GPU",
                W,
                H,
                SdlGpuConfig {
                    vsync: false,
                    ..Default::default()
                },
            )
            .unwrap();
            let factory = SdlGpuFactory::new().with_projective_fallback(
                mirui::render::ProjectiveFallback::new(vec![0; 2 * 1024 * 1024]),
            );
            collect(App::with_factory(surface, factory), frames);
        }
        _ => panic!("use --glass-sw, --glass-sdl or --glass-sdl-gpu [frames]"),
    }
}

fn collect<B: Surface, F: RendererFactory<B>>(mut app: App<B, F>, frames: usize) {
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().ignore_safe_area().id();
    mirui::gallery::demos::glass_light_toggle::setup_app(&mut app, root);
    app.add_resource(MonoClock::new(clock));
    app.add_system(sim_input_system::system());
    let mut commands = Vec::new();
    for step in (0..frames + 16).step_by(45) {
        let x = Fixed::from_int(380);
        let y = Fixed::from_int(250);
        commands.push(SimCommand::at(
            step as u32 * 16,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y,
            },
        ));
        commands.push(SimCommand::at(
            step as u32 * 16 + 1,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y,
            },
        ));
    }
    app.add_resource(SimulatedInput::new(commands));
    app.render().unwrap();
    println!(
        "logical={W}x{H} physical={:?} debug={} frames={frames} vsync=false wall_includes_present=true gpu_completion_forced=false",
        app.backend.physical_size(),
        cfg!(debug_assertions)
    );
    let mut capture = perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap();
    let mut spans: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    let mut samples = Vec::with_capacity(frames);
    for frame in 0..frames + 16 {
        TIME.store((frame as u64 + 1) * 16_000_000, Ordering::Relaxed);
        let start = Instant::now();
        app.tick();
        app.render_dirty().unwrap();
        if frame >= 16 {
            samples.push(start.elapsed().as_nanos());
        }
        capture.drain_events(|_, batch| {
            if frame >= 16 {
                for record in batch {
                    if let Some(event) = record.span() {
                        let entry = spans.entry(event.name).or_default();
                        entry.0 += 1;
                        entry.1 += event.end_ns.saturating_sub(event.start_ns);
                    }
                }
            }
        });
    }
    samples.sort_unstable();
    println!(
        "avg_us={} p50_us={} p95_us={} max_us={} trace={:?}",
        samples.iter().sum::<u128>() / frames as u128 / 1000,
        samples[frames / 2] / 1000,
        samples[(frames * 95 / 100).min(frames - 1)] / 1000,
        samples.last().unwrap() / 1000,
        capture.status()
    );
    println!("span totals are inclusive; nested totals overlap and must not be added");
    let mut rows: Vec<_> = spans.into_iter().collect();
    rows.sort_by_key(|(_, (_, time))| std::cmp::Reverse(*time));
    for (name, (calls, time)) in rows.into_iter().take(35) {
        println!(
            "{name}: calls={calls} total_us={} per_frame_us={}",
            time / 1000,
            time / 1000 / frames as u64
        );
    }
}
