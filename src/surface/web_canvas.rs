//! `web-canvas` Surface — wraps a DOM `<canvas>` and bridges
//! pointer / wheel / keyboard / touch events into mirui's
//! `InputEvent` queue.

#![cfg(target_arch = "wasm32")]

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use core::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, KeyboardEvent, PointerEvent, TouchEvent,
    WheelEvent,
};

use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface};
use crate::cache::InspectCaches;
use crate::draw::texture::ColorFormat;
use crate::event::input::{
    KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_ESCAPE, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
};
use crate::types::{Fixed, Rect};

type EventQueue = Rc<RefCell<VecDeque<InputEvent>>>;

/// Surface owning a DOM canvas and its 2D context.
///
/// The caller hands an existing `<canvas>` element; mirui resizes the
/// physical buffer to `logical × devicePixelRatio` and treats the CSS
/// box as logical pixels. Closures forwarding DOM events into the
/// queue are `Closure::forget`-ed and live until page teardown.
pub struct WebCanvasSurface {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    event_queue: EventQueue,
    /// Logical-pixel size; physical is `(width × scale, height × scale)`.
    width: u16,
    height: u16,
    scale: Fixed,
}

impl WebCanvasSurface {
    /// `canvas` must already be in the DOM with its CSS size set —
    /// mirui only owns the backing store and the 2D context state.
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        let window = web_sys::window().expect("no global `window`");
        let dpr = window.device_pixel_ratio().max(1.0);
        let scale = Fixed::from_int(dpr.round() as i32);

        // CSS box → logical pixels. `client_*` returns the rendered
        // size in CSS pixels regardless of the backing store.
        let css_w = canvas.client_width().max(1) as u16;
        let css_h = canvas.client_height().max(1) as u16;
        let phys_w = (css_w as f64 * dpr).round() as u32;
        let phys_h = (css_h as f64 * dpr).round() as u32;
        canvas.set_width(phys_w);
        canvas.set_height(phys_h);

        let ctx = canvas
            .get_context("2d")
            .expect("canvas.getContext failed")
            .expect("canvas has no 2d context")
            .dyn_into::<CanvasRenderingContext2d>()
            .expect("getContext('2d') returned a non-2d context");

        let event_queue: EventQueue = Rc::new(RefCell::new(VecDeque::new()));
        attach_listeners(&canvas, &event_queue);

        Self {
            canvas,
            ctx,
            event_queue,
            width: css_w,
            height: css_h,
            scale,
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

impl InspectCaches for WebCanvasSurface {}

impl Surface for WebCanvasSurface {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: self.scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: &Rect) {}

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.event_queue.borrow_mut().pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        // Canvas 2D retains pixels across paints — `App::run` can take
        // the dirty-only path.
        BackbufferPersistence::Persistent
    }
}

fn attach_listeners(canvas: &HtmlCanvasElement, queue: &EventQueue) {
    pointer_listener(canvas, queue, "pointerdown", |id, x, y| {
        InputEvent::PointerDown { id, x, y }
    });
    pointer_listener(canvas, queue, "pointermove", |id, x, y| {
        InputEvent::PointerMove { id, x, y }
    });
    pointer_listener(canvas, queue, "pointerup", |id, x, y| {
        InputEvent::PointerUp { id, x, y }
    });
    pointer_listener(canvas, queue, "pointercancel", |id, x, y| {
        InputEvent::PointerUp { id, x, y }
    });
    wheel_listener(canvas, queue);
    touch_listener(canvas, queue, "touchstart", TouchKind::Start);
    touch_listener(canvas, queue, "touchmove", TouchKind::Move);
    touch_listener(canvas, queue, "touchend", TouchKind::End);
    touch_listener(canvas, queue, "touchcancel", TouchKind::End);
    keyboard_listener(queue, "keydown", true);
    keyboard_listener(queue, "keyup", false);
}

fn pointer_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    name: &str,
    map: fn(u8, Fixed, Fixed) -> InputEvent,
) {
    let q = queue.clone();
    let cb = Closure::<dyn FnMut(_)>::new(move |evt: PointerEvent| {
        evt.prevent_default();
        let id = (evt.pointer_id().rem_euclid(0xff)) as u8;
        let x = Fixed::from_int(evt.offset_x());
        let y = Fixed::from_int(evt.offset_y());
        q.borrow_mut().push_back(map(id, x, y));
    });
    canvas
        .add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())
        .expect("addEventListener");
    cb.forget();
}

fn wheel_listener(canvas: &HtmlCanvasElement, queue: &EventQueue) {
    let q = queue.clone();
    let cb = Closure::<dyn FnMut(_)>::new(move |evt: WheelEvent| {
        evt.prevent_default();
        let x = Fixed::from_int(evt.offset_x());
        let y = Fixed::from_int(evt.offset_y());
        // `delta_mode == 0` is pixels (the common case for trackpads
        // and high-resolution wheels); modes 1 and 2 (lines / pages)
        // are normalised by the browser to ~16 px / line and viewport
        // height respectively, so passing the raw delta still produces
        // a sensible scroll velocity.
        let dx = Fixed::from_int(evt.delta_x().round() as i32);
        let dy = Fixed::from_int(evt.delta_y().round() as i32);
        // mirui inertial scroll expects a positive `dy` to scroll
        // content up; browsers report positive `deltaY` for scroll
        // down, so flip the sign.
        q.borrow_mut().push_back(InputEvent::Wheel {
            dx: -dx,
            dy: -dy,
            x,
            y,
        });
    });
    canvas
        .add_event_listener_with_callback("wheel", cb.as_ref().unchecked_ref())
        .expect("addEventListener");
    cb.forget();
}

#[derive(Clone, Copy)]
enum TouchKind {
    Start,
    Move,
    End,
}

fn touch_listener(canvas: &HtmlCanvasElement, queue: &EventQueue, name: &str, kind: TouchKind) {
    let q = queue.clone();
    let canvas_for_rect = canvas.clone();
    let cb = Closure::<dyn FnMut(_)>::new(move |evt: TouchEvent| {
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
    canvas
        .add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())
        .expect("addEventListener");
    cb.forget();
}

fn keyboard_listener(queue: &EventQueue, name: &str, pressed: bool) {
    let q = queue.clone();
    let window = web_sys::window().expect("no global `window`");
    let cb = Closure::<dyn FnMut(_)>::new(move |evt: KeyboardEvent| {
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
    window
        .add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())
        .expect("addEventListener");
    cb.forget();
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
