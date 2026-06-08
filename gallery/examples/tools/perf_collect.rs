//! Deterministic perf-collection harness.
//!
//! Wraps the SDL surface with `SlowSurface` so flush time mimics
//! a 80 MHz SPI display, drives the UI through `SimTimeline`, and
//! prints a per-window report via `PerfReportPlugin`. The point is
//! reproducible per-stage / per-system numbers across commits — see
//! `.local/specs/perf-harness/IMPLEMENTATION.md` for context.
//!
//! Run:
//!   cargo run --release -p gallery --example perf_collect

use mirui::anim::ease;
use mirui::components::Text;
use mirui::components::{LazyList, LazyListBinder, LazyListPool};
use mirui::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::plugins::{PerfReportPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::surface::slow::SlowSurface;
use mirui::types::{Color, DimPoint, Dimension, Fixed};
use mirui::widget::Children;

const ROW_H: i32 = 24;
const POOL_SIZE: usize = 16; // 320 / 24 = 13.3 visible, +3 buffer
const ITEM_COUNT: u32 = 200;

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        t.0 = label.into_bytes();
    } else {
        world.insert(entity, Text(label.into_bytes()));
    }
}

extern crate alloc;

fn main() {
    // SDL window with flush throttled. 50 ns/px keeps wall-clock
    // turnaround fast; raise toward 200 (~SPI 80 MHz RGB565) for
    // closer-to-device timing.
    let backend = SlowSurface::new(SdlSurface::new("perf collect", 320, 320), 50);

    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    app.add_plugin(StdInstantClockPlugin::default());
    app.add_plugin(PerfReportPlugin::new(60).with_perfetto_writer("/tmp/mirui-perf.ndjson"));

    app.add_system(sim_timeline_system::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let list = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (
            bg_color: Color::rgb(40, 40, 56),
            grow: 1.0
        ) [
            LazyList::new(ITEM_COUNT, ROW_H, POOL_SIZE as u8),
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                Row (
                    bg_color: Color::rgb(60, 60, 80),
                    text_color: Color::rgb(220, 220, 230),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 320,
                    height: ROW_H
                ) {}
            }
        }
    };

    let pool: alloc::vec::Vec<Entity> = app
        .world
        .get::<Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    app.world.insert(list, LazyListPool::new(pool));

    // Scroll halfway down, scroll back, repeat. Slow drags so spring
    // tail dominates a chunk of frames; fast flicks so inertia gets
    // exercised.
    app.world.insert_resource(SimTimeline::new(vec![
        SimAction::wait(500),
        SimAction::drag(
            DimPoint::percent(50, 80),
            DimPoint::percent(50, 20),
            1500,
            ease::ease_in_out_cubic,
        )
        .on(list),
        SimAction::wait(800),
        SimAction::drag(
            DimPoint::percent(50, 20),
            DimPoint::percent(50, 80),
            1500,
            ease::ease_in_out_cubic,
        )
        .on(list),
        SimAction::wait(800),
        SimAction::drag(
            DimPoint::percent(50, 90),
            DimPoint::percent(50, 10),
            500,
            ease::ease_in_out_cubic,
        )
        .on(list),
        SimAction::wait(2500),
    ]));

    app.set_root(root);
    app.run();
}
