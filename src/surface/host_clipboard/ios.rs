use alloc::vec::Vec;
use core::ptr::NonNull;

use objc2::{
    AnyThread, MainThreadMarker,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL, NSUTF16StringEncoding};
use objc2_ui_kit::{
    UIGraphicsImageRenderer, UIGraphicsImageRendererContext, UIGraphicsImageRendererFormat,
    UIImage, UIImageOrientation, UIPasteboard,
};

use super::apple::{mime_type, native_type, single_url};
use crate::ui::clipboard::{ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem};

pub(super) fn write(data: &ClipboardData) -> Result<(), ClipboardError> {
    let mtm = MainThreadMarker::new().ok_or(ClipboardError::Unavailable)?;
    objc2::rc::autoreleasepool(|_| write_to(&UIPasteboard::generalPasteboard(), data, mtm))
}

pub(super) fn read() -> Result<ClipboardData, ClipboardError> {
    let mtm = MainThreadMarker::new().ok_or(ClipboardError::Unavailable)?;
    objc2::rc::autoreleasepool(|_| read_from(&UIPasteboard::generalPasteboard(), mtm))
}

fn write_to(
    board: &UIPasteboard,
    data: &ClipboardData,
    mtm: MainThreadMarker,
) -> Result<(), ClipboardError> {
    let items = prepare(data, mtm)?;
    // Values are immutable NSData/NSURL representations, prepared on the main thread.
    unsafe { board.setItems(&items) };
    Ok(())
}

fn read_from(board: &UIPasteboard, mtm: MainThreadMarker) -> Result<ClipboardData, ClipboardError> {
    // Capture all items once on the main thread; conversion never rereads the board.
    let generation = unsafe { board.changeCount() };
    let items = unsafe { board.items() };
    let result = decode(&items, mtm);
    if unsafe { board.changeCount() } != generation {
        return Err(ClipboardError::Unavailable);
    }
    result
}

fn prepare(
    data: &ClipboardData,
    mtm: MainThreadMarker,
) -> Result<Retained<NSArray<NSDictionary<NSString, AnyObject>>>, ClipboardError> {
    data.validate()?;
    let mut items = Vec::new();
    items
        .try_reserve_exact(data.items().len())
        .map_err(|_| ClipboardError::Capacity)?;
    for item in data.items() {
        let capacity = item
            .formats()
            .len()
            .checked_add(2)
            .ok_or(ClipboardError::Capacity)?;
        let mut kinds = Vec::new();
        let mut values = Vec::new();
        kinds
            .try_reserve_exact(capacity)
            .map_err(|_| ClipboardError::Capacity)?;
        values
            .try_reserve_exact(capacity)
            .map_err(|_| ClipboardError::Capacity)?;
        for format in item.formats() {
            let mime = format.mime().to_ascii_lowercase();
            let kind = native_type(&mime)?;
            if kinds.contains(&kind) {
                return Err(ClipboardError::Unsupported);
            }
            if matches!(mime.as_str(), "text/plain" | "text/html" | "text/uri-list") {
                core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)?;
            }
            let bytes = NSData::with_bytes(format.bytes());
            if mime == "image/png" {
                if !format.bytes().starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Err(ClipboardError::InvalidData);
                }
                let image = UIImage::imageWithData(&bytes).ok_or(ClipboardError::InvalidData)?;
                validate_image(&image, mtm)?;
            }
            kinds.push(kind);
            values.push(bytes.into_super().into_super());
        }
        if !item
            .formats()
            .iter()
            .any(|f| f.mime().eq_ignore_ascii_case("text/rtf"))
            && let Some(format) = item
                .formats()
                .iter()
                .find(|f| f.mime().eq_ignore_ascii_case("application/rtf"))
        {
            kinds.push(NSString::from_str("public.rtf"));
            values.push(NSData::with_bytes(format.bytes()).into_super().into_super());
        }
        if let Some(format) = item
            .formats()
            .iter()
            .find(|f| f.mime().eq_ignore_ascii_case("text/uri-list"))
        {
            let url = single_url(format.bytes())?;
            kinds.push(NSString::from_str(if url.isFileURL() {
                "public.file-url"
            } else {
                "public.url"
            }));
            values.push(url.into_super().into_super());
        }
        let keys: Vec<_> = kinds.iter().map(|kind| &**kind).collect();
        items.push(NSDictionary::from_retained_objects(&keys, &values));
    }
    Ok(NSArray::from_retained_slice(&items))
}

fn validate_image(image: &UIImage, _mtm: MainThreadMarker) -> Result<(), ClipboardError> {
    // UIImage's non-atomic metadata is read only on the main thread.
    let size = unsafe { image.size() };
    let scale = unsafe { image.scale() };
    let width = size.width * scale;
    let height = size.height * scale;
    if size.width <= 0.0
        || size.height <= 0.0
        || scale <= 0.0
        || !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || width >= usize::MAX as f64
        || height >= usize::MAX as f64
        || (width.ceil() as usize)
            .checked_mul(height.ceil() as usize)
            .and_then(|n| n.checked_mul(4))
            .is_none()
    {
        return Err(ClipboardError::InvalidData);
    }
    Ok(())
}

fn png(image: &UIImage, mtm: MainThreadMarker) -> Result<Vec<u8>, ClipboardError> {
    validate_image(image, mtm)?;
    // PNG encoding alone does not apply UIImage's display orientation.
    let bytes = if unsafe { image.imageOrientation() } == UIImageOrientation::Up {
        image.png_representation()
    } else {
        let size = unsafe { image.size() };
        let format = UIGraphicsImageRendererFormat::new();
        format.setScale(unsafe { image.scale() });
        format.setOpaque(false);
        let renderer = UIGraphicsImageRenderer::initWithSize_format(
            UIGraphicsImageRenderer::alloc(),
            size,
            &format,
        );
        let draw = block2::StackBlock::new(|_: NonNull<UIGraphicsImageRendererContext>| {
            image.drawInRect(CGRect::new(CGPoint::ZERO, size));
        });
        // UIKit invokes this drawing block synchronously without retaining it.
        let action = &*draw as *const _
            as *mut block2::Block<dyn Fn(NonNull<UIGraphicsImageRendererContext>)>;
        unsafe { renderer.imageWithActions(action) }.png_representation()
    }
    .ok_or(ClipboardError::InvalidData)?;
    Ok(bytes.to_vec())
}

fn decode(
    items: &NSArray<NSDictionary<NSString, AnyObject>>,
    mtm: MainThreadMarker,
) -> Result<ClipboardData, ClipboardError> {
    if items.is_empty() {
        return Err(ClipboardError::Unavailable);
    }
    let mut data = ClipboardData::with_capacity(items.len(), usize::MAX)?;
    let uri_type = native_type("text/uri-list")?;
    for native in items {
        let has_uri = native.objectForKey(&uri_type).is_some();
        let has_utf8 = native
            .objectForKey(&NSString::from_str("public.utf8-plain-text"))
            .is_some();
        let has_png_bytes = native
            .objectForKey(&NSString::from_str("public.png"))
            .is_some_and(|value| native_data(&value).is_some());
        let mut item = ClipboardItem::with_capacity(
            native
                .len()
                .checked_add(1)
                .ok_or(ClipboardError::Capacity)?,
        )?;
        for kind in native.keys() {
            let name = kind.to_string();
            if (has_uri && matches!(name.as_str(), "public.file-url" | "public.url"))
                || (has_utf8
                    && matches!(
                        name.as_str(),
                        "public.utf16-plain-text"
                            | "public.utf16-external-plain-text"
                            | "public.plain-text"
                            | "public.text"
                    ))
            {
                continue;
            }
            let value = native
                .objectForKey(&kind)
                .ok_or(ClipboardError::InvalidData)?;
            let image = value.downcast_ref::<UIImage>();
            if image.is_some() && has_png_bytes {
                continue;
            }
            let mime = if image.is_some() {
                "image/png".into()
            } else {
                let Some(mime) = mime_type(&name) else {
                    continue;
                };
                mime
            };
            if item.formats().iter().any(|f| f.mime() == mime) {
                continue;
            }
            let bytes = if let Some(image) = image {
                png(image, mtm)?
            } else if let Some(bytes) = native_data(&value) {
                if matches!(
                    name.as_str(),
                    "public.utf16-plain-text" | "public.utf16-external-plain-text"
                ) {
                    NSString::initWithData_encoding(NSString::alloc(), bytes, NSUTF16StringEncoding)
                        .ok_or(ClipboardError::InvalidData)?
                        .to_string()
                        .into_bytes()
                } else {
                    bytes.to_vec()
                }
            } else if let Some(text) = value.downcast_ref::<NSString>() {
                if !matches!(
                    mime.as_str(),
                    "text/plain" | "text/html" | "text/uri-list" | "text/rtf" | "application/rtf"
                ) {
                    return Err(ClipboardError::Unsupported);
                }
                text.to_string().into_bytes()
            } else if let Some(url) = value.downcast_ref::<NSURL>() {
                if mime != "text/uri-list" {
                    return Err(ClipboardError::Unsupported);
                }
                url.absoluteString()
                    .ok_or(ClipboardError::InvalidData)?
                    .to_string()
                    .into_bytes()
            } else {
                return Err(ClipboardError::Unsupported);
            };
            let bytes = if mime == "text/uri-list"
                && matches!(name.as_str(), "public.file-url" | "public.url")
            {
                single_url(&bytes)?;
                let mut bytes = bytes;
                bytes.extend_from_slice(b"\r\n");
                bytes
            } else {
                bytes
            };
            item.push(ClipboardFormat::new(mime, bytes))?;
        }
        data.push(item)?;
    }
    data.validate()?;
    Ok(data)
}

fn native_data(value: &AnyObject) -> Option<&NSData> {
    if let Some(data) = value.downcast_ref::<NSData>() {
        return Some(data);
    }
    #[cfg(target_pointer_width = "64")]
    if let Some(class) = AnyClass::get(c"OS_dispatch_data") {
        let is_data: bool = unsafe { objc2::msg_send![value, isKindOfClass: class] };
        if is_data {
            // Apple permits dispatch_data_t -> NSData casts on 64-bit targets, not the reverse.
            return Some(unsafe { &*(value as *const AnyObject).cast::<NSData>() });
        }
    }
    None
}
