use std::time::Instant;

use mirui::core::cache::InspectCaches;
use mirui::gallery::demos::{nearby, page_studio, shapes};
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager};
use mirui::surface::sdl_gpu::{SdlGpuConfig, SdlGpuDriver, SdlGpuFactory, SdlGpuSurface};
use mirui::ui::widgets::Text;

const WARMUP: usize = 30;
const USAGE: &str = "usage: perf_collect --sdl-gpu <nearby|page_studio|shapes|text|digits> [frames=300] [auto|gl|gles2] [samples=0]";

#[model]
#[derive(Default)]
struct Counter {
    value: u32,
}

#[model]
impl Counter {
    #[observe]
    fn value(&self) -> u32 {
        self.value
    }

    fn advance(&mut self) {
        self.value = (self.value + 1) % 10_000;
    }
}

#[compose]
fn text_scene(counter: model!(Counter), dynamic: bool) -> Entity {
    ui! {
        Column (grow: 1.0, padding: Padding::all(20), row_gap: 12) {
            Text ("Coverage text / 0123456789", font_size: 24, height: 36)
            Text (
                text: ${ if dynamic { format!("{:04}", counter.value()) } else { String::from("0123") } },
                font_size: 24,
                height: 36
            )
            walk 0..12 with _row {
                Text (
                    "The quick brown fox jumps over the lazy dog. 0123456789",
                    font_size: 14,
                    height: 20
                )
            }
        }
    }
}

struct Options {
    scene: String,
    frames: usize,
    driver: SdlGpuDriver,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        if args.next().as_deref() != Some("--sdl-gpu") {
            return Err(USAGE.into());
        }
        let scene = args.next().ok_or(USAGE)?;
        if !matches!(
            scene.as_str(),
            "nearby" | "page_studio" | "shapes" | "text" | "digits"
        ) {
            return Err(USAGE.into());
        }
        let frames = args
            .next()
            .unwrap_or_else(|| "300".into())
            .parse::<usize>()
            .map_err(|_| String::from("frames must be an integer in 1..=100000"))?;
        if !(1..=100_000).contains(&frames) {
            return Err("frames must be in 1..=100000".into());
        }
        let driver = args.next().unwrap_or_else(|| "gl".into());
        let samples = args
            .next()
            .unwrap_or_else(|| "0".into())
            .parse::<u8>()
            .map_err(|_| String::from("samples must be 0 or 2..=255"))?;
        if samples == 1 || args.next().is_some() {
            return Err(USAGE.into());
        }
        let driver = match driver.as_str() {
            "auto" if samples == 0 => SdlGpuDriver::Auto,
            "gl" => SdlGpuDriver::OpenGl { samples },
            "gles2" => SdlGpuDriver::OpenGles2 { samples },
            _ => return Err(USAGE.into()),
        };
        Ok(Self {
            scene,
            frames,
            driver,
        })
    }
}

fn report_caches(label: &str, owner: &impl InspectCaches) {
    for (name, cache) in owner.inspect_caches() {
        println!(
            "cache phase={label} name={name} len={} limit={:?} stats={:?}",
            cache.cache_len(),
            cache.cache_max_size(),
            cache.cache_stats()
        );
    }
}

pub(super) fn run() -> Result<(), String> {
    let options = Options::parse(std::env::args().skip(1))?;
    let (width, height) = if options.scene == "page_studio" {
        (960, 680)
    } else {
        (640, 480)
    };
    let config = SdlGpuConfig {
        driver: options.driver,
        vsync: false,
        ..Default::default()
    };
    let backend = SdlGpuSurface::with_config("Mirui GPU perf", width, height, config)
        .map_err(|error| error.to_string())?;
    let mut app = App::with_factory(backend, SdlGpuFactory::new());
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().ignore_safe_area().id();
    let mut counter = None;
    match options.scene.as_str() {
        "nearby" => {
            nearby::setup_app(&mut app, root);
        }
        "page_studio" => page_studio::setup_app(&mut app, root),
        "shapes" => {
            app.with_widget(shapes::shapes_view());
            app.compose(root, shapes::build_widgets());
        }
        _ => {
            let font = Font::from_mirx(
                "MIRUI Play",
                12,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../src/gallery/demos/assets/play_ui.mirx"
                )),
                &mirx::reader::PayloadLimits::HOST,
            )
            .map_err(|error| format!("font: {error:?}"))?;
            app.with_fonts(FontManager::new(mirui::core::cache::MaxSize::Unbound, font));
            let model = app.add_model(Counter::default()).finish();
            app.compose(root, text_scene(model.clone(), options.scene == "digits"));
            counter = Some(model);
        }
    }
    app.add_resource(MonoClock::new(|| 0));
    let display = app.backend.display_info();
    println!(
        "scene={} renderer={} logical={}x{} physical={:?} scale={:?} config={config:?} debug_assertions={} warmup={WARMUP} frames={} batching_hint={:?}",
        options.scene,
        app.backend.renderer_name(),
        display.width,
        display.height,
        app.backend.physical_size(),
        display.scale,
        cfg!(debug_assertions),
        options.frames,
        sdl2::hint::get("SDL_RENDER_BATCHING")
    );
    println!(
        "metric=full_render_and_present_wall_ns systems=not_advanced readback=none gpu_completion=not_forced"
    );
    let start = Instant::now();
    app.render()
        .map_err(|error| format!("cold render: {error:?}"))?;
    println!("cold_ns={}", start.elapsed().as_nanos());
    let mut samples = Vec::with_capacity(options.frames);
    for frame in 0..WARMUP + options.frames {
        while let Some(event) = app.poll_event() {
            if matches!(event, mirui::surface::InputEvent::Quit) {
                return Err("collection cancelled; incomplete samples discarded".into());
            }
        }
        let start = Instant::now();
        if options.scene == "digits" {
            counter.as_ref().expect("digits model").advance();
        }
        app.render()
            .map_err(|error| format!("frame {frame}: {error:?}"))?;
        let elapsed = start.elapsed().as_nanos();
        if frame >= WARMUP {
            samples.push(elapsed);
        }
        if frame + 1 == WARMUP {
            report_caches("warm", &app.backend);
            report_caches("warm", &app.factory);
        }
    }
    samples.sort_unstable();
    let percentile = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
    println!(
        "avg_ns={} p50_ns={} p95_ns={} p99_ns={} max_ns={}",
        samples.iter().sum::<u128>() / samples.len() as u128,
        percentile(50),
        percentile(95),
        percentile(99),
        samples.last().unwrap()
    );
    report_caches("measured", &app.backend);
    report_caches("measured", &app.factory);
    Ok(())
}
