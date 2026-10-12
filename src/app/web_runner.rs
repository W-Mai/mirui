use alloc::rc::Rc;
use core::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use super::{App, RendererFactory, Surface};

const EVENTS: &[&str] = &[
    "pointerdown",
    "pointerup",
    "pointercancel",
    "keydown",
    "keyup",
    "copy",
    "cut",
    "paste",
    "beforeinput",
    "input",
    "compositionstart",
    "compositionupdate",
    "compositionend",
    "focusin",
    "focusout",
    "focus",
    "blur",
    "visibilitychange",
];

pub(super) struct InputBoundary {
    window: web_sys::Window,
    callback: Closure<dyn Fn(JsValue)>,
}

impl InputBoundary {
    pub(super) fn new<B, F>(app: &Rc<RefCell<Option<App<B, F>>>>) -> Self
    where
        B: Surface + 'static,
        F: RendererFactory<B> + 'static,
    {
        let app = Rc::downgrade(app);
        let callback = Closure::<dyn Fn(JsValue)>::new(move |_| {
            let Some(app) = app.upgrade() else { return };
            // Focusing the DOM text transport can synchronously dispatch focus events.
            let Ok(mut borrowed) = app.try_borrow_mut() else {
                return;
            };
            if let Some(app) = borrowed.as_mut() {
                app.dispatch_pending_input();
            }
        });
        let window = web_sys::window().expect("no global window");
        for event in EVENTS {
            window
                .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
                .expect("input boundary listener");
        }
        Self { window, callback }
    }
}

impl Drop for InputBoundary {
    fn drop(&mut self) {
        for event in EVENTS {
            let _ = self
                .window
                .remove_event_listener_with_callback(event, self.callback.as_ref().unchecked_ref());
        }
    }
}
