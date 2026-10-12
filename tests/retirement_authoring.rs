use mirui::core::cache::InspectCaches;
use mirui::input::event::input::PointerKind;
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{DisplayInfo, FramebufferAccess, InputEvent};
use std::collections::VecDeque;

struct QueuedSurface {
    pixels: FramebufSurface<fn(&[u8], PhysicalRect)>,
    events: VecDeque<InputEvent>,
}

impl InspectCaches for QueuedSurface {}

impl Surface for QueuedSurface {
    fn display_info(&self) -> DisplayInfo {
        self.pixels.display_info()
    }
    fn flush(&mut self, area: PhysicalRect) {
        self.pixels.flush(area);
    }
    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }
}

impl FramebufferAccess for QueuedSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.pixels.framebuffer()
    }
}

#[model]
#[derive(Default)]
struct State {
    pending: Option<Entity>,
    count: usize,
    ticks: usize,
}

#[model]
impl State {
    fn remove(&mut self, node: Entity) {
        self.pending = Some(node);
    }

    fn pending(&self) -> Option<Entity> {
        self.pending
    }

    fn clear(&mut self) {
        self.pending = None;
    }

    fn observe(&mut self, count: usize) {
        self.count = count;
        self.ticks += 1;
    }

    fn count(&self) -> usize {
        self.count
    }

    fn ticks(&self) -> usize {
        self.ticks
    }
}

#[component]
struct Part {
    state: model!(State),
}

#[system]
fn delete_pending(state: model!(State), commands: commands!()) {
    if let Some(node) = state.pending() {
        state.clear();
        commands.despawn(node);
    }
}

#[system]
fn observe_parts(state: model!(State), parts: query!(Part)) {
    let mut count = 0;
    for part in parts {
        assert_eq!(part.state.pending(), None);
        count += 1;
    }
    state.observe(count);
}

#[view(component = Part)]
fn part_view() {}

#[event]
fn remove_parent(node: entity!(), nodes: query!(mirui::ui::Parent), commands: commands!()) {
    if let Some(parent) = nodes.get(node) {
        commands.despawn(parent.0);
    }
}

#[compose]
fn panel(state: model!(State)) -> Entity {
    ui! {
        View (width: 20, height: 20) [
            Part { state: state.clone() },
        ] {
            View (width: 10, height: 10) [
                Part { state: state.clone() },
            ] on Tap(remove_parent::callback())
        }
    }
}

#[test]
fn subtree_deletion_preserves_shared_models_and_registered_view_systems() {
    let mut app = App::headless(32, 32);
    let state = app.add_model(State::default()).finish();
    app.add_system(delete_pending::system(state.clone()));
    app.with_widget(part_view::view().with_systems([observe_parts::system(state.clone())]));
    let root = app.spawn_root().id();
    let target = app.compose(root, panel(state.clone()));
    app.compose(root, |tree| {
        tree.spawn(Part {
            state: state.clone(),
        })
    });
    app.tick();
    assert_eq!(state.count(), 3);
    state.remove(target);
    app.tick();
    assert_eq!(state.count(), 1);
    let previous = state.ticks();
    app.tick();
    assert!(state.ticks() > previous);
    assert_eq!(state.count(), 1);
}

#[test]
fn deleting_a_parent_in_an_event_retires_descendants_before_the_next_input() {
    let mut app = App::new(QueuedSurface {
        pixels: FramebufSurface::new(32, 32, |_, _| {}),
        events: VecDeque::new(),
    });
    app.with_default_widgets().with_default_systems();
    let state = app.add_model(State::default()).finish();
    app.with_widget(part_view::view().with_systems([observe_parts::system(state.clone())]));
    let root = app.spawn_root().id();
    app.compose(root, panel(state.clone()));
    app.render().unwrap();
    for _ in 0..2 {
        app.backend.events.extend([
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: 4.into(),
                y: 4.into(),
            },
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: 4.into(),
                y: 4.into(),
            },
        ]);
    }
    app.tick();
    assert_eq!(state.count(), 0);
    app.compose(root, panel(state.clone()));
    app.tick();
    assert_eq!(state.count(), 2);
}
