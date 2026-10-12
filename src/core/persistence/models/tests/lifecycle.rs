use super::*;
use crate::{
    core::persistence::PersistencePlugin,
    surface::{DisplayInfo, FramebufferAccess, InputEvent, Surface, framebuf::FramebufSurface},
    types::PhysicalRect,
};
use alloc::collections::VecDeque;
use core::cell::Cell;

struct LifecycleSurface {
    frame: FramebufSurface<fn(&[u8], PhysicalRect)>,
    events: Rc<RefCell<VecDeque<InputEvent>>>,
}

impl crate::core::cache::InspectCaches for LifecycleSurface {}

impl Surface for LifecycleSurface {
    fn display_info(&self) -> DisplayInfo {
        self.frame.display_info()
    }
    fn flush(&mut self, area: PhysicalRect) {
        self.frame.flush(area);
    }
    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.borrow_mut().pop_front()
    }
}

impl FramebufferAccess for LifecycleSurface {
    fn framebuffer(&mut self) -> crate::render::texture::Texture<'_> {
        self.frame.framebuffer()
    }
}

#[test]
fn failed_saves_retry_at_save_boundaries_and_disabled_lifecycle_performs_no_io() {
    std::thread_local! { static NOW: Cell<u64> = const { Cell::new(0) }; }
    NOW.set(0);
    let store = Store::default();
    let events = Rc::new(RefCell::new(VecDeque::new()));
    let mut app = App::new(LifecycleSurface {
        frame: FramebufSurface::new(16, 16, |_, _| {}),
        events: events.clone(),
    });
    app.add_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    app.add_plugin(PersistencePlugin::new(store.clone()).autosave(1));
    app.spawn_root().id();
    let model = app.add_model(settings(1)).finish();
    store.0.borrow_mut().fail_write = true;
    NOW.set(1_000_000);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 1);
    assert_eq!(
        app.persistence().status(&model),
        PersistenceStatus::Failed {
            error: PersistenceError::Storage(StorageError::Io),
            protected: false,
        }
    );
    model.set(2);
    store.0.borrow_mut().fail_write = false;
    NOW.set(1_999_999);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 1);
    NOW.set(2_000_000);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 2);
    assert_eq!(stored(&store, "settings"), 2);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);

    app.persistence().set_enabled(false).unwrap();
    model.set(3);
    let reads = store.0.borrow().reads;
    events.borrow_mut().push_back(InputEvent::AppSuspend);
    assert!(!app.tick());
    events.borrow_mut().push_back(InputEvent::AppResume);
    NOW.set(20_000_000);
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 2);
    assert_eq!(store.0.borrow().reads, reads);
    assert_eq!(
        app.persistence().status(&model),
        PersistenceStatus::Disabled
    );
    app.persistence().set_enabled(true).unwrap();
    assert_eq!(model.value(), 3);
    assert_eq!(store.0.borrow().reads, reads);
    store.0.borrow_mut().fail_write = true;
    app.suspend();
    assert_eq!(store.0.borrow().writes, 3);
    assert_eq!(stored(&store, "settings"), 2);
    store.0.borrow_mut().fail_write = false;
    app.resume();
    events.borrow_mut().push_back(InputEvent::Quit);
    assert!(app.tick());
    assert_eq!(store.0.borrow().writes, 4);
    assert_eq!(stored(&store, "settings"), 3);
    assert_eq!(app.persistence().status(&model), PersistenceStatus::Clean);
    assert!(app.tick());
    assert_eq!(store.0.borrow().writes, 4);
}

#[test]
fn suspend_and_quit_save_current_models_without_an_autosave_clock() {
    let store = Store::default();
    let events = Rc::new(RefCell::new(VecDeque::new()));
    let mut app = App::new(LifecycleSurface {
        frame: FramebufSurface::new(16, 16, |_, _| {}),
        events: events.clone(),
    });
    app.add_plugin(PersistencePlugin::new(store.clone()).autosave(1));
    app.spawn_root().id();
    let model = app.add_model(settings(4)).finish();
    assert!(!app.tick());
    assert!(!app.tick());
    assert_eq!(store.0.borrow().writes, 0);
    app.suspend();
    assert_eq!(stored(&store, "settings"), 4);
    app.resume();
    model.set(5);
    events.borrow_mut().push_back(InputEvent::Quit);
    assert!(app.tick());
    assert_eq!(stored(&store, "settings"), 5);
    assert_eq!(store.0.borrow().writes, 2);
}
