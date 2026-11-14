//! Plays Gallery demos with seeded random taps and drags on a simulated
//! 16 ms clock, times every frame, and every 40 frames compares the
//! incrementally painted framebuffer with a full repaint of the same state.
//! A difference means a region changed without being marked dirty.
//!
//! ```text
//! cargo run --release -p gallery --example game_audit --no-default-features \
//!     --features snapshot -- OUT_DIR marble_play lumen_lab ...
//! ```
//!
//! Environment: `FRAMES` (default 400), `SEED`, `SNAP_EVERY` (frames between
//! saved PNGs), `SLOW_US` (save and report frames slower than this),
//! `DARK_ONLY`. Build with `--features snapshot,perf` for a per-span breakdown.

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use mirui::input::event::input::{InputEvent, PointerKind};
use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{DisplayInfo, FramebufferAccess, RedrawStrategy, Surface};

static NOW_NS: AtomicU64 = AtomicU64::new(0);

fn clock() -> u64 {
    NOW_NS.load(Ordering::Relaxed)
}

type Inner = FramebufSurface<fn(&[u8], PhysicalRect)>;

struct AuditSurface {
    inner: Inner,
    events: VecDeque<InputEvent>,
}

impl mirui::core::cache::InspectCaches for AuditSurface {}

impl Surface for AuditSurface {
    fn display_info(&self) -> DisplayInfo {
        self.inner.display_info()
    }
    fn physical_size(&self) -> (u32, u32) {
        self.inner.physical_size()
    }
    fn flush(&mut self, area: PhysicalRect) -> Result<(), mirui::render::RenderError> {
        self.inner.flush(area)
    }
    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }
    fn redraw_strategy(&self) -> RedrawStrategy {
        self.inner.redraw_strategy()
    }
}

impl FramebufferAccess for AuditSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.inner.framebuffer()
    }
}

type AuditApp = App<AuditSurface>;

fn pixels(app: &mut AuditApp) -> Vec<u8> {
    app.backend
        .framebuffer()
        .read_pixels()
        .expect("read pixels")
        .bytes()
        .to_vec()
}

fn write_png(path: &Path, (width, height): (u16, u16), rgba: &[u8]) {
    let file = std::io::BufWriter::new(fs::File::create(path).expect("create png"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

fn pointer(kind: u8, x: i32, y: i32) -> InputEvent {
    let (x, y) = (Fixed::from_int(x), Fixed::from_int(y));
    let k = PointerKind::Touch;
    match kind {
        0 => InputEvent::PointerDown {
            kind: k,
            id: 0,
            x,
            y,
        },
        1 => InputEvent::PointerMove {
            kind: k,
            id: 0,
            x,
            y,
        },
        _ => InputEvent::PointerUp {
            kind: k,
            id: 0,
            x,
            y,
        },
    }
}

/// Differing pixels and their bounding box.
fn diff(a: &[u8], b: &[u8], width: usize) -> (usize, (usize, usize, usize, usize)) {
    let mut count = 0;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        let d = pa
            .iter()
            .zip(pb)
            .take(3)
            .map(|(p, q)| p.abs_diff(*q))
            .max()
            .unwrap_or(0);
        if d > 8 {
            count += 1;
            let (x, y) = (i % width, i / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (count, (x0, y0, x1, y1))
}

fn run(slug: &str, dark: bool, frames: u32, out: &Path, seed: u32) -> String {
    let demo = gallery::lookup_demo(slug).expect("registered demo");
    let size = demo.size.fixed_size().unwrap_or((
        demo.size.max_width.unwrap_or(480).min(480),
        demo.size.max_height.unwrap_or(320).min(320),
    ));
    let (width, height) = size;
    let flush: fn(&[u8], PhysicalRect) = |_, _| {};
    let surface = AuditSurface {
        inner: FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush),
        events: VecDeque::new(),
    };
    let mut app: AuditApp = App::new(surface);
    app.with_default_widgets().with_default_systems();
    let root = gallery::setup_demo(slug, &mut app).expect("setup");
    if let Some(id) = gallery::appearance_theme_id(app.theme().id().as_str(), dark) {
        app.set_theme(id).expect("theme");
    }
    app.set_root(root);
    NOW_NS.store(0, Ordering::Relaxed);
    app.add_resource(MonoClock::new(clock));
    app.prepare_text_layout().ok();
    app.render().ok();

    #[cfg(feature = "perf")]
    let mut capture =
        mirui::core::perf::Capture::new(mirui::core::perf::DEFAULT_TRACE_BUDGET).unwrap();
    #[cfg(feature = "perf")]
    let mut spans: std::collections::BTreeMap<&'static str, (u64, u64)> = Default::default();
    let mut rng = Rng(seed);
    let mut times = Vec::with_capacity(frames as usize);
    let mut mismatches = Vec::new();
    let mut gesture: Option<(i32, i32, i32, i32, u32)> = None;
    let theme = if dark { "dark" } else { "light" };
    let every: u32 = env::var("SNAP_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(frames / 4);
    let mut slowest = (0u64, 0u32);
    for frame in 0..frames {
        NOW_NS.store(u64::from(frame + 1) * 16_000_000, Ordering::Relaxed);
        // Input: a tap, a drag over a few frames, or nothing.
        match gesture.take() {
            Some((x, y, tx, ty, left)) if left > 0 => {
                let step_x = x + (tx - x) / left as i32;
                let step_y = y + (ty - y) / left as i32;
                app.backend.events.push_back(pointer(1, step_x, step_y));
                gesture = Some((step_x, step_y, tx, ty, left - 1));
            }
            Some((x, y, ..)) => app.backend.events.push_back(pointer(2, x, y)),
            None if frame % 9 == 4 => {
                let x = rng.below(u32::from(width)) as i32;
                let y = rng.below(u32::from(height)) as i32;
                app.backend.events.push_back(pointer(0, x, y));
                let drag = rng.below(4) == 0;
                let (tx, ty) = if drag {
                    (
                        rng.below(u32::from(width)) as i32,
                        rng.below(u32::from(height)) as i32,
                    )
                } else {
                    (x, y)
                };
                gesture = Some((x, y, tx, ty, if drag { 6 } else { 1 }));
            }
            None => {}
        }
        let start = Instant::now();
        if app.tick() {
            return format!("{slug} {theme}: quit at frame {frame}");
        }
        let micros = start.elapsed().as_micros() as u64;
        if let Err(error) = app.render_dirty() {
            eprintln!("  render error at frame {frame}: {error:?}");
        }
        if let Some(limit) = env::var("SLOW_US").ok().and_then(|v| v.parse::<u64>().ok())
            && micros > limit
        {
            eprintln!("  slow frame {frame}: {micros}us gesture {gesture:?}");
            let rgba = pixels(&mut app);
            write_png(
                &out.join(format!("{slug}_{theme}_slow_{frame:04}.png")),
                size,
                &rgba,
            );
        }
        if micros > slowest.0 {
            slowest = (micros, frame);
        }
        times.push(micros);
        #[cfg(feature = "perf")]
        capture.drain_events(|_, batch| {
            for record in batch {
                if let Some(span) = record.span() {
                    let entry = spans.entry(span.name).or_default();
                    entry.0 += 1;
                    entry.1 += span.end_ns.saturating_sub(span.start_ns);
                }
            }
        });
        if frame == 0 || frame % every == every - 1 {
            let rgba = pixels(&mut app);
            write_png(
                &out.join(format!("{slug}_{theme}_{frame:04}.png")),
                size,
                &rgba,
            );
        }
        if frame % 40 == 39 {
            let incremental = pixels(&mut app);
            app.render().ok();
            let full = pixels(&mut app);
            let (count, bbox) = diff(&incremental, &full, usize::from(width));
            if count > 0 {
                if mismatches.len() < 3 {
                    let base = out.join(format!("{slug}_{theme}_stale_{frame:04}"));
                    write_png(&base.with_extension("inc.png"), size, &incremental);
                    write_png(&base.with_extension("full.png"), size, &full);
                }
                mismatches.push(format!("f{frame}:{count}px@{bbox:?}"));
            }
        }
    }
    #[cfg(feature = "perf")]
    {
        capture.stop();
        let mut rows: Vec<_> = spans.into_iter().collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1.1));
        for (name, (count, total)) in rows.into_iter().take(18) {
            eprintln!(
                "    {name:<36} {count:>7} {:>9}us/frame",
                total / 1000 / u64::from(frames)
            );
        }
    }
    times.sort_unstable();
    let pct = |p: f64| times[((times.len() - 1) as f64 * p) as usize];
    let avg = times.iter().sum::<u64>() / times.len() as u64;
    format!(
        "{slug:<18} {theme:<5} {width}x{height}  avg {avg:>6}us  p50 {:>6}  p95 {:>6}  max {:>7} @f{}  stale {}",
        pct(0.5),
        pct(0.95),
        pct(1.0),
        slowest.1,
        if mismatches.is_empty() {
            "none".to_string()
        } else {
            format!("{} {}", mismatches.len(), mismatches.join(" "))
        }
    )
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let out = PathBuf::from(args.remove(0));
    fs::create_dir_all(&out).expect("output dir");
    let frames: u32 = env::var("FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400);
    for slug in &args {
        let themes: &[bool] = if env::var_os("DARK_ONLY").is_some() {
            &[true]
        } else {
            &[true, false]
        };
        for &dark in themes {
            let seed = env::var("SEED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0x1234_5678);
            let line = catch_unwind(AssertUnwindSafe(|| run(slug, dark, frames, &out, seed)))
                .unwrap_or_else(|_| {
                    format!("{slug} {}: PANIC", if dark { "dark" } else { "light" })
                });
            println!("{line}");
        }
    }
}
