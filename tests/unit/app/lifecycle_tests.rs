use super::*;
use alloc::rc::Rc;
use core::cell::RefCell;

struct SpawnBeforeRender(bool);

impl<B, F> Plugin<B, F> for SpawnBeforeRender
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn pre_render(&mut self, world: &mut World) {
        if !self.0 {
            world.spawn_empty();
            self.0 = true;
        }
    }
}

#[test]
fn render_prepares_dirty_storage_after_plugin_spawns() {
    let mut app = App::headless(32, 32);
    let initial = app.world.live_entity_count();
    app.add_plugin(SpawnBeforeRender(false));
    let viewport = app.backend.viewport();
    app.prepare_render(viewport);

    let slots = app.world.allocated_entity_slots() as usize;
    let prepared = app
        .world
        .storage::<crate::ui::dirty::Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert_eq!(app.world.live_entity_count(), initial + 1);
    assert!(prepared.0 >= slots);
    assert!(prepared.1 >= app.world.live_entity_count());
}

#[test]
fn dirty_queries_do_not_run_render_hooks() {
    let mut app = App::headless(32, 32);
    app.add_plugin(SpawnBeforeRender(false));
    app.spawn_root().id();
    let initial = app.world.live_entity_count();
    for _ in 0..3 {
        assert_eq!(app.dirty_region().unwrap(), Some(Rect::new(0, 0, 32, 32)));
    }
    assert_eq!(app.world.live_entity_count(), initial);
    app.render_dirty().unwrap();
    assert_eq!(app.world.live_entity_count(), initial + 1);
    assert!(app.dirty_regions().unwrap().is_empty());
}

#[test]
fn dirty_dense_reservation_excludes_released_entity_slots() {
    let mut app = App::headless(32, 32);
    let entities: alloc::vec::Vec<_> = (0..64).map(|_| app.world.spawn_empty()).collect();
    for entity in entities {
        assert!(app.world.despawn(entity));
    }
    app.prepare_dirty_storage();

    let prepared = app
        .world
        .storage::<crate::ui::dirty::Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert!(prepared.0 >= app.world.allocated_entity_slots() as usize);
    assert!(prepared.1 < prepared.0);
}

#[derive(Default, Clone)]
struct Trace {
    on_start: u32,
    on_suspend: u32,
    on_resume: u32,
    on_memory_warning: u32,
    user_inputs: u32,
    lifecycle_requests: u32,
    on_quit: u32,
}

struct TracePlugin {
    trace: Rc<RefCell<Trace>>,
}

impl<B, F> Plugin<B, F> for TracePlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}
    fn on_start(&mut self, _world: &mut World) {
        self.trace.borrow_mut().on_start += 1;
    }
    fn on_suspend(&mut self, _world: &mut World) {
        self.trace.borrow_mut().on_suspend += 1;
    }
    fn on_resume(&mut self, _world: &mut World) {
        self.trace.borrow_mut().on_resume += 1;
    }
    fn on_memory_warning(&mut self, _world: &mut World) {
        self.trace.borrow_mut().on_memory_warning += 1;
    }
    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        use crate::app::lifecycle::SuspendRequest;
        match event {
            InputEvent::AppSuspend => {
                self.trace.borrow_mut().lifecycle_requests += 1;
                world.insert_resource(SuspendRequest::Suspend);
            }
            InputEvent::AppResume => {
                self.trace.borrow_mut().lifecycle_requests += 1;
                world.insert_resource(SuspendRequest::Resume);
            }
            _ => self.trace.borrow_mut().user_inputs += 1,
        }
        false
    }
    fn on_quit(&mut self, _world: &mut World) {
        self.trace.borrow_mut().on_quit += 1;
    }
}

fn fresh() -> (
    App<crate::surface::framebuf::FramebufSurface<HeadlessFlush>>,
    Rc<RefCell<Trace>>,
) {
    let mut app = App::headless(32, 32);
    let trace = Rc::new(RefCell::new(Trace::default()));
    app.add_plugin(TracePlugin {
        trace: trace.clone(),
    });
    (app, trace)
}

#[test]
fn suspend_resume_toggle_fires_hooks_once() {
    let (mut app, trace) = fresh();
    app.suspend();
    app.suspend();
    app.resume();
    app.resume();
    let t = trace.borrow();
    assert_eq!(t.on_suspend, 1, "duplicate suspend must not re-fire");
    assert_eq!(t.on_resume, 1, "duplicate resume must not re-fire");
}

#[test]
fn is_suspended_reflects_state() {
    let (mut app, _) = fresh();
    assert!(!app.is_suspended());
    app.suspend();
    assert!(app.is_suspended());
    app.resume();
    assert!(!app.is_suspended());
}

#[test]
fn tick_short_circuits_while_suspended() {
    let (mut app, _) = fresh();
    app.suspend();
    let quit = app.tick();
    assert!(!quit, "suspended tick returns without firing on_quit");
    assert!(
        app.is_suspended(),
        "tick must not flip the suspend state without a request"
    );
}

#[test]
fn simulation_blocks_user_input_but_preserves_suspend_resume_and_quit() {
    use super::queued_surface::QueuedSurface;
    use crate::input::event::sim::{SimAction, SimTimeline};

    let mut app = App::new(QueuedSurface::new(32, 32, crate::types::Fixed::ONE));
    let trace = Rc::new(RefCell::new(Trace::default()));
    app.add_plugin(TracePlugin {
        trace: trace.clone(),
    });
    app.world
        .insert_resource(SimTimeline::new(alloc::vec![SimAction::wait(1000)]));
    let input = InputEvent::Key {
        physical: None,
        code: 1,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    app.backend
        .events
        .extend([input.clone(), InputEvent::AppSuspend]);
    assert!(!app.tick());
    assert!(app.is_suspended());
    assert_eq!(trace.borrow().user_inputs, 0);
    assert_eq!(trace.borrow().on_suspend, 1);

    app.backend.events.push_back(InputEvent::AppResume);
    assert!(!app.tick());
    assert!(!app.is_suspended());
    assert_eq!(trace.borrow().on_resume, 1);
    assert_eq!(trace.borrow().lifecycle_requests, 2);

    app.backend.events.extend([input, InputEvent::Quit]);
    assert!(app.tick());
    assert_eq!(trace.borrow().on_quit, 1);
    assert_eq!(trace.borrow().user_inputs, 0);
    assert!(app.world.resource::<SimTimeline>().unwrap().is_running());
}

#[test]
fn suspend_request_resource_drives_state() {
    let (mut app, trace) = fresh();
    app.world
        .insert_resource(crate::app::lifecycle::SuspendRequest::Suspend);
    app.tick();
    assert!(app.is_suspended(), "Suspend request drained into state");
    app.world
        .insert_resource(crate::app::lifecycle::SuspendRequest::Resume);
    app.tick();
    assert!(!app.is_suspended(), "Resume request drained into state");
    let t = trace.borrow();
    assert_eq!(t.on_suspend, 1);
    assert_eq!(t.on_resume, 1);
}

#[test]
fn memory_warning_preserves_state_and_notifies_plugins() {
    let (mut app, trace) = fresh();
    let root = app.spawn_root().id();
    app.set_root(root);
    let path = app
        .paths()
        .insert(crate::render::path::Path::new())
        .unwrap();

    app.handle_memory_warning();

    assert_eq!(trace.borrow().on_memory_warning, 1);
    assert!(app.paths().with(path, |_| ()).is_ok());
    assert_eq!(app.root, Some(root));
}
