use alloc::vec::Vec;

use crate::ui::clipboard::ClipboardError;

const HEADER: usize = 20;

pub(super) struct Paths<'a> {
    bytes: &'a [u8],
    unit: usize,
}

impl<'a> Paths<'a> {
    pub(super) fn read(block: &'a [u8]) -> Result<Self, ClipboardError> {
        let header = block.get(..HEADER).ok_or(ClipboardError::InvalidData)?;
        let offset = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        if offset < HEADER {
            return Err(ClipboardError::InvalidData);
        }
        let wide = header[16..20].iter().any(|byte| *byte != 0);
        let unit = if wide { 2 } else { 1 };
        let bytes = block.get(offset..).ok_or(ClipboardError::InvalidData)?;
        let mut rest = bytes;
        let mut count = 0;
        loop {
            let (path, next) = split_path(rest, unit)?;
            if path.is_empty() {
                if count == 0 {
                    let (second, _) = split_path(next, unit)?;
                    if !second.is_empty() {
                        return Err(ClipboardError::InvalidData);
                    }
                }
                let end = bytes.len() - rest.len();
                return Ok(Self {
                    bytes: &bytes[..end],
                    unit,
                });
            }
            if wide {
                valid_unicode(
                    path.chunks_exact(2)
                        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]])),
                )?;
            }
            count += 1;
            rest = next;
        }
    }

    pub(super) fn is_unicode(&self) -> bool {
        self.unit == 2
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        let mut rest = self.bytes;
        core::iter::from_fn(move || {
            if rest.is_empty() {
                return None;
            }
            let (path, next) = split_path(rest, self.unit).ok()?;
            rest = next;
            Some(path)
        })
    }
}

fn split_path(bytes: &[u8], unit: usize) -> Result<(&[u8], &[u8]), ClipboardError> {
    let end = bytes
        .chunks_exact(unit)
        .position(|word| word.iter().all(|byte| *byte == 0))
        .ok_or(ClipboardError::InvalidData)?
        * unit;
    Ok((&bytes[..end], &bytes[end + unit..]))
}

pub(super) fn encode(paths: &[impl AsRef<[u16]>]) -> Result<Vec<u8>, ClipboardError> {
    if paths.is_empty() {
        return Err(ClipboardError::InvalidData);
    }
    let length = paths.iter().try_fold(HEADER + 2, |length, path| {
        let path = path.as_ref();
        if path.is_empty() || path.contains(&0) {
            return Err(ClipboardError::InvalidData);
        }
        valid_unicode(path.iter().copied())?;
        path.len()
            .checked_add(1)
            .and_then(|units| units.checked_mul(2))
            .and_then(|bytes| length.checked_add(bytes))
            .ok_or(ClipboardError::Capacity)
    })?;
    let mut block = Vec::new();
    block
        .try_reserve_exact(length)
        .map_err(|_| ClipboardError::Capacity)?;
    block.extend_from_slice(&(HEADER as u32).to_le_bytes());
    block.extend_from_slice(&[0; 12]);
    block.extend_from_slice(&1u32.to_le_bytes());
    for path in paths {
        for unit in path.as_ref().iter().copied().chain(core::iter::once(0)) {
            block.extend_from_slice(&unit.to_le_bytes());
        }
    }
    block.extend_from_slice(&[0; 2]);
    Ok(block)
}

fn valid_unicode(units: impl Iterator<Item = u16>) -> Result<(), ClipboardError> {
    if char::decode_utf16(units).any(|character| character.is_err()) {
        Err(ClipboardError::InvalidData)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "files/tests.rs"]
mod tests;
