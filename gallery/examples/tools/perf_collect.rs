//! Deterministic perf-collection harness.
//!
//! Wraps the SDL surface with `SlowSurface` so flush time mimics
//! a 80 MHz SPI display, drives the UI through `SimTimeline`, and
//! prints per-stage and per-system reports via `PerfReportPlugin`.
//!
//! Run:
//!   cargo run --release -p gallery --example perf_collect

use mirui::anim::ease;
use mirui::app::plugins::{PerfReportPlugin, StdInstantClockPlugin};
use mirui::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::surface::slow::SlowSurface;
use mirui::types::{Color, DimPoint, Dimension, Fixed};
use mirui::ui::widgets::LazyList;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::lazy_list::CreateCell;

const ROW_H: i32 = 24;
const ITEM_COUNT: u32 = 200;

#[event]
fn create_row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! {
        Text (
            bg_color: Color::rgb(60, 60, 80),
            text_color: Color::rgb(220, 220, 230),
            text: ${ alloc::format!("Row {}", item.index()) }
        )
    }
}

extern crate alloc;

#[compose]
fn scroll_list() -> Entity {
    ui! {
        View (
            bg_color: Color::rgb(40, 40, 56),
            grow: 1.0
        ) [
            LazyList::new(ITEM_COUNT, ROW_H),
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                ..Default::default()
            },
        ] on LazyList::CreateCell(create_row::callback())
    }
}

fn main() {
    // SDL window with flush throttled. 50 ns/px keeps wall-clock
    // turnaround fast; raise toward 200 (~SPI 80 MHz RGB565) for
    // closer-to-device timing.
    let backend = SlowSurface::new(SdlSurface::new("perf collect", 320, 320), 50);

    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PerfReportPlugin::new(60).with_perfetto_writer("/tmp/mirui-perf.ndjson"));

    app.add_system(sim_timeline_system::system());

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let list = app.compose(root, scroll_list());

    // Scroll halfway down, scroll back, repeat. Slow drags so spring
    // tail dominates a chunk of frames; fast flicks so inertia gets
    // exercised.
    app.add_resource(SimTimeline::new(vec![
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

    app.run();
}
