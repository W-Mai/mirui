use super::{EventQueue, Listener, LogicalSize, keyboard_listener, register_listener};
use alloc::{format, rc::Rc, vec::Vec};
use core::cell::{Cell, RefCell};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{CompositionEvent, Document, Element, HtmlCanvasElement, HtmlTextAreaElement};

use crate::input::event::input::{InputEvent, KEY_BACKSPACE, KEY_DELETE, KEY_RETURN};
use crate::surface::web_input::{TextInputAction, text_input_action};
use crate::ui::text_input::{TextInputSession, TextInputState};

pub(super) struct DomFocus {
    canvas: HtmlCanvasElement,
    input: RefCell<Option<HtmlTextAreaElement>>,
    pub transferring: Cell<bool>,
}

impl DomFocus {
    pub fn contains(&self, element: &Element) -> bool {
        element == self.canvas.unchecked_ref::<Element>()
            || self
                .input
                .borrow()
                .as_ref()
                .is_some_and(|input| element == input.unchecked_ref::<Element>())
    }

    pub fn focused(&self, document: &Document) -> bool {
        document.has_focus().unwrap_or(false)
            && document
                .active_element()
                .as_ref()
                .is_some_and(|element| self.contains(element))
    }
}

pub(super) struct NativeTextInput {
    pub focus: Rc<DomFocus>,
    queue: EventQueue,
    logical_size: LogicalSize,
    active: Option<(TextInputState, Transport)>,
}

impl NativeTextInput {
    pub fn new(canvas: &HtmlCanvasElement, queue: &EventQueue, logical_size: &LogicalSize) -> Self {
        Self {
            focus: Rc::new(DomFocus {
                canvas: canvas.clone(),
                input: RefCell::new(None),
                transferring: Cell::new(false),
            }),
            queue: queue.clone(),
            logical_size: logical_size.clone(),
            active: None,
        }
    }

    pub fn set(&mut self, next: Option<TextInputState>) {
        let canvas = &self.focus.canvas;
        let document = canvas.owner_document().expect("canvas document");
        let changed =
            self.active.as_ref().map(|(state, _)| state.session) != next.map(|state| state.session);
        if changed {
            let owned_focus = self.focus.focused(&document);
            self.focus.transferring.set(true);
            self.active.take();
            self.focus.input.borrow_mut().take();
            if let Some(state) = next {
                let transport = Transport::new(&document, state.session, &self.queue);
                *self.focus.input.borrow_mut() = Some(transport.input.clone());
                transport.position(canvas, self.logical_size.get(), state);
                if owned_focus {
                    transport.focus();
                }
                self.active = Some((state, transport));
            } else if owned_focus {
                let options = web_sys::FocusOptions::new();
                options.set_prevent_scroll(true);
                let _ = canvas.focus_with_options(&options);
            }
            self.focus.transferring.set(false);
        }
        if let (Some(next), Some((state, transport))) = (next, self.active.as_mut()) {
            *state = next;
            transport.position(canvas, self.logical_size.get(), next);
            if document.has_focus().unwrap_or(false)
                && document.active_element().as_ref() == Some(canvas.unchecked_ref::<Element>())
            {
                self.focus.transferring.set(true);
                transport.focus();
                self.focus.transferring.set(false);
            }
        }
    }
}

struct Transport {
    input: HtmlTextAreaElement,
    listeners: Vec<Listener>,
    geometry: Cell<Option<[f64; 3]>>,
}

impl Transport {
    fn new(document: &Document, session: TextInputSession, queue: &EventQueue) -> Self {
        let input = document
            .create_element("textarea")
            .expect("textarea")
            .unchecked_into::<HtmlTextAreaElement>();
        input.set_tab_index(-1);
        input.set_spellcheck(false);
        for (name, value) in [
            ("aria-label", "Text input"),
            ("autocomplete", "off"),
            ("autocapitalize", "off"),
            ("data-mirui-text-input", ""),
            (
                "style",
                "position:fixed;opacity:0;pointer-events:none;resize:none;overflow:hidden;width:1px;min-width:0;min-height:0;padding:0;border:0;margin:0;font-size:16px;",
            ),
        ] {
            input
                .set_attribute(name, value)
                .expect("textarea attribute");
        }
        document
            .body()
            .expect("document body")
            .append_child(&input)
            .expect("text transport");
        let composing = Rc::new(Cell::new(false));
        let mut listeners = Vec::with_capacity(6);
        for name in ["compositionstart", "compositionend"] {
            let event_target = input.clone().into();
            let input = input.clone();
            let queue = queue.clone();
            let composing = composing.clone();
            let callback = Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
                if name == "compositionstart" {
                    composing.set(true);
                    return;
                }
                if !composing.replace(false) {
                    return;
                }
                let event: CompositionEvent = raw.unchecked_into();
                let text = event.data().unwrap_or_default();
                input.set_value("");
                queue.borrow_mut().push_back(if text.is_empty() {
                    session.cancel()
                } else {
                    session.commit(text)
                });
            });
            listeners.push(register_listener(event_target, name, callback));
        }
        {
            let target = input.clone();
            let queue = queue.clone();
            let composing = composing.clone();
            listeners.push(register_listener(
                input.clone().into(),
                "input",
                Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
                    let event: web_sys::InputEvent = raw.unchecked_into();
                    let action = text_input_action(composing.get(), &event.input_type());
                    if action == TextInputAction::Preview {
                        let text = target.value();
                        let selection = target
                            .selection_start()
                            .ok()
                            .flatten()
                            .zip(target.selection_end().ok().flatten())
                            .and_then(|(start, end)| {
                                crate::surface::text_input_range::utf8_selection(
                                    &text,
                                    start as usize..end as usize,
                                    true,
                                )
                            });
                        queue
                            .borrow_mut()
                            .push_back(session.preedit(text, selection));
                    } else {
                        let text = target.value();
                        target.set_value("");
                        if !text.is_empty() && action == TextInputAction::Commit {
                            queue.borrow_mut().push_back(session.commit(text));
                        }
                    }
                }),
            ));
        }
        {
            let queue = queue.clone();
            let composing = composing.clone();
            listeners.push(register_listener(
                input.clone().into(),
                "beforeinput",
                Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
                    let event: web_sys::InputEvent = raw.unchecked_into();
                    if event.input_type() == "insertFromPaste" {
                        event.prevent_default();
                        return;
                    }
                    if composing.get() || event.is_composing() {
                        return;
                    }
                    let kind = event.input_type();
                    let code = match kind.as_str() {
                        "deleteContentBackward" => Some(KEY_BACKSPACE),
                        "deleteContentForward" => Some(KEY_DELETE),
                        "insertLineBreak" | "insertParagraph" => Some(KEY_RETURN),
                        "insertText" | "insertCompositionText" | "insertFromComposition" => return,
                        _ => None,
                    };
                    event.prevent_default();
                    if let Some(code) = code {
                        let mut queue = queue.borrow_mut();
                        for pressed in [true, false] {
                            queue.push_back(InputEvent::Key {
                                code,
                                pressed,
                                modifiers: Some(Default::default()),
                                repeat: Some(false),
                            });
                        }
                    }
                }),
            ));
        }
        listeners.push(keyboard_listener(
            input.clone().into(),
            queue,
            "keydown",
            true,
            Some(composing.clone()),
        ));
        listeners.push(keyboard_listener(
            input.clone().into(),
            queue,
            "keyup",
            false,
            Some(composing),
        ));
        Self {
            input,
            listeners,
            geometry: Cell::new(None),
        }
    }

    fn focus(&self) {
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        let _ = self.input.focus_with_options(&options);
    }

    fn position(
        &self,
        canvas: &HtmlCanvasElement,
        logical: Option<(u16, u16)>,
        state: TextInputState,
    ) {
        let rect = canvas.get_bounding_client_rect();
        let (width, height) = logical.map_or(
            (
                f64::from(canvas.client_width().max(1)),
                f64::from(canvas.client_height().max(1)),
            ),
            |(w, h)| (f64::from(w.max(1)), f64::from(h.max(1))),
        );
        let (x, y, h) = state.caret.map_or((0.0, 0.0, 16.0), |caret| {
            (
                f64::from(caret.x.to_f32()),
                f64::from(caret.y.to_f32()),
                f64::from(caret.h.to_f32()),
            )
        });
        let geometry = [
            rect.left() + x * rect.width() / width,
            rect.top() + y * rect.height() / height,
            (h * rect.height() / height).max(1.0),
        ];
        if self.geometry.replace(Some(geometry)) == Some(geometry) {
            return;
        }
        for (name, value) in ["left", "top", "height"].into_iter().zip(geometry) {
            let _ = self.input.style().set_property(name, &format!("{value}px"));
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.listeners.clear();
        self.input.remove();
    }
}
