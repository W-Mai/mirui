use super::apple::{mime_type, native_type, single_url};
use alloc::vec::Vec;
use objc2::{ClassType, MainThreadMarker, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardItem, NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};

use crate::ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem};

pub(super) fn write(data: &ClipboardData) -> Result<(), ClipboardError> {
    MainThreadMarker::new().ok_or(ClipboardError::Unavailable)?;
    objc2::rc::autoreleasepool(|_| {
        let board = general()?;
        write_to(&board, data)
    })
}

pub(super) fn read() -> Result<ClipboardData, ClipboardError> {
    MainThreadMarker::new().ok_or(ClipboardError::Unavailable)?;
    objc2::rc::autoreleasepool(|_| {
        let board = general()?;
        read_from(&board)
    })
}

fn general() -> Result<Retained<NSPasteboard>, ClipboardError> {
    // The pasteboard service can return nil when unavailable, despite the SDK's nonnull annotation.
    let board: Option<Retained<NSPasteboard>> =
        unsafe { objc2::msg_send![NSPasteboard::class(), generalPasteboard] };
    board.ok_or(ClipboardError::Unavailable)
}

fn write_to(board: &NSPasteboard, data: &ClipboardData) -> Result<(), ClipboardError> {
    data.validate()?;
    let mut items = Vec::new();
    items
        .try_reserve_exact(data.items().len())
        .map_err(|_| ClipboardError::Capacity)?;
    for item in data.items() {
        let native = NSPasteboardItem::new();
        for format in item.formats() {
            let mime = format.mime().to_ascii_lowercase();
            let kind = native_type(&mime)?;
            if native.types().iter().any(|existing| existing == kind) {
                return Err(ClipboardError::Unsupported);
            }
            if matches!(mime.as_str(), "text/plain" | "text/html" | "text/uri-list") {
                core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)?;
            }
            if !native.setData_forType(&NSData::with_bytes(format.bytes()), &kind) {
                return Err(ClipboardError::Platform);
            }
        }
        if !item
            .formats()
            .iter()
            .any(|format| format.mime().eq_ignore_ascii_case("text/rtf"))
            && let Some(format) = item
                .formats()
                .iter()
                .find(|format| format.mime().eq_ignore_ascii_case("application/rtf"))
            && !native.setData_forType(
                &NSData::with_bytes(format.bytes()),
                &NSString::from_str("public.rtf"),
            )
        {
            return Err(ClipboardError::Platform);
        }
        if let Some(format) = item
            .formats()
            .iter()
            .find(|format| format.mime().eq_ignore_ascii_case("text/uri-list"))
        {
            add_native_urls(&native, format.bytes())?;
        }
        if let Some(format) = item
            .formats()
            .iter()
            .find(|format| format.mime().eq_ignore_ascii_case("image/png"))
        {
            if !format.bytes().starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(ClipboardError::InvalidData);
            }
            bitmap(&NSData::with_bytes(format.bytes()))?;
        }
        items.push(ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
            native,
        ));
    }
    let items = NSArray::from_retained_slice(&items);
    board.clearContents();
    if board.writeObjects(&items) {
        Ok(())
    } else {
        Err(ClipboardError::Platform)
    }
}

fn add_native_urls(item: &NSPasteboardItem, bytes: &[u8]) -> Result<(), ClipboardError> {
    let url = single_url(bytes)?;
    let kind = if url.isFileURL() {
        "public.file-url"
    } else {
        "public.url"
    };
    let absolute = url.absoluteString().ok_or(ClipboardError::InvalidData)?;
    if !item.setString_forType(&absolute, &NSString::from_str(kind)) {
        return Err(ClipboardError::Platform);
    }
    Ok(())
}

fn bitmap(bytes: &NSData) -> Result<Retained<NSBitmapImageRep>, ClipboardError> {
    let bitmap = NSBitmapImageRep::imageRepWithData(bytes).ok_or(ClipboardError::InvalidData)?;
    let width = usize::try_from(bitmap.pixelsWide()).map_err(|_| ClipboardError::InvalidData)?;
    let height = usize::try_from(bitmap.pixelsHigh()).map_err(|_| ClipboardError::InvalidData)?;
    let stride = usize::try_from(bitmap.bytesPerRow()).map_err(|_| ClipboardError::InvalidData)?;
    if width == 0 || height == 0 || stride == 0 || stride.checked_mul(height).is_none() {
        return Err(ClipboardError::InvalidData);
    }
    Ok(bitmap)
}

fn read_from(board: &NSPasteboard) -> Result<ClipboardData, ClipboardError> {
    let generation = board.changeCount();
    let result = read_items(board);
    if board.changeCount() != generation {
        return Err(ClipboardError::Unavailable);
    }
    result
}

fn read_items(board: &NSPasteboard) -> Result<ClipboardData, ClipboardError> {
    let items = board.pasteboardItems().ok_or(ClipboardError::Unavailable)?;
    if items.is_empty() {
        return Err(ClipboardError::Unavailable);
    }
    let mut data = ClipboardData::with_capacity(items.len(), usize::MAX)?;
    let uri_type = native_type("text/uri-list")?;
    for native in &items {
        let types = native.types();
        let has_uri_list = types.iter().any(|kind| kind == uri_type);
        let has_utf8 = types
            .iter()
            .any(|kind| kind.to_string() == "public.utf8-plain-text");
        let capacity = types.len().checked_add(1).ok_or(ClipboardError::Capacity)?;
        let mut item = ClipboardItem::with_capacity(capacity)?;
        let mut tiff = None;
        for kind in &types {
            let name = kind.to_string();
            if (has_uri_list && matches!(name.as_str(), "public.file-url" | "public.url"))
                || (has_utf8
                    && matches!(
                        name.as_str(),
                        "public.utf16-external-plain-text" | "public.plain-text"
                    ))
            {
                continue;
            }
            if name == "public.tiff" {
                tiff = Some(kind.clone());
            }
            let Some(mime) = mime_type(&name) else {
                continue;
            };
            if item.formats().iter().any(|format| format.mime() == mime) {
                continue;
            }
            let bytes = if mime == "text/plain" && name.starts_with("public.") {
                native
                    .stringForType(&kind)
                    .ok_or(ClipboardError::InvalidData)?
                    .to_string()
                    .into_bytes()
            } else if mime == "text/uri-list" && name.starts_with("public.") {
                let url = native
                    .stringForType(&kind)
                    .ok_or(ClipboardError::InvalidData)?
                    .to_string();
                alloc::format!("{url}\r\n").into_bytes()
            } else {
                native
                    .dataForType(&kind)
                    .ok_or(ClipboardError::Platform)?
                    .to_vec()
            };
            item.push(ClipboardFormat::new(mime, bytes))?;
        }
        if let Some(kind) = tiff
            && !item
                .formats()
                .iter()
                .any(|format| format.mime() == "image/png")
        {
            let bytes = native.dataForType(&kind).ok_or(ClipboardError::Platform)?;
            let bitmap = bitmap(&bytes)?;
            // An empty properties dictionary contains no incorrectly typed values.
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }
            .ok_or(ClipboardError::InvalidData)?;
            item.push(ClipboardFormat::new("image/png", png.to_vec()))?;
        }
        if item.formats().is_empty() {
            return Err(ClipboardError::Unsupported);
        }
        data.push(item)?;
    }
    data.validate()?;
    Ok(data)
}
