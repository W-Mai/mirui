use alloc::boxed::Box;

use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::NSResponder;
use objc2_foundation::NSObjectProtocol;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

impl Action {
    pub(crate) fn input(self) -> crate::ui::clipboard::ClipboardInput {
        use crate::ui::clipboard::ClipboardInput;
        match self {
            Self::Copy => ClipboardInput::copy(),
            Self::Cut => ClipboardInput::cut(),
            Self::Paste => ClipboardInput::paste_requested(),
            Self::SelectAll => ClipboardInput::select_all(),
        }
    }
}

define_class!(
    // NSResponder has no additional subclassing requirements; all access is main-thread only.
    #[unsafe(super(NSResponder))]
    #[name = "MiruiClipboardResponder"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Box<dyn Fn(Action)>]
    struct MenuResponder;

    unsafe impl NSObjectProtocol for MenuResponder {}

    impl MenuResponder {
        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) { self.ivars()(Action::Copy); }
        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) { self.ivars()(Action::Cut); }
        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) { self.ivars()(Action::Paste); }
        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) { self.ivars()(Action::SelectAll); }
    }
);

pub(crate) struct Registration {
    host: Retained<NSResponder>,
    responder: Retained<MenuResponder>,
    // Native nextResponder links are unretained. Keep the original successor alive until detach.
    _next: Option<Retained<NSResponder>>,
}

impl Registration {
    pub(crate) fn new(
        host: &NSResponder,
        dispatch: impl Fn(Action) + 'static,
        mtm: MainThreadMarker,
    ) -> Self {
        let this = MenuResponder::alloc(mtm).set_ivars(Box::new(dispatch));
        let responder: Retained<MenuResponder> = unsafe { msg_send![super(this), init] };
        let next = unsafe { host.nextResponder() };
        // Registration owns both link destinations and detaches before releasing them.
        unsafe {
            responder.setNextResponder(next.as_deref());
            host.setNextResponder(Some(&responder));
        }
        Self {
            host: host.into(),
            responder,
            _next: next,
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut current = self.host.clone();
        let mut checkpoint = current.clone();
        let mut distance = 0usize;
        let mut interval = 1usize;
        // Preserve other responders inserted around this registration. Detect a malformed cycle
        // without allocating during teardown or traversing the same cycle indefinitely.
        while let Some(next) = unsafe { current.nextResponder() } {
            if core::ptr::eq::<NSResponder>(&*next, &**self.responder) {
                let successor = unsafe { self.responder.nextResponder() };
                unsafe {
                    current.setNextResponder(successor.as_deref());
                    self.responder.setNextResponder(None);
                }
                return;
            }
            current = next;
            distance += 1;
            if core::ptr::eq::<NSResponder>(&*current, &*checkpoint) {
                break;
            }
            if distance == interval {
                checkpoint = current.clone();
                distance = 0;
                interval = interval.saturating_mul(2);
            }
        }
    }
}
