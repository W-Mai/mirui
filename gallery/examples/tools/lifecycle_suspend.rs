//! Standalone host-loop demo of `App::suspend()` / `App::resume()`.
//!
//! Runs **outside** `gallery::run` because the gallery framework owns
//! its tick loop and doesn't surface a place to peek at host events.
//! This binary drives `App::tick` itself so the suspend/resume API can
//! be exercised from the host.
//!
//! Controls:
//!   - ENTER / RETURN  toggle suspend / resume
//!   - ESC             quit
//!
//! On-screen:
//!   - current state (running / suspended)
//!   - tick counter (advances while running, frozen while suspended)
//!   - on_suspend / on_resume hook fire counts

extern crate alloc;

use mirui::app::App;
use mirui::app::plugin::Plugin;
use mirui::app::plugins::StdInstantClockPlugin;
use mirui::core::reactive::Signal;
use mirui::ecs::{Entity, World};
use mirui::input::event::input::{InputEvent, KEY_ESCAPE, KEY_RETURN};
use mirui::prelude::*;
use mirui::render::factory::RendererFactory;
use mirui::surface::Surface;
use mirui::surface::sdl::SdlSurface;

#[derive(Clone)]
struct LifecycleCounters {
    on_suspend: Signal<u32>,
    on_resume: Signal<u32>,
    ticks: Signal<u32>,
    suspended: Signal<bool>,
}

impl LifecycleCounters {
    fn new() -> Self {
        Self {
            on_suspend: Signal::new(0),
            on_resume: Signal::new(0),
            ticks: Signal::new(0),
            suspended: Signal::new(false),
        }
    }
}

struct LifecycleTracePlugin {
    counters: LifecycleCounters,
}

impl<B, F> Plugin<B, F> for LifecycleTracePlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}
    fn on_suspend(&mut self, _world: &mut World) {
        self.counters.on_suspend.update(|n| *n += 1);
        self.counters.suspended.set(true);
    }
    fn on_resume(&mut self, _world: &mut World) {
        self.counters.on_resume.update(|n| *n += 1);
        self.counters.suspended.set(false);
    }
    fn post_render(&mut self, _world: &mut World, _render_nanos: u64) {
        self.counters.ticks.update(|n| *n += 1);
    }
}

fn build_ui(world: &mut World, parent: Entity, counters: LifecycleCounters) {
    let state_label = counters.suspended.clone();
    let ticks_label = counters.ticks.clone();
    let suspend_label = counters.on_suspend.clone();
    let resume_label = counters.on_resume.clone();

    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            padding: Padding::all(16),
            justify: JustifyContent::Center,
            align: AlignItems::Center
        ) {
            View (
                height: 40,
                text: ${
                    if state_label.get() {
                        alloc::string::String::from("State: SUSPENDED")
                    } else {
                        alloc::string::String::from("State: running")
                    }
                }
            )
            View (
                height: 32,
                text: ${ alloc::format!("Ticks: {}", ticks_label.get()) }
            )
            View (
                height: 32,
                text: ${ alloc::format!("on_suspend: {}", suspend_label.get()) }
            )
            View (
                height: 32,
                text: ${ alloc::format!("on_resume: {}", resume_label.get()) }
            )
            View (
                height: 24,
                text: "ENTER toggle suspend, ESC quit"
            )
        }
    };
}

fn main() {
    let backend = SdlSurface::new("mirui - lifecycle suspend", 480, 240);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(StdInstantClockPlugin);

    let counters = LifecycleCounters::new();
    app.add_plugin(LifecycleTracePlugin {
        counters: counters.clone(),
    });

    let parent = app.spawn_root().id();
    build_ui(&mut app.world, parent, counters);

    loop {
        if let Some(event) = app.backend.poll_event() {
            match event {
                InputEvent::Quit => break,
                InputEvent::Key {
                    code: KEY_ESCAPE,
                    pressed: true,
                } => break,
                InputEvent::Key {
                    code: KEY_RETURN,
                    pressed: true,
                } => {
                    if app.is_suspended() {
                        app.resume();
                    } else {
                        app.suspend();
                    }
                    continue;
                }
                _ => {
                    // Drop host-intercepted lifecycle events; the tick
                    // loop will poll its own input for widget dispatch.
                }
            }
        }
        if app.tick() {
            break;
        }
    }
}
