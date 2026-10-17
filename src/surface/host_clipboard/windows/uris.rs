use alloc::{string::String, vec::Vec};
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Globalization::{CP_ACP, MB_ERR_INVALID_CHARS, MultiByteToWideChar};

use crate::ui::clipboard::ClipboardError;

pub(super) fn to_files(bytes: &[u8]) -> Result<Option<Vec<u8>>, ClipboardError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ClipboardError::InvalidData)?;
    if text.as_bytes().contains(&0) {
        return Err(ClipboardError::InvalidData);
    }
    let mut paths = Vec::new();
    let mut only_files = true;
    let mut count = 0;
    for line in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let uri = url::Url::parse(line).map_err(|_| ClipboardError::InvalidData)?;
        count += 1;
        if uri.scheme() != "file" {
            only_files = false;
            continue;
        }
        if uri.query().is_some() || uri.fragment().is_some() {
            return Err(ClipboardError::InvalidData);
        }
        let path = uri
            .to_file_path()
            .map_err(|_| ClipboardError::Unsupported)?;
        let units = path.as_os_str().encode_wide();
        let mut wide = Vec::new();
        wide.try_reserve_exact(units.clone().count())
            .map_err(|_| ClipboardError::Capacity)?;
        wide.extend(units);
        paths.try_reserve(1).map_err(|_| ClipboardError::Capacity)?;
        paths.push(wide);
    }
    if count == 0 {
        return Err(ClipboardError::InvalidData);
    }
    if only_files {
        super::files::encode(&paths).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn from_files(block: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    let paths = super::files::Paths::read(block)?;
    let mut text = String::new();
    for path in paths.iter() {
        let path = if paths.is_unicode() {
            let units = path
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]));
            String::from_utf8(super::unicode::decode_units(units)?)
                .map_err(|_| ClipboardError::InvalidData)?
        } else {
            ansi(path)?
        };
        let uri = url::Url::from_file_path(&path).map_err(|_| ClipboardError::InvalidData)?;
        text.try_reserve(uri.as_str().len() + 2)
            .map_err(|_| ClipboardError::Capacity)?;
        text.push_str(uri.as_str());
        text.push_str("\r\n");
    }
    if text.is_empty() {
        return Err(ClipboardError::InvalidData);
    }
    Ok(text.into_bytes())
}

fn ansi(bytes: &[u8]) -> Result<String, ClipboardError> {
    let count = i32::try_from(bytes.len()).map_err(|_| ClipboardError::Capacity)?;
    let required = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            count,
            core::ptr::null_mut(),
            0,
        )
    };
    if required <= 0 {
        return Err(ClipboardError::InvalidData);
    }
    let mut wide = Vec::new();
    wide.try_reserve_exact(required as usize)
        .map_err(|_| ClipboardError::Capacity)?;
    wide.resize(required as usize, 0);
    let written = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            count,
            wide.as_mut_ptr(),
            required,
        )
    };
    if written != required {
        return Err(ClipboardError::InvalidData);
    }
    String::from_utf16(&wide).map_err(|_| ClipboardError::InvalidData)
}
