use alloc::vec::Vec;

use crate::ui::clipboard::ClipboardError;

const FILE_HEADER: usize = 14;
const INFO_HEADER: usize = 40;
const V4_HEADER: usize = 108;
pub(super) const V5_HEADER: usize = 124;
const PROFILE_LINKED: u32 = 0x4c49_4e4b;
const PROFILE_EMBEDDED: u32 = 0x4d42_4544;

pub(super) fn bgra_storage(width: u32, height: u32) -> Result<Vec<u8>, ClipboardError> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(ClipboardError::InvalidData);
    }
    let image = width
        .checked_mul(4)
        .and_then(|stride| stride.checked_mul(height))
        .ok_or(ClipboardError::Capacity)?;
    let length = (image as usize)
        .checked_add(V5_HEADER)
        .ok_or(ClipboardError::Capacity)?;
    let mut dib = Vec::new();
    dib.try_reserve_exact(length)
        .map_err(|_| ClipboardError::Capacity)?;
    dib.resize(length, 0);
    dib[..4].copy_from_slice(&(V5_HEADER as u32).to_le_bytes());
    dib[4..8].copy_from_slice(&width.to_le_bytes());
    dib[8..12].copy_from_slice(&(-(height as i32)).to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[16..20].copy_from_slice(&3u32.to_le_bytes());
    dib[20..24].copy_from_slice(&image.to_le_bytes());
    for (index, mask) in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000]
        .iter()
        .enumerate()
    {
        dib[40 + index * 4..44 + index * 4].copy_from_slice(&mask.to_le_bytes());
    }
    dib[56..60].copy_from_slice(&0x7352_4742u32.to_le_bytes());
    dib[108..112].copy_from_slice(&4u32.to_le_bytes());
    Ok(dib)
}

pub(super) fn to_bmp(dib: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    let layout = Layout::read(dib)?;
    let length = layout
        .end
        .checked_add(FILE_HEADER)
        .ok_or(ClipboardError::Capacity)?;
    let length_u32 = u32::try_from(length).map_err(|_| ClipboardError::Capacity)?;
    let pixels =
        u32::try_from(layout.pixels + FILE_HEADER).map_err(|_| ClipboardError::Capacity)?;
    let mut bmp = Vec::new();
    bmp.try_reserve_exact(length)
        .map_err(|_| ClipboardError::Capacity)?;
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&length_u32.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&pixels.to_le_bytes());
    bmp.extend_from_slice(&dib[..layout.end]);
    Ok(bmp)
}

struct Layout {
    pixels: usize,
    end: usize,
}

impl Layout {
    fn read(dib: &[u8]) -> Result<Self, ClipboardError> {
        let header = word(dib, 0)? as usize;
        if !matches!(header, INFO_HEADER | V4_HEADER | V5_HEADER) {
            return Err(ClipboardError::Unsupported);
        }
        if dib.len() < header || half(dib, 12)? != 1 {
            return Err(ClipboardError::InvalidData);
        }
        let width = word(dib, 4)? as i32;
        let height = word(dib, 8)? as i32;
        if width <= 0 || height == 0 {
            return Err(ClipboardError::InvalidData);
        }
        let depth = half(dib, 14)? as usize;
        let compression = word(dib, 16)?;
        match (compression, depth) {
            (0, 1 | 4 | 8 | 16 | 24 | 32) | (3, 16 | 32) => (),
            (1, 8) | (2, 4) if height > 0 => (),
            (0..=3, _) => return Err(ClipboardError::InvalidData),
            _ => return Err(ClipboardError::Unsupported),
        }
        let colors = word(dib, 32)? as usize;
        let colors = if depth <= 8 {
            if colors > 1 << depth {
                return Err(ClipboardError::InvalidData);
            }
            if colors == 0 { 1 << depth } else { colors }
        } else {
            colors
        };
        let masks = usize::from(header == INFO_HEADER && compression == 3) * 12;
        let pixels = colors
            .checked_mul(4)
            .and_then(|palette| header.checked_add(masks)?.checked_add(palette))
            .ok_or(ClipboardError::Capacity)?;
        let declared = word(dib, 20)? as usize;
        let image = if matches!(compression, 0 | 3) {
            let stride = (width as usize)
                .checked_mul(depth)
                .and_then(|bits| bits.checked_add(31))
                .map(|bits| (bits / 32) * 4)
                .ok_or(ClipboardError::Capacity)?;
            let required = stride
                .checked_mul(height.unsigned_abs() as usize)
                .ok_or(ClipboardError::Capacity)?;
            if declared != 0 && declared != required {
                return Err(ClipboardError::InvalidData);
            }
            required
        } else {
            if declared == 0 {
                return Err(ClipboardError::InvalidData);
            }
            declared
        };
        let mut end = pixels.checked_add(image).ok_or(ClipboardError::Capacity)?;
        if header == V5_HEADER {
            match word(dib, 56)? {
                PROFILE_LINKED => return Err(ClipboardError::Unsupported),
                PROFILE_EMBEDDED => {
                    let profile = word(dib, 112)? as usize;
                    let size = word(dib, 116)? as usize;
                    if profile < end || size == 0 {
                        return Err(ClipboardError::InvalidData);
                    }
                    end = profile.checked_add(size).ok_or(ClipboardError::Capacity)?;
                }
                _ => (),
            }
        }
        if end > dib.len() {
            return Err(ClipboardError::InvalidData);
        }
        Ok(Self { pixels, end })
    }
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, ClipboardError> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
        .ok_or(ClipboardError::InvalidData)
}

fn half(bytes: &[u8], offset: usize) -> Result<u16, ClipboardError> {
    bytes
        .get(offset..offset + 2)
        .map(|value| u16::from_le_bytes(value.try_into().unwrap()))
        .ok_or(ClipboardError::InvalidData)
}

#[cfg(test)]
#[path = "bitmap/tests.rs"]
mod tests;
