use crate::ui::clipboard::{ClipboardDispatch, ClipboardRead, ClipboardWrite};

mod macos;

pub(crate) fn write(request: ClipboardWrite) -> ClipboardDispatch {
    let result = macos::write(request.data());
    ClipboardDispatch::Complete(request.complete(result))
}

pub(crate) fn read(request: ClipboardRead) -> ClipboardDispatch {
    ClipboardDispatch::Complete(request.complete(macos::read()))
}
