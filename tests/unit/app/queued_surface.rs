use super::*;
use crate::core::cache::InspectCaches;
use crate::render::texture::Texture;
use crate::surface::framebuf::FramebufSurface;
use crate::surface::{DisplayInfo, FramebufferAccess, InputEvent};
use crate::types::{Fixed, PhysicalRect};
use alloc::collections::VecDeque;

type FlushFn = fn(&[u8], PhysicalRect);

pub(super) struct QueuedSurface {
    framebuffer: FramebufSurface<FlushFn>,
    pub(super) events: VecDeque<InputEvent>,
    pub(super) text_input: Option<crate::ui::text_input::TextInputState>,
    clipboard: Option<crate::ui::clipboard::ClipboardWrite>,
    clipboard_read: Option<crate::ui::clipboard::ClipboardRead>,
}

impl QueuedSurface {
    pub(super) fn new(physical_w: u16, physical_h: u16, scale: Fixed) -> Self {
        fn flush(_: &[u8], _: PhysicalRect) {}
        Self {
            framebuffer: FramebufSurface::with_scale(physical_w, physical_h, scale, flush),
            events: VecDeque::new(),
            text_input: None,
            clipboard: None,
            clipboard_read: None,
        }
    }
}

impl InspectCaches for QueuedSurface {}

impl Surface for QueuedSurface {
    fn display_info(&self) -> DisplayInfo {
        self.framebuffer.display_info()
    }

    fn physical_size(&self) -> (u32, u32) {
        self.framebuffer.physical_size()
    }

    fn flush(&mut self, area: PhysicalRect) -> Result<(), crate::render::RenderError> {
        self.framebuffer.flush(area)
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }

    fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.text_input = state;
    }

    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        assert!(self.clipboard.replace(request).is_none());
        crate::ui::clipboard::ClipboardDispatch::Pending
    }

    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        assert!(self.clipboard_read.replace(request).is_none());
        crate::ui::clipboard::ClipboardDispatch::Pending
    }
}

impl FramebufferAccess for QueuedSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.framebuffer.framebuffer()
    }
}

#[test]
fn clipboard_acknowledgments_complete_once_in_active_and_suspended_apps() {
    use crate::prelude::*;
    use crate::ui::clipboard::{ClipboardFinished, ClipboardOutcome};
    use crate::ui::widgets::{TextEditing, TextInput, TextSelection};
    for suspended in [false, true] {
        let mut app = App::new(QueuedSurface::new(64, 32, Fixed::ONE));
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let mut editor = TextEditing::default().with_text("copy me").unwrap();
        editor
            .set_selection(TextSelection {
                anchor: 0,
                cursor: 4,
            })
            .unwrap();
        let results = alloc::rc::Rc::new(core::cell::Cell::new(0));
        let capture = results.clone();
        let node = app.compose(root, |cx| {
            crate::ui! {
                TextInput () [
                    editor,
                ] on ClipboardFinished(move |event: &ClipboardFinished| {
                    assert_eq!(event.result, Ok(ClipboardOutcome::Copied));
                    capture.set(capture.get() + 1);
                })
            }
        });
        let button = app.compose(root, |cx| {
            crate::ui! {
                View () on Tap { commands!().clipboard().copy(node); }
            }
        });
        crate::ui::delivery::dispatch_gesture(
            &mut app.world,
            &crate::input::event::gesture::GestureEvent::Tap {
                target: button,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            0,
        )
        .unwrap();
        app.poll_event();
        let write = app.backend.clipboard.take().unwrap();
        assert_eq!(write.data().plain_text(), Ok("copy"));
        assert_eq!(results.get(), 0);
        if suspended {
            app.suspend();
        }
        let response = write.complete(Ok(()));
        app.backend
            .events
            .push_back(InputEvent::Clipboard(response.clone()));
        app.backend
            .events
            .push_back(InputEvent::Clipboard(response));
        assert!(!app.dispatch_pending_input());
        assert_eq!(results.get(), 1);
        assert_eq!(app.is_suspended(), suspended);
    }
}

#[test]
fn clipboard_reads_apply_once_in_active_and_suspended_apps() {
    use crate::prelude::*;
    use crate::ui::clipboard::{ClipboardData, ClipboardFinished, ClipboardOutcome};
    use crate::ui::widgets::{TextEditing, TextInput, TextSelection};
    for suspended in [false, true] {
        let mut app = App::new(QueuedSurface::new(64, 32, Fixed::ONE));
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let mut editor = TextEditing::default().with_text("hello world").unwrap();
        editor
            .set_selection(TextSelection {
                anchor: 6,
                cursor: 11,
            })
            .unwrap();
        let results = alloc::rc::Rc::new(core::cell::Cell::new(0));
        let capture = results.clone();
        let node = app.compose(root, |cx| {
            crate::ui! {
                TextInput () [
                    editor,
                ] on ClipboardFinished(move |event: &ClipboardFinished| {
                    assert_eq!(event.result, Ok(ClipboardOutcome::Pasted));
                    capture.set(capture.get() + 1);
                })
            }
        });
        let button = app.compose(root, |cx| {
            crate::ui! {
                View () on Tap { commands!().clipboard().paste(node); }
            }
        });
        crate::ui::delivery::dispatch_gesture(
            &mut app.world,
            &crate::input::event::gesture::GestureEvent::Tap {
                target: button,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            0,
        )
        .unwrap();
        app.poll_event();
        let read = app.backend.clipboard_read.take().unwrap();
        assert_eq!(results.get(), 0);
        if suspended {
            app.suspend();
        }
        let response = read.complete(ClipboardData::text("mirui"));
        app.backend
            .events
            .push_back(InputEvent::Clipboard(response.clone()));
        app.backend
            .events
            .push_back(InputEvent::Clipboard(response));
        assert!(!app.dispatch_pending_input());
        app.world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| assert_eq!(text, "hello mirui"));
        assert_eq!(results.get(), 1);
        assert_eq!(app.is_suspended(), suspended);
    }
}

#[test]
fn native_editor_context_is_updated_before_poll_and_after_layout() {
    use crate::input::event::focus::FocusState;
    use crate::ui::widgets::TextInput;
    #[crate::compose]
    fn panel() -> Entity {
        crate::ui! {
            TextInput (width: 160, height: 36)
        }
    }
    let mut app = App::new(QueuedSurface::new(400, 200, Fixed::from_int(2)));
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let editor = app.compose(root, panel());
    app.render().unwrap();
    assert!(app.backend.text_input.is_none());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    assert!(app.poll_event().is_none());
    let first = app.backend.text_input.unwrap();
    assert!(first.caret.is_some());
    crate::input::event::deliver_input(
        &mut app.world,
        Some(root),
        &first.session.commit("abc"),
        0,
        200,
        100,
    );
    app.render_dirty().unwrap();
    let next = app.backend.text_input.unwrap();
    assert_eq!(first.session, next.session);
    assert!(next.caret.unwrap().x > first.caret.unwrap().x);
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    app.poll_event();
    assert!(app.backend.text_input.is_none());
}

#[test]
fn input_boundary_establishes_a_session_and_edits_without_rendering() {
    use crate::ui::widgets::{TextEditing, TextInput};
    #[crate::compose]
    fn panel() -> Entity {
        crate::ui! {
            TextInput (width: 160, height: 36)
        }
    }
    let mut app = App::new(QueuedSurface::new(200, 100, Fixed::ONE));
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let editor = app.compose(root, panel());
    app.render().unwrap();
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.backend.events.extend([
        InputEvent::PointerDown {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        },
        InputEvent::PointerUp {
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        },
    ]);
    assert!(!app.dispatch_pending_input());
    let session = app.backend.text_input.unwrap().session;
    app.backend.events.push_back(session.commit("abc"));
    assert!(!app.dispatch_pending_input());
    app.world.get::<TextEditing>(editor).unwrap().read(|text| {
        assert_eq!(text, "abc");
    });
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );
    assert!(!app.dispatch_pending_input());
    assert_eq!(app.world.get::<TextEditing>(editor).unwrap().len(), 3);
}
