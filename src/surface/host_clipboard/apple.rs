use alloc::string::String;
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;

use crate::ui::clipboard::ClipboardError;

pub(super) fn native_type(mime: &str) -> Result<Retained<NSString>, ClipboardError> {
    let known = match mime {
        "text/plain" => Some("public.utf8-plain-text"),
        "text/html" => Some("public.html"),
        "text/rtf" => Some("public.rtf"),
        "image/png" => Some("public.png"),
        _ => None,
    };
    if let Some(kind) = known {
        return Ok(NSString::from_str(kind));
    }
    let kind =
        UTType::typeWithMIMEType(&NSString::from_str(mime)).ok_or(ClipboardError::Unsupported)?;
    let restored = kind
        .preferredMIMEType()
        .ok_or(ClipboardError::Unsupported)?;
    if !restored.to_string().eq_ignore_ascii_case(mime) {
        return Err(ClipboardError::Unsupported);
    }
    Ok(kind.identifier())
}

pub(super) fn mime_type(native: &str) -> Option<String> {
    match native {
        "public.utf8-plain-text" | "public.utf16-external-plain-text" | "public.plain-text" => {
            Some("text/plain".into())
        }
        "public.html" => Some("text/html".into()),
        "public.rtf" => Some("text/rtf".into()),
        "public.png" => Some("image/png".into()),
        "public.file-url" | "public.url" => Some("text/uri-list".into()),
        _ => UTType::typeWithIdentifier(&NSString::from_str(native))?
            .preferredMIMEType()
            .map(|mime| mime.to_string()),
    }
}

pub(super) fn single_url(bytes: &[u8]) -> Result<Retained<NSURL>, ClipboardError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ClipboardError::InvalidData)?;
    let mut selected = None;
    for line in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if line.as_bytes().contains(&0) {
            return Err(ClipboardError::InvalidData);
        }
        let url =
            NSURL::URLWithString(&NSString::from_str(line)).ok_or(ClipboardError::InvalidData)?;
        if url.scheme().is_none() {
            return Err(ClipboardError::InvalidData);
        }
        if selected.replace(url).is_some() {
            return Err(ClipboardError::Unsupported);
        }
    }
    selected.ok_or(ClipboardError::InvalidData)
}
