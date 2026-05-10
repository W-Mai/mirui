//! Verify every Plugin lifecycle hook fires the expected number of times
//! against the real App run path. Uses FramebufBackend so no SDL dependency.

use std::cell::Cell;
use std::rc::Rc;

use mirui::app::{App, RendererFactory};
use mirui::backend::framebuf::FramebufBackend;
use mirui::backend::{Backend, InputEvent};
use mirui::ecs::World;
use mirui::plugin::Plugin;
use mirui::types::Rect;

#[derive(Default)]
struct Counts {
    build: Cell<u32>,
    pre: Cell<u32>,
    post: Cell<u32>,
    event: Cell<u32>,
    quit: Cell<u32>,
}

struct CountPlugin {
    counts: Rc<Counts>,
    consume_next_event: bool,
}

impl<B, F> Plugin<B, F> for CountPlugin
where
    B: Backend,
    F: RendererFactory,
{
    fn build(&mut self, _app: &mut App<B, F>) {
        self.counts.build.set(self.counts.build.get() + 1);
    }
    fn pre_render(&mut self, _world: &mut World) {
        self.counts.pre.set(self.counts.pre.get() + 1);
    }
    fn post_render(&mut self, _world: &mut World, _render_nanos: u64) {
        self.counts.post.set(self.counts.post.get() + 1);
    }
    fn on_event(&mut self, _world: &mut World, _event: &InputEvent) -> bool {
        self.counts.event.set(self.counts.event.get() + 1);
        self.consume_next_event
    }
    fn on_quit(&mut self, _world: &mut World) {
        self.counts.quit.set(self.counts.quit.get() + 1);
    }
}

fn noop_flush(_: &[u8], _: &Rect) {}

#[test]
fn build_fires_once_on_add_plugin() {
    let backend = FramebufBackend::new(64, 64, noop_flush);
    let mut app = App::new(backend);
    let counts = Rc::new(Counts::default());
    app.add_plugin(CountPlugin {
        counts: Rc::clone(&counts),
        consume_next_event: false,
    });
    assert_eq!(counts.build.get(), 1);
}

#[test]
fn pre_and_post_render_fire_per_render_call() {
    let backend = FramebufBackend::new(64, 64, noop_flush);
    let mut app = App::new(backend);
    let counts = Rc::new(Counts::default());
    app.add_plugin(CountPlugin {
        counts: Rc::clone(&counts),
        consume_next_event: false,
    });

    // render() always fires hooks once. render_dirty() also fires once,
    // regardless of whether there was actually a dirty region.
    use mirui::ecs::Entity;
    let root = Entity {
        id: 0,
        generation: 0,
    };
    app.set_root(root);

    app.render();
    app.render_dirty();
    app.render_dirty();

    assert_eq!(counts.pre.get(), 3);
    assert_eq!(counts.post.get(), 3);
}
