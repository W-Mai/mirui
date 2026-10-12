use super::EventQueue;
use crate::surface::InputEvent;
use crate::surface::web_clipboard_format;
use crate::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardFormat, ClipboardItem,
    ClipboardRead, ClipboardWrite,
};
use alloc::vec::Vec;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

pub(super) fn read(request: ClipboardRead, queue: EventQueue) -> ClipboardDispatch {
    let promise = match invoke("read", None) {
        Ok(promise) => promise,
        Err(error) => return ClipboardDispatch::Complete(request.complete(Err(error))),
    };
    wasm_bindgen_futures::spawn_local(async move {
        let result = read_data(promise).await;
        queue
            .borrow_mut()
            .push_back(InputEvent::Clipboard(request.complete(result)));
    });
    ClipboardDispatch::Pending
}

pub(super) fn write(request: ClipboardWrite, queue: EventQueue) -> ClipboardDispatch {
    let promise = match write_data(request.data()) {
        Ok(promise) => promise,
        Err(error) => return ClipboardDispatch::Complete(request.complete(Err(error))),
    };
    wasm_bindgen_futures::spawn_local(async move {
        let result = JsFuture::from(promise).await.map(|_| ()).map_err(failure);
        queue
            .borrow_mut()
            .push_back(InputEvent::Clipboard(request.complete(result)));
    });
    ClipboardDispatch::Pending
}

fn invoke(method: &str, argument: Option<&JsValue>) -> Result<js_sys::Promise, ClipboardError> {
    let window = web_sys::window().ok_or(ClipboardError::Unsupported)?;
    let clipboard = window.navigator().clipboard();
    let value: &JsValue = clipboard.as_ref();
    if value.is_null() || value.is_undefined() {
        return Err(ClipboardError::Unsupported);
    }
    let function = js_sys::Reflect::get(value, &JsValue::from_str(method))
        .map_err(failure)?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| ClipboardError::Unsupported)?;
    let result = match argument {
        Some(argument) => function.call1(value, argument),
        None => function.call0(value),
    };
    result
        .map_err(failure)?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| ClipboardError::Platform)
}

fn write_data(data: &ClipboardData) -> Result<js_sys::Promise, ClipboardError> {
    data.validate()?;
    let [item] = data.items() else {
        return Err(ClipboardError::Unsupported);
    };
    if let [format] = item.formats()
        && format.mime().eq_ignore_ascii_case("text/plain")
    {
        return invoke("writeText", Some(&JsValue::from_str(data.plain_text()?)));
    }
    let representations = js_sys::Object::new();
    for format in item.formats() {
        let parts = js_sys::Array::new();
        parts.push(&js_sys::Uint8Array::from(format.bytes()));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(format.mime());
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(failure)?;
        let mime = web_clipboard_format::encode(format.mime());
        js_sys::Reflect::set(
            &representations,
            &JsValue::from_str(&mime),
            &js_sys::Promise::resolve(&blob),
        )
        .map_err(failure)?;
    }
    let item = web_sys::ClipboardItem::new_with_record_from_str_to_blob_promise(&representations)
        .map_err(failure)?;
    let items = js_sys::Array::new();
    items.push(&item);
    invoke("write", Some(&items))
}

fn failure(error: JsValue) -> ClipboardError {
    let name = js_sys::Reflect::get(&error, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string());
    match name.as_deref() {
        Some("NotAllowedError") => ClipboardError::PermissionDenied,
        Some("NotSupportedError") => ClipboardError::Unsupported,
        _ => ClipboardError::Platform,
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
            let mime = alloc::string::String::from(web_clipboard_format::decode(&mime));
            item.push(ClipboardFormat::new(mime, bytes))?;
        }
        data.push(item)?;
    }
    Ok(data)
}
