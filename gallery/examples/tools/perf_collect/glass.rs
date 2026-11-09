use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use mirui::core::perf;
use mirui::input::event::input::{InputEvent, PointerKind};
use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
use mirui::prelude::*;
#[path = "../backend_snapshot_support.rs"]
mod backend_snapshot_support;

static TIME: AtomicU64 = AtomicU64::new(0);
const W: u16 = 760;
const H: u16 = 520;
const WARMUP_FRAMES: usize = 16;

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
    for step in (0..frames + WARMUP_FRAMES).step_by(45) {
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
    let initial_start = Instant::now();
    app.render().unwrap();
    let initial_elapsed = initial_start.elapsed().as_nanos();
    let snapshots = std::env::var_os("GLASS_SNAPSHOTS").map(std::path::PathBuf::from);
    if let Some(folder) = &snapshots {
        let frame = app.snapshot().unwrap().unwrap();
        backend_snapshot_support::write_png(&folder.join("initial.png"), &frame);
    }
    println!(
        "logical={W}x{H} physical={:?} debug={} frames={frames} vsync=false wall_includes_present=true gpu_completion_forced=false",
        app.backend.physical_size(),
        cfg!(debug_assertions)
    );
    println!("initial_render_us={}", initial_elapsed / 1000);
    let mut capture = perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap();
    let mut spans: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    let mut samples = Vec::with_capacity(frames);
    let mut warmup = [0_u128; WARMUP_FRAMES];
    let mut warmup_slots = warmup.iter_mut();
    for frame in 0..frames + WARMUP_FRAMES {
        TIME.store((frame as u64 + 1) * 16_000_000, Ordering::Relaxed);
        let start = Instant::now();
        app.tick();
        app.render_dirty().unwrap();
        let elapsed = start.elapsed().as_nanos();
        if let Some(folder) = &snapshots
            && [20, 40, 70].contains(&frame)
        {
            let image = app.snapshot().unwrap().unwrap();
            backend_snapshot_support::write_png(&folder.join(format!("frame-{frame}.png")), &image);
        }
        if let Some(slot) = warmup_slots.next() {
            *slot = elapsed;
        } else {
            samples.push(elapsed);
        }
        capture.drain_events(|_, batch| {
            if frame >= WARMUP_FRAMES {
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
    println!(
        "warmup_frames={WARMUP_FRAMES} warmup_mean_us={} warmup_max_us={}",
        warmup.iter().sum::<u128>() / WARMUP_FRAMES as u128 / 1000,
        warmup.iter().max().unwrap() / 1000
    );
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
