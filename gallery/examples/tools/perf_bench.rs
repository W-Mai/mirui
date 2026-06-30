//! Multi-scene image hot-path bench. Uses mirui's own `trace_span!`
//! recorder so the breakdown matches what ESP perf-fps reports
//! (`sw.fill_aligned`, `sw.blit`, `draw.entity`, `draw.view_dispatch`,
//! `render.draw_tree`, etc). Also reports image-path-only micros
//! (ResourceManager::resolve, World::resource lookup) that ESP can't
//! easily isolate.
//!
//! Each scene runs FRAMES times after a WARMUP pass that primes
//! caches and (importantly) lets `trace_span!` ENABLED stay off so
//! warmup events don't pollute the histogram.
//!
//! Compile with `--features perf` so trace_span emits events;
//! otherwise it's a no-op and the breakdown stays empty.

use mirui::app::plugins::StdInstantClockPlugin;
use mirui::core::perf::{self, PerfEvent};
use mirui::core::resource::ResourceManager;
use mirui::ecs::Entity;
use mirui::prelude::*;
use mirui::render::command::{CompositeMode, DrawCommand};
use mirui::render::renderer::Renderer;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{FramebufferAccess, Surface};
use mirui::types::{Point, Rect, Transform, Viewport};
use mirui::ui::render_system;
use mirui::ui::widgets::Image;
use mirui::ui::widgets::assets::*;
use mirui::ui::{Children, Parent};

use std::collections::BTreeMap;
use std::time::Instant;

const W: u16 = 800;
const H: u16 = 600;
const FRAMES: u32 = 500;
const WARMUP: u32 = 30;

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_aggregate(label: &str, events: &[PerfEvent]) {
    let mut buckets: BTreeMap<&'static str, Vec<u64>> = BTreeMap::new();
    for ev in events {
        let dur = ev.end_ns.saturating_sub(ev.start_ns);
        buckets.entry(ev.name).or_default().push(dur);
    }
    println!("\n[scene: {label}]  ({} unique spans)", buckets.len());
    println!(
        "  {:<32} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "span", "count", "avg_ns", "p50_ns", "p99_ns", "max_ns", "total_us"
    );
    let mut rows: Vec<_> = buckets.into_iter().collect();
    rows.sort_by(|a, b| {
        let a_total: u64 = a.1.iter().sum();
        let b_total: u64 = b.1.iter().sum();
        b_total.cmp(&a_total)
    });
    for (name, mut samples) in rows {
        samples.sort();
        let count = samples.len() as u64;
        let total: u64 = samples.iter().sum();
        let avg = total / count.max(1);
        let p50 = percentile(&samples, 0.50);
        let p99 = percentile(&samples, 0.99);
        let max = *samples.last().unwrap_or(&0);
        println!(
            "  {name:<32} {count:>6} {avg:>10} {p50:>10} {p99:>10} {max:>10} {:>10}",
            total / 1000,
        );
    }
}

fn record_scene<F>(label: &str, mut frame_fn: F)
where
    F: FnMut(),
{
    perf::set_enabled(false);
    for _ in 0..WARMUP {
        frame_fn();
    }
    let _ = perf::drain_events();

    perf::set_enabled(true);
    let wall_start = Instant::now();
    for _ in 0..FRAMES {
        frame_fn();
    }
    let wall_ns = wall_start.elapsed().as_nanos() as u64;
    perf::set_enabled(false);
    let events = perf::drain_events();
    println!(
        "  wall {:>7}us / {} frames = {}us/frame",
        wall_ns / 1000,
        FRAMES,
        (wall_ns / FRAMES as u64) / 1000,
    );
    print_aggregate(label, &events);
}

fn build_image_heavy<B: Surface + FramebufferAccess>(app: &mut App<B>, count: u32) -> Entity {
    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(18, 18, 28, 255))
        .layout(LayoutStyle {
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            ..Default::default()
        })
        .id();
    let iw = IMG_THUMBS_UP.width as i32;
    let ih = IMG_THUMBS_UP.height as i32;
    let cols = (W as i32) / (iw + 2);
    let mut children = Vec::new();
    for n in 0..(count as i32) {
        let col = n % cols;
        let row = n / cols;
        let c = WidgetBuilder::new(&mut app.world)
            .layout(LayoutStyle {
                position: mirui::ui::layout::Position::Absolute,
                left: Dimension::px(col * (iw + 2) + 2),
                top: Dimension::px(row * (ih + 2) + 2),
                width: Dimension::px(iw),
                height: Dimension::px(ih),
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
    root
}

fn build_no_image<B: Surface + FramebufferAccess>(app: &mut App<B>, count: u32) -> Entity {
    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(18, 18, 28, 255))
        .layout(LayoutStyle {
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            ..Default::default()
        })
        .id();
    let iw = IMG_THUMBS_UP.width as i32;
    let ih = IMG_THUMBS_UP.height as i32;
    let cols = (W as i32) / (iw + 2);
    let mut children = Vec::new();
    for n in 0..(count as i32) {
        let col = n % cols;
        let row = n / cols;
        let c = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgba(80, 120, 200, 255))
            .layout(LayoutStyle {
                position: mirui::ui::layout::Position::Absolute,
                left: Dimension::px(col * (iw + 2) + 2),
                top: Dimension::px(row * (ih + 2) + 2),
                width: Dimension::px(iw),
                height: Dimension::px(ih),
                ..Default::default()
            })
            .id();
        children.push(c);
    }
    for child in &children {
        app.world.insert(*child, Parent(root));
        if let Some(cs) = app.world.get_mut::<Children>(root) {
            cs.0.push(*child);
        }
    }
    app.set_root(root);
    root
}

fn run_scene_image_heavy(count: u32, label: &str) {
    let backend = FramebufSurface::new(W, H, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());

    let root = build_image_heavy(&mut app, count);
    let info = app.backend.display_info();
    let viewport = Viewport::new(info.width, info.height, Fixed::ONE);

    record_scene(label, || {
        mirui::trace_span!("frame.layout", {
            render_system::update_layout(&mut app.world, root, &viewport);
        });
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        mirui::trace_span!("frame.render", {
            render_system::render(&app.world, root, &viewport, &mut renderer);
        });
    });
}

fn run_scene_no_image(count: u32, label: &str) {
    let backend = FramebufSurface::new(W, H, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.add_plugin(StdInstantClockPlugin);
    let root = build_no_image(&mut app, count);
    let info = app.backend.display_info();
    let viewport = Viewport::new(info.width, info.height, Fixed::ONE);

    record_scene(label, || {
        mirui::trace_span!("frame.layout", {
            render_system::update_layout(&mut app.world, root, &viewport);
        });
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        mirui::trace_span!("frame.render", {
            render_system::render(&app.world, root, &viewport, &mut renderer);
        });
    });
}

fn run_micro_resolve() {
    let backend = FramebufSurface::new(W, H, |_, _| {});
    let mut app = App::new(backend);
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());
    let mgr = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();

    let n_per_frame = 1000u32;
    record_scene("micro_resolve_x1000", || {
        mirui::trace_span!("frame.resolve_loop", {
            for _ in 0..n_per_frame {
                mirui::trace_span!("resolve_call", {
                    std::hint::black_box(mgr.resolve("thumbs_up"));
                });
            }
        });
    });
}

fn run_micro_world_resource() {
    let backend = FramebufSurface::new(W, H, |_, _| {});
    let mut app = App::new(backend);
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());

    let n_per_frame = 1000u32;
    record_scene("micro_world_resource_x1000", || {
        mirui::trace_span!("frame.world_resource_loop", {
            for _ in 0..n_per_frame {
                mirui::trace_span!("world_resource_call", {
                    std::hint::black_box(app.world.resource::<ResourceManager<Texture<'static>>>());
                });
            }
        });
    });
}

fn run_micro_blit() {
    let mut backend = FramebufSurface::new(W, H, |_, _| {});
    let info = backend.display_info();
    let viewport = Viewport::new(info.width, info.height, Fixed::ONE);
    let n_blits = 1000u32;
    let iw = IMG_THUMBS_UP.width as i32;
    let ih = IMG_THUMBS_UP.height as i32;
    let cols = (W as i32) / (iw + 2);
    let clip = Rect::new(0, 0, W, H);

    record_scene("micro_sw_blit_x1000", || {
        mirui::trace_span!("frame.blit_loop", {
            let tex = backend.framebuffer();
            let mut renderer = SwRenderer::new(tex);
            renderer.viewport = viewport;
            for n in 0..(n_blits as i32) {
                let col = n % cols;
                let row = n / cols;
                let cmd = DrawCommand::Blit {
                    pos: Point {
                        x: Fixed::from_int(col * (iw + 2) + 2),
                        y: Fixed::from_int(row * (ih + 2) + 2),
                    },
                    size: Point {
                        x: Fixed::from_int(iw),
                        y: Fixed::from_int(ih),
                    },
                    transform: Transform::default(),
                    quad: None,
                    texture: &IMG_THUMBS_UP,
                    opa: 255,
                    radius: Fixed::ZERO,
                    composite: CompositeMode::SourceOver,
                };
                renderer.draw(&cmd, &clip);
            }
        });
    });
}

fn main() {
    println!("=== perf_bench (frames={FRAMES} warmup={WARMUP}, viewport {W}x{H}) ===");
    run_scene_image_heavy(1000, "image_heavy x1000");
    run_scene_image_heavy(100, "image_heavy x100");
    run_scene_no_image(1000, "no_image x1000");
    run_micro_resolve();
    run_micro_world_resource();
    run_micro_blit();
}
