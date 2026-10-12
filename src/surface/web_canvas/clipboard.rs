use super::EventQueue;
use crate::surface::InputEvent;
use crate::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardFormat, ClipboardItem, ClipboardRead,
};
use alloc::vec::Vec;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

pub(super) fn read(request: ClipboardRead, queue: EventQueue) -> ClipboardDispatch {
    let Some(window) = web_sys::window() else {
        return ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Unsupported)));
    };
    let clipboard = window.navigator().clipboard();
    let value: &JsValue = clipboard.as_ref();
    if value.is_null() || value.is_undefined() {
        return ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Unsupported)));
    }
    let promise = clipboard.read();
    wasm_bindgen_futures::spawn_local(async move {
        let result = read_data(promise).await;
        queue
            .borrow_mut()
            .push_back(InputEvent::Clipboard(request.complete(result)));
    });
    ClipboardDispatch::Pending
}

fn failure(error: JsValue) -> ClipboardError {
    let name = js_sys::Reflect::get(&error, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string());
    if name.as_deref() == Some("NotAllowedError") {
        ClipboardError::PermissionDenied
    } else {
        ClipboardError::Platform
    }
}

async fn read_data(promise: js_sys::Promise) -> Result<ClipboardData, ClipboardError> {
    let items = JsFuture::from(promise)
        .await
        .map_err(failure)?
        .dyn_into::<js_sys::Array>()
        .map_err(|_| ClipboardError::InvalidData)?;
    let mut data = ClipboardData::with_capacity(items.length() as usize, usize::MAX)?;
    for item in items.iter() {
        let source = item
            .dyn_into::<web_sys::ClipboardItem>()
            .map_err(|_| ClipboardError::InvalidData)?;
        let types = source.types();
        let mut item = ClipboardItem::with_capacity(types.length() as usize)?;
        for mime in types.iter() {
            let mime = mime.as_string().ok_or(ClipboardError::InvalidData)?;
            let blob = JsFuture::from(source.get_type(&mime))
                .await
                .map_err(failure)?
                .dyn_into::<web_sys::Blob>()
                .map_err(|_| ClipboardError::InvalidData)?;
            let buffer = JsFuture::from(blob.array_buffer()).await.map_err(failure)?;
            let source = js_sys::Uint8Array::new(&buffer);
            let mut bytes = Vec::new();
            let len = source.length() as usize;
            bytes
                .try_reserve_exact(len)
                .map_err(|_| ClipboardError::Capacity)?;
            bytes.resize(len, 0);
            source.copy_to(&mut bytes);
            item.push(ClipboardFormat::new(mime, bytes))?;
        }
        data.push(item)?;
    }
    Ok(data)
}
