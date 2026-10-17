use super::{DomFocus, EventQueue, Listener, register_listener};
use crate::surface::InputEvent;
use crate::surface::web_clipboard_format;
use crate::ui::clipboard::{
    ClipboardData, ClipboardDispatch, ClipboardError, ClipboardFormat, ClipboardInput,
    ClipboardItem, ClipboardRead, ClipboardWrite,
};
use alloc::{rc::Rc, vec::Vec};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

pub(super) fn attach(
    document: &web_sys::Document,
    queue: &EventQueue,
    focus: &Rc<DomFocus>,
    listeners: &mut Vec<Listener>,
) {
    for name in ["copy", "cut", "paste"] {
        let queue = queue.clone();
        let focus = focus.clone();
        let owner = document.clone();
        listeners.push(register_listener(
            document.clone().into(),
            name,
            Closure::<dyn FnMut(JsValue)>::new(move |raw: JsValue| {
                let event: web_sys::ClipboardEvent = raw.unchecked_into();
                if !event.is_trusted() || event.default_prevented() || !focus.focused(&owner) {
                    return;
                }
                let Some(target) = event
                    .target()
                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                else {
                    return;
                };
                if !focus.contains(&target) {
                    return;
                }
                event.prevent_default();
                let input = match name {
                    "copy" => ClipboardInput::copy(),
                    "cut" => ClipboardInput::cut(),
                    _ => read_native(&event, queue.clone())
                        .unwrap_or_else(|error| ClipboardInput::paste(Err(error))),
                };
                queue
                    .borrow_mut()
                    .push_back(InputEvent::ClipboardInput(input));
            }),
        ));
    }
}

fn read_native(
    event: &web_sys::ClipboardEvent,
    queue: EventQueue,
) -> Result<ClipboardInput, ClipboardError> {
    let source = event.clipboard_data().ok_or(ClipboardError::InvalidData)?;
    let types = source.types();
    let files = source.files();
    let count = files.as_ref().map_or(0, web_sys::FileList::length) as usize;
    let mut blobs = Vec::new();
    blobs
        .try_reserve_exact(count)
        .map_err(|_| ClipboardError::Capacity)?;
    if let Some(files) = files {
        for index in 0..files.length() {
            let file = files.get(index).ok_or(ClipboardError::InvalidData)?;
            blobs.push(file.unchecked_into::<web_sys::Blob>());
        }
    }
    let mut item = ClipboardItem::with_capacity(types.length() as usize)?;
    for mime in types.iter() {
        let mime = mime.as_string().ok_or(ClipboardError::InvalidData)?;
        if mime.eq_ignore_ascii_case("Files") {
            if blobs.is_empty() {
                return Err(ClipboardError::Unsupported);
            }
            continue;
        }
        let text = source.get_data(&mime).map_err(failure)?;
        let mime = alloc::string::String::from(web_clipboard_format::decode(&mime));
        item.push(ClipboardFormat::new(mime, text.into_bytes()))?;
    }
    let capacity = count.checked_add(1).ok_or(ClipboardError::Capacity)?;
    let mut data = ClipboardData::with_capacity(capacity, usize::MAX)?;
    if !item.formats().is_empty() {
        data.push(item)?;
    }
    if blobs.is_empty() {
        data.validate()?;
        return Ok(ClipboardInput::paste(Ok(data)));
    }
    let (sender, receiver) = futures_channel::oneshot::channel::<ClipboardRead>();
    let queue = Rc::downgrade(&queue);
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(request) = receiver.await else { return };
        let result = read_files(data, blobs).await;
        if let Some(queue) = queue.upgrade() {
            queue
                .borrow_mut()
                .push_back(InputEvent::Clipboard(request.complete(result)));
        }
    });
    Ok(ClipboardInput::paste_deferred(
        move |request| match sender.send(request) {
            Ok(()) => ClipboardDispatch::Pending,
            Err(request) => {
                ClipboardDispatch::Complete(request.complete(Err(ClipboardError::Unavailable)))
            }
        },
    ))
}

async fn read_files(
    mut data: ClipboardData,
    blobs: Vec<web_sys::Blob>,
) -> Result<ClipboardData, ClipboardError> {
    for blob in blobs {
        let mime = blob.type_();
        let mime = if mime.is_empty() {
            "application/octet-stream"
        } else {
            &mime
        };
        let mut item = ClipboardItem::with_capacity(1)?;
        item.push(ClipboardFormat::new(
            alloc::string::String::from(web_clipboard_format::decode(mime)),
            read_blob(&blob).await?,
        ))?;
        data.push(item)?;
    }
    data.validate()?;
    Ok(data)
}

async fn read_blob(blob: &web_sys::Blob) -> Result<Vec<u8>, ClipboardError> {
    let buffer = JsFuture::from(blob.array_buffer()).await.map_err(failure)?;
    let source = js_sys::Uint8Array::new(&buffer);
    let mut bytes = Vec::new();
    let len = source.length() as usize;
    bytes
        .try_reserve_exact(len)
        .map_err(|_| ClipboardError::Capacity)?;
    bytes.resize(len, 0);
    source.copy_to(&mut bytes);
    Ok(bytes)
}

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
            let bytes = read_blob(&blob).await?;
            let mime = alloc::string::String::from(web_clipboard_format::decode(&mime));
            item.push(ClipboardFormat::new(mime, bytes))?;
        }
        data.push(item)?;
    }
    Ok(data)
}
