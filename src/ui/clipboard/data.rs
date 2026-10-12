use alloc::borrow::Cow;
use alloc::vec::Vec;

use super::ClipboardError;

/// One MIME representation of a clipboard item.
#[derive(Clone, Debug)]
pub struct ClipboardFormat {
    mime: Cow<'static, str>,
    bytes: Cow<'static, [u8]>,
}

impl ClipboardFormat {
    pub const fn borrowed(mime: &'static str, bytes: &'static [u8]) -> Self {
        Self {
            mime: Cow::Borrowed(mime),
            bytes: Cow::Borrowed(bytes),
        }
    }

    pub fn new(mime: impl Into<Cow<'static, str>>, bytes: impl Into<Cow<'static, [u8]>>) -> Self {
        Self {
            mime: mime.into(),
            bytes: bytes.into(),
        }
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Alternate representations of one selected object.
#[derive(Debug)]
pub struct ClipboardItem {
    formats: Cow<'static, [ClipboardFormat]>,
    limit: usize,
}

impl Clone for ClipboardItem {
    fn clone(&self) -> Self {
        let formats = match &self.formats {
            Cow::Borrowed(formats) => Cow::Borrowed(*formats),
            Cow::Owned(formats) => {
                let mut copy = Vec::with_capacity(self.limit);
                copy.extend_from_slice(formats);
                Cow::Owned(copy)
            }
        };
        Self {
            formats,
            limit: self.limit,
        }
    }
}

impl ClipboardItem {
    pub const fn borrowed(formats: &'static [ClipboardFormat]) -> Self {
        Self {
            formats: Cow::Borrowed(formats),
            limit: formats.len(),
        }
    }

    pub fn with_capacity(formats: usize) -> Result<Self, ClipboardError> {
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(formats)
            .map_err(|_| ClipboardError::Capacity)?;
        Ok(Self {
            formats: Cow::Owned(storage),
            limit: formats,
        })
    }

    /// Append without growing the prepared representation storage.
    pub fn push(&mut self, format: ClipboardFormat) -> Result<(), ClipboardError> {
        validate_mime(format.mime())?;
        if self
            .formats
            .iter()
            .any(|old| old.mime().eq_ignore_ascii_case(format.mime()))
        {
            return Err(ClipboardError::InvalidData);
        }
        let Cow::Owned(formats) = &mut self.formats else {
            return Err(ClipboardError::Capacity);
        };
        if formats.len() == self.limit {
            return Err(ClipboardError::Capacity);
        }
        formats.push(format);
        Ok(())
    }

    pub fn formats(&self) -> &[ClipboardFormat] {
        &self.formats
    }
}

/// Owned or static clipboard contents, independent of the original selection.
#[derive(Debug)]
pub struct ClipboardData {
    items: Cow<'static, [ClipboardItem]>,
    item_limit: usize,
    byte_limit: usize,
}

impl Clone for ClipboardData {
    fn clone(&self) -> Self {
        let items = match &self.items {
            Cow::Borrowed(items) => Cow::Borrowed(*items),
            Cow::Owned(items) => {
                let mut copy = Vec::with_capacity(self.item_limit);
                copy.extend_from_slice(items);
                Cow::Owned(copy)
            }
        };
        Self {
            items,
            item_limit: self.item_limit,
            byte_limit: self.byte_limit,
        }
    }
}

impl ClipboardData {
    pub const fn borrowed(items: &'static [ClipboardItem]) -> Self {
        Self {
            items: Cow::Borrowed(items),
            item_limit: items.len(),
            byte_limit: usize::MAX,
        }
    }

    pub fn with_capacity(items: usize, bytes: usize) -> Result<Self, ClipboardError> {
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(items)
            .map_err(|_| ClipboardError::Capacity)?;
        Ok(Self {
            items: Cow::Owned(storage),
            item_limit: items,
            byte_limit: bytes,
        })
    }

    /// Append an item without growing storage or exceeding the declared byte limit.
    pub fn push(&mut self, item: ClipboardItem) -> Result<(), ClipboardError> {
        validate_item(&item)?;
        let existing = byte_count(&self.items)?;
        let required = byte_count(core::slice::from_ref(&item))?
            .checked_add(existing)
            .ok_or(ClipboardError::Capacity)?;
        if required > self.byte_limit {
            return Err(ClipboardError::Capacity);
        }
        let Cow::Owned(items) = &mut self.items else {
            return Err(ClipboardError::Capacity);
        };
        if items.len() == self.item_limit {
            return Err(ClipboardError::Capacity);
        }
        items.push(item);
        Ok(())
    }

    /// Freeze a borrowed text selection into a separately owned UTF-8 snapshot.
    pub fn text(text: &str) -> Result<Self, ClipboardError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(text.len())
            .map_err(|_| ClipboardError::Capacity)?;
        bytes.extend_from_slice(text.as_bytes());
        Self::text_bytes(bytes)
    }

    #[cfg(any(feature = "sdl", feature = "sdl-gpu"))]
    pub(crate) fn owned_text(text: alloc::string::String) -> Result<Self, ClipboardError> {
        Self::text_bytes(text.into_bytes())
    }

    fn text_bytes(bytes: Vec<u8>) -> Result<Self, ClipboardError> {
        let len = bytes.len();
        let mut item = ClipboardItem::with_capacity(1)?;
        item.push(ClipboardFormat::new("text/plain", bytes))?;
        let mut data = Self::with_capacity(1, len)?;
        data.push(item)?;
        Ok(data)
    }

    pub fn items(&self) -> &[ClipboardItem] {
        &self.items
    }

    /// Require one complete plain-text representation, without discarding other data.
    pub fn plain_text(&self) -> Result<&str, ClipboardError> {
        let [item] = self.items() else {
            return Err(ClipboardError::Unsupported);
        };
        let [format] = item.formats() else {
            return Err(ClipboardError::Unsupported);
        };
        if !format.mime().eq_ignore_ascii_case("text/plain") {
            return Err(ClipboardError::Unsupported);
        }
        core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)
    }

    pub(crate) fn validate(&self) -> Result<(), ClipboardError> {
        if self.items.is_empty() {
            return Err(ClipboardError::InvalidData);
        }
        for item in self.items() {
            validate_item(item)?;
        }
        if byte_count(&self.items)? > self.byte_limit {
            return Err(ClipboardError::Capacity);
        }
        Ok(())
    }
}

fn validate_mime(mime: &str) -> Result<(), ClipboardError> {
    let Some((kind, subtype)) = mime.split_once('/') else {
        return Err(ClipboardError::InvalidData);
    };
    let token = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
    };
    if !token(kind) || !token(subtype) {
        return Err(ClipboardError::InvalidData);
    }
    Ok(())
}

fn validate_item(item: &ClipboardItem) -> Result<(), ClipboardError> {
    if item.formats.is_empty() {
        return Err(ClipboardError::InvalidData);
    }
    for (index, format) in item.formats.iter().enumerate() {
        validate_mime(format.mime())?;
        if item.formats[..index]
            .iter()
            .any(|old| old.mime().eq_ignore_ascii_case(format.mime()))
        {
            return Err(ClipboardError::InvalidData);
        }
    }
    Ok(())
}

fn byte_count(items: &[ClipboardItem]) -> Result<usize, ClipboardError> {
    items
        .iter()
        .flat_map(ClipboardItem::formats)
        .try_fold(0usize, |sum, format| {
            sum.checked_add(format.bytes().len())
                .ok_or(ClipboardError::Capacity)
        })
}
