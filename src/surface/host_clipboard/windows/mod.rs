use alloc::{borrow::Cow, string::String, vec::Vec};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::HWND,
    System::DataExchange::{
        EnumClipboardFormats, GetClipboardFormatNameW, RegisterClipboardFormatW,
    },
};

use crate::ui::clipboard::{
    ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem, validate_mime,
};

mod bitmap;
mod files;
mod framing;
mod html;
mod images;
mod memory;
mod unicode;
mod uris;

const UNICODE: u32 = 13;
const DIB: u32 = 8;
const DIBV5: u32 = 17;
const HDROP: u32 = 15;

pub(super) fn write(
    window: &impl HasWindowHandle,
    data: &ClipboardData,
) -> Result<(), ClipboardError> {
    let window = hwnd(window)?;
    data.validate()?;
    let [item] = data.items() else {
        return Err(ClipboardError::Unsupported);
    };
    let mut prepared = Vec::new();
    for format in item.formats() {
        prepare(&mut prepared, format)?;
    }
    let session = memory::Session::open(window)?;
    let result = (|| {
        session.clear()?;
        for (id, bytes) in prepared {
            session.put(id, bytes)?;
        }
        Ok(())
    })();
    session.finish(result)
}

fn hwnd(window: &impl HasWindowHandle) -> Result<HWND, ClipboardError> {
    match window
        .window_handle()
        .map_err(|_| ClipboardError::Unavailable)?
        .as_raw()
    {
        RawWindowHandle::Win32(window) => Ok(window.hwnd.get() as HWND),
        _ => Err(ClipboardError::Unsupported),
    }
}

type Prepared = Vec<(u32, memory::Memory)>;

fn append(
    prepared: &mut Prepared,
    id: u32,
    bytes: &[u8],
    exact: bool,
) -> Result<(), ClipboardError> {
    if prepared.iter().any(|(old, _)| *old == id) {
        return Err(ClipboardError::Unsupported);
    }
    let memory = memory::Memory::copy(bytes, exact)?;
    prepared
        .try_reserve(1)
        .map_err(|_| ClipboardError::Capacity)?;
    prepared.push((id, memory));
    Ok(())
}

fn register(name: &str) -> Result<u32, ClipboardError> {
    let length = name.encode_utf16().count();
    if length == 0 || length > 255 || name.as_bytes().contains(&0) {
        return Err(ClipboardError::Unsupported);
    }
    let mut wide = [0u16; 256];
    for (index, unit) in name.encode_utf16().enumerate() {
        wide[index] = unit;
    }
    let id = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
    if id == 0 {
        Err(ClipboardError::Platform)
    } else {
        Ok(id)
    }
}

fn prepare(prepared: &mut Prepared, format: &ClipboardFormat) -> Result<(), ClipboardError> {
    let mime = format.mime().to_ascii_lowercase();
    let bytes = format.bytes();
    match mime.as_str() {
        "text/plain" => append(prepared, UNICODE, &unicode::encode(bytes)?, false),
        "text/html" => append(
            prepared,
            register("HTML Format")?,
            &html::encode(bytes)?,
            false,
        ),
        "image/png" => {
            if framing::png(bytes)?.len() != bytes.len() {
                return Err(ClipboardError::InvalidData);
            }
            let dib = images::png_to_dib(bytes)?;
            append(prepared, register("PNG")?, bytes, false)?;
            append(prepared, DIBV5, &dib, false)
        }
        "text/uri-list" => {
            let files = uris::to_files(bytes)?;
            append(prepared, register(&mime)?, bytes, true)?;
            if let Some(files) = files {
                append(prepared, HDROP, &files, false)?;
            }
            Ok(())
        }
        "text/rtf" | "application/rtf" => {
            framing::rtf(bytes)?;
            append(prepared, register(&mime)?, bytes, true)?;
            let native = register("Rich Text Format")?;
            if !prepared.iter().any(|(id, _)| *id == native) {
                let mut terminated = owned(bytes)?;
                terminated
                    .try_reserve_exact(1)
                    .map_err(|_| ClipboardError::Capacity)?;
                terminated.push(0);
                append(prepared, native, &terminated, false)?;
            }
            Ok(())
        }
        _ => append(prepared, register(&mime)?, bytes, true),
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Bytes,
    Unicode,
    Html,
    Png,
    Dib,
    Files,
    Rtf,
    Url,
}

struct Format {
    id: u32,
    mime: Cow<'static, str>,
    kind: Kind,
    priority: u8,
}

fn identify(id: u32) -> Result<Option<Format>, ClipboardError> {
    let (mime, kind, priority) = match id {
        UNICODE => (Cow::Borrowed("text/plain"), Kind::Unicode, 1),
        HDROP => (Cow::Borrowed("text/uri-list"), Kind::Files, 1),
        DIBV5 => (Cow::Borrowed("image/png"), Kind::Dib, 2),
        DIB => (Cow::Borrowed("image/png"), Kind::Dib, 3),
        0xc000..=0xffff => {
            let mut name = [0u16; 256];
            let length =
                unsafe { GetClipboardFormatNameW(id, name.as_mut_ptr(), name.len() as i32) };
            if length <= 0 {
                return Err(ClipboardError::Platform);
            }
            let mut name = String::from_utf16(&name[..length as usize])
                .map_err(|_| ClipboardError::InvalidData)?;
            name.make_ascii_lowercase();
            match name.as_str() {
                "html format" => (Cow::Borrowed("text/html"), Kind::Html, 1),
                "png" => (Cow::Borrowed("image/png"), Kind::Png, 1),
                "rich text format" => (Cow::Borrowed("text/rtf"), Kind::Rtf, 1),
                "uniformresourcelocatorw" => (Cow::Borrowed("text/uri-list"), Kind::Url, 2),
                _ if validate_mime(&name).is_ok() => (Cow::Owned(name), Kind::Bytes, 0),
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(Format {
        id,
        mime,
        kind,
        priority,
    }))
}

fn formats() -> Result<Vec<Format>, ClipboardError> {
    let mut formats: Vec<Format> = Vec::new();
    let mut id = 0;
    loop {
        memory::clear_error();
        id = unsafe { EnumClipboardFormats(id) };
        if id == 0 {
            memory::enumeration_end()?;
            break;
        }
        let Some(format) = identify(id)? else {
            continue;
        };
        if let Some(old) = formats.iter_mut().find(|old| old.mime == format.mime) {
            if format.priority < old.priority {
                *old = format;
            }
        } else {
            formats
                .try_reserve(1)
                .map_err(|_| ClipboardError::Capacity)?;
            formats.push(format);
        }
    }
    if formats.is_empty() {
        return Err(ClipboardError::Unsupported);
    }
    Ok(formats)
}

pub(super) fn read() -> Result<ClipboardData, ClipboardError> {
    let session = memory::Session::open(core::ptr::null_mut())?;
    let snapshot = (|| {
        let formats = formats()?;
        let mut snapshot = Vec::new();
        snapshot
            .try_reserve_exact(formats.len())
            .map_err(|_| ClipboardError::Capacity)?;
        for format in formats {
            let bytes = session.get(format.id)?;
            snapshot.push((format, bytes));
        }
        Ok(snapshot)
    })();
    let snapshot = session.finish(snapshot)?;
    let mut item = ClipboardItem::with_capacity(snapshot.len())?;
    let mut size = 0usize;
    for (format, mut bytes) in snapshot {
        let bytes = match format.kind {
            Kind::Bytes => bytes,
            Kind::Unicode => unicode::decode(&bytes)?,
            Kind::Html => owned(html::decode(&bytes)?)?,
            Kind::Png => {
                bytes.truncate(framing::png(&bytes)?.len());
                bytes
            }
            Kind::Dib => images::dib_to_png(&bytes)?,
            Kind::Files => uris::from_files(&bytes)?,
            Kind::Rtf => {
                bytes.truncate(framing::rtf(&bytes)?.len());
                bytes
            }
            Kind::Url => {
                let mut text = unicode::decode(&bytes)?;
                let value = core::str::from_utf8(&text).map_err(|_| ClipboardError::InvalidData)?;
                url::Url::parse(value).map_err(|_| ClipboardError::InvalidData)?;
                text.try_reserve_exact(2)
                    .map_err(|_| ClipboardError::Capacity)?;
                text.extend_from_slice(b"\r\n");
                text
            }
        };
        size = size
            .checked_add(bytes.len())
            .ok_or(ClipboardError::Capacity)?;
        item.push(ClipboardFormat::new(format.mime, bytes))?;
    }
    let mut data = ClipboardData::with_capacity(1, size)?;
    data.push(item)?;
    Ok(data)
}

fn owned(bytes: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(bytes.len())
        .map_err(|_| ClipboardError::Capacity)?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}

fn native_error(error: windows::core::Error) -> ClipboardError {
    use windows::Win32::Foundation::*;
    match error.code() {
        E_OUTOFMEMORY => ClipboardError::Capacity,
        WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT
        | WINCODEC_ERR_COMPONENTNOTFOUND
        | WINCODEC_ERR_UNSUPPORTEDOPERATION => ClipboardError::Unsupported,
        E_INVALIDARG | WINCODEC_ERR_BADIMAGE | WINCODEC_ERR_UNKNOWNIMAGEFORMAT => {
            ClipboardError::InvalidData
        }
        _ => ClipboardError::Platform,
    }
}
