//! `web-canvas` Surface — wraps a DOM `<canvas>` and bridges
//! pointer / wheel / keyboard / touch events into mirui's
//! `InputEvent` queue.

#![cfg(target_arch = "wasm32")]

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, EventTarget, HtmlCanvasElement, KeyboardEvent, PointerEvent,
    TouchEvent, WheelEvent,
};

use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface};
use crate::core::cache::InspectCaches;
use crate::input::event::input::{
    KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_ESCAPE, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
};
use crate::render::texture::ColorFormat;
use crate::types::{Fixed, Rect};

type EventQueue = Rc<RefCell<VecDeque<InputEvent>>>;

/// Owned by the surface so `Drop` runs `removeEventListener`.
struct Listener {
    target: EventTarget,
    event: String,
    closure: Closure<dyn FnMut(JsValue)>,
}

/// The backing store is sized to `logical × devicePixelRatio`; the
/// CSS box stays in logical pixels.
pub struct WebCanvasSurface {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    event_queue: EventQueue,
    _listeners: Vec<Listener>,
}

impl WebCanvasSurface {
    /// `canvas` must already be in the DOM with its CSS size set —
    /// mirui only owns the backing store and the 2D context state.
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        sync_canvas_size(&canvas);
        let ctx = canvas
            .get_context("2d")
            .expect("canvas.getContext failed")
            .expect("canvas has no 2d context")
            .dyn_into::<CanvasRenderingContext2d>()
            .expect("getContext('2d') returned a non-2d context");

        let event_queue: EventQueue = Rc::new(RefCell::new(VecDeque::new()));
        let listeners = attach_listeners(&canvas, &event_queue);

        Self {
            canvas,
            ctx,
            event_queue,
            _listeners: listeners,
        }
    }

    /// Returns the underlying 2D canvas context.
    pub fn ctx(&self) -> &CanvasRenderingContext2d {
        &self.ctx
    }

    pub fn canvas(&self) -> &HtmlCanvasElement {
        &self.canvas
    }
}

impl Drop for WebCanvasSurface {
    fn drop(&mut self) {
        // Otherwise the browser keeps invoking the dropped closure.
        for listener in self._listeners.drain(..) {
            let _ = listener.target.remove_event_listener_with_callback(
                &listener.event,
                listener.closure.as_ref().unchecked_ref(),
            );
        }
    }
}

impl InspectCaches for WebCanvasSurface {}

impl Surface for WebCanvasSurface {
    fn display_info(&self) -> DisplayInfo {
        // Re-sync each query so window resizes / OS zoom are picked up
        // without a dedicated `resize` listener.
        let (css_w, css_h, scale) = sync_canvas_size(&self.canvas);
        DisplayInfo {
            width: css_w,
            height: css_h,
            scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: &Rect) {}

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.event_queue.borrow_mut().pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        // `set_width` blanks the backing store on every resize / DPR
        // change, so every frame repaints instead of trusting persistence.
        BackbufferPersistence::Transient
    }
}

/// `set_width` / `set_height` blank the backing store on every
/// assignment, so the `if !=` guards skip same-size frames.
/// Fractional DPR is preserved to match the rendered extent.
fn sync_canvas_size(canvas: &HtmlCanvasElement) -> (u16, u16, Fixed) {
    let window = web_sys::window().expect("no global `window`");
    let dpr = window.device_pixel_ratio().max(1.0);
    let css_w = canvas.client_width().max(1) as u16;
    let css_h = canvas.client_height().max(1) as u16;
    let phys_w = (css_w as f64 * dpr).round() as u32;
    let phys_h = (css_h as f64 * dpr).round() as u32;
    if canvas.width() != phys_w {
        canvas.set_width(phys_w);
    }
    if canvas.height() != phys_h {
        canvas.set_height(phys_h);
    }
    let scale = Fixed::from_f32(dpr as f32);
    (css_w, css_h, scale)
}

fn attach_listeners(canvas: &HtmlCanvasElement, queue: &EventQueue) -> Vec<Listener> {
    let mut listeners = Vec::with_capacity(12);
    listeners.push(pointer_listener(
        canvas,
        queue,
        "pointerdown",
        |id, x, y| InputEvent::PointerDown { id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        "pointermove",
        |id, x, y| InputEvent::PointerMove { id, x, y },
    ));
    listeners.push(pointer_listener(canvas, queue, "pointerup", |id, x, y| {
        InputEvent::PointerUp { id, x, y }
    }));
    listeners.push(pointer_listener(
        canvas,
        queue,
        "pointercancel",
        |id, x, y| InputEvent::PointerUp { id, x, y },
    ));
    listeners.push(leave_listener(canvas, queue));
    listeners.push(wheel_listener(canvas, queue));
    listeners.push(touch_listener(
        canvas,
        queue,
        "touchstart",
        TouchKind::Start,
    ));
    listeners.push(touch_listener(canvas, queue, "touchmove", TouchKind::Move));
    listeners.push(touch_listener(canvas, queue, "touchend", TouchKind::End));
    listeners.push(touch_listener(canvas, queue, "touchcancel", TouchKind::End));
    listeners.push(keyboard_listener(queue, "keydown", true));
    listeners.push(keyboard_listener(queue, "keyup", false));
    listeners
}

fn pointer_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    name: &str,
    map: fn(u8, Fixed, Fixed) -> InputEvent,
) -> Listener {
    let q = queue.clone();
    // Capture so move/up keep firing once the cursor leaves the canvas.
    let capture_on_down = name == "pointerdown";
    let canvas_for_capture = canvas.clone();
    let canvas_for_rect = canvas.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: PointerEvent = raw.unchecked_into();
        evt.prevent_default();
        if capture_on_down {
            let _ = canvas_for_capture.set_pointer_capture(evt.pointer_id());
        }
        let id = (evt.pointer_id().rem_euclid(0xff)) as u8;
        // `offset_x/y` is relative to the canvas backing-store box, which
        // diverges from the CSS box once the canvas is resized (or pointer
        // capture is held); `client_x/y - rect` is the reliable CSS-space
        // coordinate, matching the touch path.
        let rect = canvas_for_rect.get_bounding_client_rect();
        let x = Fixed::from_int((evt.client_x() as f64 - rect.left()).round() as i32);
        let y = Fixed::from_int((evt.client_y() as f64 - rect.top()).round() as i32);
        q.borrow_mut().push_back(map(id, x, y));
    });
    register_listener(canvas.clone().into(), name, closure)
}

fn register_listener(
    target: EventTarget,
    name: &str,
    closure: Closure<dyn FnMut(JsValue)>,
) -> Listener {
    target
        .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
        .expect("addEventListener");
    Listener {
        target,
        event: name.into(),
        closure,
    }
}

/// Synthetic off-screen `PointerMove` so `hover_system` clears the
/// active widget. Skipped while a button is held — the captured
/// pointer is still delivering real coordinates and would race.
fn leave_listener(canvas: &HtmlCanvasElement, queue: &EventQueue) -> Listener {
    let q = queue.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: PointerEvent = raw.unchecked_into();
        if evt.buttons() != 0 {
            return;
        }
        const OFF: i32 = i16::MIN as i32;
        q.borrow_mut().push_back(InputEvent::PointerMove {
            id: 0,
            x: Fixed::from_int(OFF),
            y: Fixed::from_int(OFF),
        });
    });
    register_listener(canvas.clone().into(), "pointerleave", closure)
}

fn wheel_listener(canvas: &HtmlCanvasElement, queue: &EventQueue) -> Listener {
    let q = queue.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: WheelEvent = raw.unchecked_into();
        evt.prevent_default();
        let x = Fixed::from_int(evt.offset_x());
        let y = Fixed::from_int(evt.offset_y());
        // Wheel pixels → scroll-system detents (step = 20). Divisor 4
        // lands an active drag at a comfortable magnitude.
        let dx_units = evt.delta_x() / 4.0;
        let dy_units = evt.delta_y() / 4.0;
        let dx = Fixed::from_f32(dx_units as f32);
        // DOM `deltaY > 0` = content scrolls down; flip to match
        // `scroll_system`'s convention. `dx` keeps the browser sign.
        let dy = Fixed::from_f32(-dy_units as f32);
        q.borrow_mut().push_back(InputEvent::Wheel { dx, dy, x, y });
    });
    register_listener(canvas.clone().into(), "wheel", closure)
}

#[derive(Clone, Copy)]
enum TouchKind {
    Start,
    Move,
    End,
}

fn touch_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    name: &str,
    kind: TouchKind,
) -> Listener {
    let q = queue.clone();
    let canvas_for_rect = canvas.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: TouchEvent = raw.unchecked_into();
        evt.prevent_default();
        // `client_x/y` are viewport-relative — subtract the canvas
        // rect to match pointer events' `offsetX/Y`.
        let rect = canvas_for_rect.get_bounding_client_rect();
        let touches = match kind {
            TouchKind::End => evt.changed_touches(),
            _ => evt.target_touches(),
        };
        for i in 0..touches.length() {
            let Some(touch) = touches.item(i) else {
                continue;
            };
            let x = Fixed::from_int((touch.client_x() as f64 - rect.left()).round() as i32);
            let y = Fixed::from_int((touch.client_y() as f64 - rect.top()).round() as i32);
            let id = (touch.identifier().rem_euclid(0xff)) as u8;
            let event = match kind {
                TouchKind::Start => InputEvent::PointerDown { id, x, y },
                TouchKind::Move => InputEvent::PointerMove { id, x, y },
                TouchKind::End => InputEvent::PointerUp { id, x, y },
            };
            q.borrow_mut().push_back(event);
        }
    });
    register_listener(canvas.clone().into(), name, closure)
}

fn keyboard_listener(queue: &EventQueue, name: &str, pressed: bool) -> Listener {
    let q = queue.clone();
    let window = web_sys::window().expect("no global `window`");
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: KeyboardEvent = raw.unchecked_into();
        let key = evt.key();
        if let Some(code) = map_key(&key) {
            evt.prevent_default();
            q.borrow_mut().push_back(InputEvent::Key { code, pressed });
        }
        if pressed && key.chars().count() == 1 {
            // `key` is post-IME / post-shift / post-dead-key — emit it
            // alongside `Key` so text widgets get both signals.
            if let Some(ch) = key.chars().next() {
                q.borrow_mut().push_back(InputEvent::CharInput { ch });
            }
        }
    });
    register_listener(window.into(), name, closure)
}

fn map_key(key: &str) -> Option<u32> {
    Some(match key {
        "Backspace" => KEY_BACKSPACE,
        "Delete" => KEY_DELETE,
        "ArrowLeft" => KEY_LEFT,
        "ArrowRight" => KEY_RIGHT,
        "Home" => KEY_HOME,
        "End" => KEY_END,
        "Enter" => KEY_RETURN,
        "Escape" => KEY_ESCAPE,
        _ => return None,
    })
}
