//! `web-canvas` Surface — wraps a DOM `<canvas>` and bridges
//! pointer / wheel / keyboard events into mirui's
//! `InputEvent` queue.

#![cfg(target_arch = "wasm32")]

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, EventTarget, HtmlCanvasElement, KeyboardEvent, PointerEvent,
    WheelEvent,
};

use super::backbuffer_invalidation::BackbufferInvalidation;
use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface};
use crate::core::cache::InspectCaches;
use crate::input::event::input::{
    KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_ESCAPE, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
};
use crate::render::texture::ColorFormat;
use crate::types::Fixed;

type EventQueue = Rc<RefCell<VecDeque<InputEvent>>>;
type LogicalSize = Rc<Cell<Option<(u16, u16)>>>;

/// Owned by the surface so `Drop` runs `removeEventListener`.
struct Listener {
    target: EventTarget,
    event: String,
    closure: Closure<dyn FnMut(JsValue)>,
}

/// The backing store tracks the logical viewport, device scale, and optional
/// uniform CSS display scale.
pub struct WebCanvasSurface {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    backbuffer: BackbufferInvalidation,
    event_queue: EventQueue,
    logical_size: LogicalSize,
    _listeners: Vec<Listener>,
}

impl WebCanvasSurface {
    /// `canvas` must already be in the DOM with its CSS size set —
    /// mirui only owns the backing store and the 2D context state.
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        let logical_size = Rc::new(Cell::new(None));
        let (css_w, css_h, scale, _) = sync_canvas_size(&canvas, logical_size.get());
        let ctx = canvas
            .get_context("2d")
            .expect("canvas.getContext failed")
            .expect("canvas has no 2d context")
            .dyn_into::<CanvasRenderingContext2d>()
            .expect("getContext('2d') returned a non-2d context");

        let event_queue: EventQueue = Rc::new(RefCell::new(VecDeque::new()));
        let listeners = attach_listeners(&canvas, &event_queue, &logical_size);

        Self {
            canvas,
            ctx,
            backbuffer: BackbufferInvalidation::new((css_w, css_h, scale)),
            event_queue,
            logical_size,
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

    pub fn set_logical_size(&self, size: Option<(u16, u16)>) {
        self.logical_size.set(size);
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
        let (css_w, css_h, scale, reset) = sync_canvas_size(&self.canvas, self.logical_size.get());
        self.backbuffer.observe((css_w, css_h, scale), reset);
        DisplayInfo {
            width: css_w,
            height: css_h,
            scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: crate::types::PhysicalRect) {}

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.event_queue.borrow_mut().pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        self.display_info();
        self.backbuffer.take_persistence()
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.canvas.width(), self.canvas.height())
    }
}

/// `set_width` / `set_height` blank the backing store on every
/// assignment, so the `if !=` guards skip same-size frames.
/// Fractional DPR is preserved to match the rendered extent.
fn sync_canvas_size(
    canvas: &HtmlCanvasElement,
    logical_size: Option<(u16, u16)>,
) -> (u16, u16, Fixed, bool) {
    let window = web_sys::window().expect("no global `window`");
    let dpr = window.device_pixel_ratio().max(1.0);
    let display_width = crate::surface::saturating_u16(canvas.client_width().max(1));
    let display_height = crate::surface::saturating_u16(canvas.client_height().max(1));
    let metrics = crate::surface::canvas_metrics(
        display_width,
        display_height,
        Fixed::from_f32(dpr as f32),
        logical_size,
    );
    let reset =
        canvas.width() != metrics.physical_width || canvas.height() != metrics.physical_height;
    if canvas.width() != metrics.physical_width {
        canvas.set_width(metrics.physical_width);
    }
    if canvas.height() != metrics.physical_height {
        canvas.set_height(metrics.physical_height);
    }
    (
        metrics.logical_width,
        metrics.logical_height,
        metrics.scale,
        reset,
    )
}

fn attach_listeners(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    logical_size: &LogicalSize,
) -> Vec<Listener> {
    let mut listeners = Vec::with_capacity(8);
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        "pointerdown",
        |id, x, y| InputEvent::PointerDown { id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        "pointermove",
        |id, x, y| InputEvent::PointerMove { id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        "pointerup",
        |id, x, y| InputEvent::PointerUp { id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        "pointercancel",
        |id, x, y| InputEvent::PointerCancel { id, x, y },
    ));
    listeners.push(leave_listener(canvas, queue));
    listeners.push(wheel_listener(canvas, queue, logical_size));
    listeners.push(keyboard_listener(canvas, queue, "keydown", true));
    listeners.push(keyboard_listener(canvas, queue, "keyup", false));
    listeners
}

fn pointer_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    logical_size: &LogicalSize,
    name: &str,
    map: fn(u8, Fixed, Fixed) -> InputEvent,
) -> Listener {
    let q = queue.clone();
    // Capture so move/up keep firing once the cursor leaves the canvas.
    let capture_on_down = name == "pointerdown";
    let canvas_for_capture = canvas.clone();
    let canvas_for_rect = canvas.clone();
    let logical_size = logical_size.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: PointerEvent = raw.unchecked_into();
        evt.prevent_default();
        if capture_on_down {
            let _ = canvas_for_capture.focus();
            let _ = canvas_for_capture.set_pointer_capture(evt.pointer_id());
        }
        let id = (evt.pointer_id().rem_euclid(0xff)) as u8;
        // `offset_x/y` is relative to the canvas backing-store box, which
        // diverges from the CSS box once the canvas is resized (or pointer
        // capture is held); `client_x/y - rect` is the reliable CSS-space
        // coordinate. Fixed logical canvases then map that CSS position back
        // into the demo's retained viewport.
        let rect = canvas_for_rect.get_bounding_client_rect();
        let (x, y, _, _) = map_event_coordinates(
            &rect,
            evt.client_x() as f64,
            evt.client_y() as f64,
            logical_size.get(),
        );
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

fn wheel_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    logical_size: &LogicalSize,
) -> Listener {
    let q = queue.clone();
    let canvas_for_rect = canvas.clone();
    let logical_size = logical_size.clone();
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
        let evt: WheelEvent = raw.unchecked_into();
        evt.prevent_default();
        let rect = canvas_for_rect.get_bounding_client_rect();
        let (x, y, scale_x, scale_y) = map_event_coordinates(
            &rect,
            evt.client_x() as f64,
            evt.client_y() as f64,
            logical_size.get(),
        );
        // Wheel pixels → scroll-system detents (step = 20). Divisor 4
        // lands an active drag at a comfortable magnitude.
        let dx_units = evt.delta_x() * scale_x / 4.0;
        let dy_units = evt.delta_y() * scale_y / 4.0;
        let dx = Fixed::from_f32(dx_units as f32);
        // DOM `deltaY > 0` = content scrolls down; flip to match
        // `scroll_system`'s convention. `dx` keeps the browser sign.
        let dy = Fixed::from_f32(-dy_units as f32);
        q.borrow_mut().push_back(InputEvent::Wheel { dx, dy, x, y });
    });
    register_listener(canvas.clone().into(), "wheel", closure)
}

fn map_event_coordinates(
    rect: &web_sys::DomRect,
    client_x: f64,
    client_y: f64,
    logical_size: Option<(u16, u16)>,
) -> (Fixed, Fixed, f64, f64) {
    let (logical_width, logical_height) =
        logical_size.map_or((None, None), |(width, height)| (Some(width), Some(height)));
    let scale_x =
        crate::surface::canvas_axis_scale(Fixed::from_f32(rect.width() as f32), logical_width);
    let scale_y =
        crate::surface::canvas_axis_scale(Fixed::from_f32(rect.height() as f32), logical_height);
    (
        crate::surface::canvas_axis_coordinate(
            Fixed::from_f32((client_x - rect.left()) as f32),
            Fixed::from_f32(rect.width() as f32),
            logical_width,
        ),
        crate::surface::canvas_axis_coordinate(
            Fixed::from_f32((client_y - rect.top()) as f32),
            Fixed::from_f32(rect.height() as f32),
            logical_height,
        ),
        f64::from(scale_x.to_f32()),
        f64::from(scale_y.to_f32()),
    )
}

fn keyboard_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    name: &str,
    pressed: bool,
) -> Listener {
    let q = queue.clone();
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
    register_listener(canvas.clone().into(), name, closure)
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
