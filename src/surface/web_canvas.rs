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
use super::{BackbufferPersistence, DisplayInfo, InputEvent, PointerKind, Surface};
use crate::core::cache::InspectCaches;
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
    use super::web_input::{PointerPhase, WebPointers};
    let pointers = Rc::new(RefCell::new(WebPointers::new()));
    let mut listeners = Vec::with_capacity(15);
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointerdown",
        PointerPhase::Down,
        |kind, id, x, y| InputEvent::PointerDown { kind, id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointermove",
        PointerPhase::Move,
        |kind, id, x, y| InputEvent::PointerMove { kind, id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointerup",
        PointerPhase::Up,
        |kind, id, x, y| InputEvent::PointerUp { kind, id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointercancel",
        PointerPhase::Cancel,
        |kind, id, x, y| InputEvent::PointerCancel { kind, id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "lostpointercapture",
        PointerPhase::Cancel,
        |kind, id, x, y| InputEvent::PointerCancel { kind, id, x, y },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointerenter",
        PointerPhase::Enter,
        |kind, id, _, _| InputEvent::PointerEnter { kind, id },
    ));
    listeners.push(pointer_listener(
        canvas,
        queue,
        logical_size,
        &pointers,
        "pointerleave",
        PointerPhase::Leave,
        |kind, id, _, _| InputEvent::PointerLeave { kind, id },
    ));
    listeners.push(wheel_listener(canvas, queue, logical_size));
    listeners.push(keyboard_listener(canvas, queue, "keydown", true));
    listeners.push(keyboard_listener(canvas, queue, "keyup", false));
    let window = web_sys::window().expect("no global window");
    let document = canvas.owner_document().expect("canvas document");
    let targets: [EventTarget; 2] = [canvas.clone().into(), window.into()];
    for target in targets {
        for name in ["focus", "blur"] {
            let canvas = canvas.clone();
            let document = document.clone();
            let queue = queue.clone();
            let pointers = pointers.clone();
            let closure = Closure::<dyn FnMut(JsValue)>::new(move |_| {
                enqueue_host_focus(&canvas, &document, &queue, &pointers);
            });
            listeners.push(register_listener(target.clone(), name, closure));
        }
    }
    let observed_document = document.clone();
    let visibility_queue = queue.clone();
    let visibility_pointers = pointers.clone();
    listeners.push(register_listener(
        document.clone().into(),
        "visibilitychange",
        Closure::<dyn FnMut(JsValue)>::new(move |_| {
            if observed_document.hidden() {
                visibility_pointers.borrow_mut().clear();
            }
            visibility_queue
                .borrow_mut()
                .push_back(InputEvent::HostVisibility {
                    visible: !observed_document.hidden(),
                });
        }),
    ));
    enqueue_host_focus(canvas, &document, queue, &pointers);
    queue.borrow_mut().push_back(InputEvent::HostVisibility {
        visible: !document.hidden(),
    });
    listeners
}

fn enqueue_host_focus(
    canvas: &HtmlCanvasElement,
    document: &web_sys::Document,
    queue: &EventQueue,
    pointers: &Rc<RefCell<super::web_input::WebPointers>>,
) {
    if let Ok(focused) = document.has_focus() {
        let focused = focused
            && document
                .active_element()
                .is_some_and(|element| &element == canvas.unchecked_ref::<web_sys::Element>());
        if !focused {
            pointers.borrow_mut().clear();
        }
        queue
            .borrow_mut()
            .push_back(InputEvent::HostFocus { focused });
    }
}

fn pointer_listener(
    canvas: &HtmlCanvasElement,
    queue: &EventQueue,
    logical_size: &LogicalSize,
    pointers: &Rc<RefCell<super::web_input::WebPointers>>,
    name: &str,
    phase: super::web_input::PointerPhase,
    map: fn(PointerKind, u8, Fixed, Fixed) -> InputEvent,
) -> Listener {
    let q = queue.clone();
    let pointers = pointers.clone();
    let types = PointerTypes::new();
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
        let kind = types.classify(&evt);
        let Some(id) = pointers
            .borrow_mut()
            .translate(evt.pointer_id(), kind, phase)
        else {
            return;
        };
        // `offset_x/y` is relative to the canvas backing-store box, which
        // diverges from the CSS box once the canvas is resized (or pointer
        // capture is held); `client_x/y - rect` is the reliable CSS-space
        // coordinate. Fixed logical canvases then map that CSS position back
        // into the demo's retained viewport.
        let rect = canvas_for_rect.get_bounding_client_rect();
        let (x, y) = map_event_coordinates(
            &rect,
            evt.client_x() as f64,
            evt.client_y() as f64,
            logical_size.get(),
        );
        let mut queue = q.borrow_mut();
        queue.push_back(map(kind, id, x, y));
        if matches!(phase, super::web_input::PointerPhase::Enter) {
            queue.push_back(InputEvent::PointerMove { kind, id, x, y });
        }
    });
    register_listener(canvas.clone().into(), name, closure)
}

struct PointerTypes {
    property: JsValue,
    mouse: JsValue,
    touch: JsValue,
    pen: JsValue,
}

impl PointerTypes {
    fn new() -> Self {
        Self {
            property: JsValue::from_str("pointerType"),
            mouse: JsValue::from_str("mouse"),
            touch: JsValue::from_str("touch"),
            pen: JsValue::from_str("pen"),
        }
    }

    fn classify(&self, event: &PointerEvent) -> PointerKind {
        // Keep platform strings in JS rather than allocating a Rust String per event.
        let value =
            js_sys::Reflect::get(event.as_ref(), &self.property).unwrap_or(JsValue::UNDEFINED);
        if value == self.mouse {
            PointerKind::Mouse
        } else if value == self.touch {
            PointerKind::Touch
        } else if value == self.pen {
            PointerKind::Pen
        } else {
            PointerKind::Unknown
        }
    }
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
        let (x, y) = map_event_coordinates(
            &rect,
            evt.client_x() as f64,
            evt.client_y() as f64,
            logical_size.get(),
        );
        let mode = evt.delta_mode();
        let Some((dx, dy)) = super::web_input::wheel_delta(
            evt.delta_x(),
            evt.delta_y(),
            mode,
            (rect.width(), rect.height()),
            logical_size.get(),
        ) else {
            return;
        };
        q.borrow_mut().push_back(InputEvent::Wheel { dx, dy, x, y });
    });
    register_listener(canvas.clone().into(), "wheel", closure)
}

fn map_event_coordinates(
    rect: &web_sys::DomRect,
    client_x: f64,
    client_y: f64,
    logical_size: Option<(u16, u16)>,
) -> (Fixed, Fixed) {
    let (logical_width, logical_height) =
        logical_size.map_or((None, None), |(width, height)| (Some(width), Some(height)));
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
        if let Some(input) = super::web_input::keyboard_input(
            evt.key(),
            pressed,
            crate::input::event::input::KeyModifiers {
                shift: evt.shift_key(),
                control: evt.ctrl_key(),
                alt: evt.alt_key(),
                meta: evt.meta_key(),
            },
            evt.repeat(),
            evt.is_composing(),
        ) {
            if matches!(input, InputEvent::Key { .. }) {
                evt.prevent_default();
            }
            q.borrow_mut().push_back(input);
        }
    });
    register_listener(canvas.clone().into(), name, closure)
}
