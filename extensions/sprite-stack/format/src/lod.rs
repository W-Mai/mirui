//! Optional SLOD chunk containing sparse levels in base atlas coordinates.
pub mod bake;

use crate::StackDescriptor;
use mirx::{
    ChunkFlags, ChunkType, Reader,
    image::{ImageRef, SampleLayout},
};

pub const CHUNK_TYPE: ChunkType = ChunkType::new(0x534c).unwrap();
const HEADER: usize = 20;
const RECORD: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LodError {
    Duplicate,
    InvalidHeader,
    InvalidRecipe,
    InvalidLevel,
    InvalidSample,
    InvalidPalette,
    Limit,
}
impl core::fmt::Display for LodError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid sprite LOD: {self:?}")
    }
}
impl core::error::Error for LodError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LodColor {
    Indexed(u8),
    Rgba([u8; 4]),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LodCell {
    pub x: u16,
    pub y: u16,
    pub color: LodColor,
}

fn u16_at(bytes: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], pos: usize) -> usize {
    u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize
}

/// Extra levels only. The primary IMAGE and SSTK describe step one.
#[derive(Clone, Copy, Debug)]
pub struct StackLods<'a> {
    payload: &'a [u8],
    table: &'a [u8],
    ramps: &'a [u8],
    layers: u16,
    stride: u8,
}
impl<'a> StackLods<'a> {
    pub fn from_reader(
        reader: &Reader<'a>,
        stack: StackDescriptor<'a>,
        image: ImageRef<'a>,
    ) -> Result<Option<Self>, LodError> {
        let mut chunks = reader.chunks().filter(|c| c.chunk_type() == CHUNK_TYPE);
        let Some(chunk) = chunks.next() else {
            return Ok(None);
        };
        if chunks.next().is_some() {
            return Err(LodError::Duplicate);
        }
        if chunk.flags() != ChunkFlags::NONE {
            return Err(LodError::InvalidHeader);
        }
        Self::open(chunk.payload(), stack, image).map(Some)
    }
    pub fn open(
        p: &'a [u8],
        stack: StackDescriptor<'a>,
        image: ImageRef<'a>,
    ) -> Result<Self, LodError> {
        if p.len() < HEADER
            || &p[..4] != b"SLOD"
            || u16_at(p, 4) != 1
            || u16_at(p, 18) != 0
            || (u16_at(p, 14), u16_at(p, 16)) != stack.canvas()
        {
            return Err(LodError::InvalidHeader);
        }
        let indexed = image.surface().sample_layout().is_indexed();
        let sample_width = if indexed {
            1
        } else if image.surface().sample_layout() == SampleLayout::RGBA8888 {
            4
        } else {
            return Err(LodError::InvalidPalette);
        };
        let stride = 4 + sample_width;
        let layers = u16_at(p, 10);
        let count = usize::from(u16_at(p, 8));
        if p[6] != sample_width || usize::from(layers) != stack.layers().len() || count == 0 {
            return Err(LodError::InvalidHeader);
        }
        let nr = usize::from(u16_at(p, 12));
        let ramp_end = HEADER + nr * 2;
        let table_end = ramp_end + count * RECORD;
        let ramps = p.get(HEADER..ramp_end).ok_or(LodError::InvalidRecipe)?;
        if p[7] != u8::from(nr > 0) || (nr > 0 && !indexed) {
            return Err(LodError::InvalidRecipe);
        }
        validate_ramps(ramps.chunks_exact(2).map(|r| [r[0], r[1]]))?;
        let table = p.get(ramp_end..table_end).ok_or(LodError::InvalidLevel)?;
        let this = Self {
            payload: p,
            table,
            ramps,
            layers,
            stride,
        };
        let palette = image.color_table().map(|t| t.as_bytes());
        let mut last_step = 1;
        let mut next = table_end;
        for record in table.chunks_exact(RECORD) {
            let step = u16_at(record, 0);
            let len = u32_at(record, 8);
            if step <= last_step || u16_at(record, 2) != 0 || u32_at(record, 4) != next {
                return Err(LodError::InvalidLevel);
            }
            next = next.checked_add(len).ok_or(LodError::Limit)?;
            let level = this.read_level(record).ok_or(LodError::InvalidLevel)?;
            let offsets = level.offsets;
            if u32_at(offsets, 0) != 0
                || u32_at(offsets, offsets.len() - 4) != level.samples.len() / usize::from(stride)
            {
                return Err(LodError::InvalidLevel);
            }
            for layer in 0..usize::from(layers) {
                let mut prev = None;
                let samples = level.layer(layer).ok_or(LodError::InvalidLevel)?;
                if samples.len() != 0
                    && stack
                        .layers()
                        .get(layer)
                        .ok_or(LodError::InvalidLevel)?
                        .width()
                        == 0
                {
                    return Err(LodError::InvalidLevel);
                }
                for cell in samples {
                    let (width, height) = stack.canvas();
                    let key = (cell.y, cell.x);
                    if cell.x >= width
                        || cell.y >= height
                        || cell.x % step != 0
                        || cell.y % step != 0
                        || prev.is_some_and(|v| v >= key)
                    {
                        return Err(LodError::InvalidSample);
                    }
                    match cell.color {
                        LodColor::Indexed(i)
                            if palette.and_then(|p| p.get(usize::from(i) * 4 + 3))
                                != Some(&255) =>
                        {
                            return Err(LodError::InvalidPalette);
                        }
                        LodColor::Rgba([_, _, _, a]) if a != 255 => {
                            return Err(LodError::InvalidSample);
                        }
                        _ => {}
                    }
                    prev = Some(key)
                }
            }
            last_step = step;
        }
        if next != p.len() {
            return Err(LodError::InvalidLevel);
        }
        Ok(this)
    }
    fn read_level(self, r: &[u8]) -> Option<LodLevel<'a>> {
        let at = u32_at(r, 4);
        let tail = at.checked_add(u32_at(r, 8))?;
        let data = self.payload.get(at..tail)?;
        let (offsets, samples) = data.split_at_checked((usize::from(self.layers) + 1) * 4)?;
        if samples.len() % usize::from(self.stride) != 0 {
            return None;
        }
        Some(LodLevel {
            step: u16_at(r, 0),
            offsets,
            samples,
            stride: self.stride,
        })
    }
    pub fn len(self) -> usize {
        self.table.len() / RECORD
    }
    pub fn is_empty(self) -> bool {
        self.table.is_empty()
    }
    pub fn level(self, index: usize) -> Option<LodLevel<'a>> {
        self.read_level(self.table.chunks_exact(RECORD).nth(index)?)
    }
    /// A nonempty sequence gives material ramps in tie-breaking order.
    pub fn ramps(self) -> impl ExactSizeIterator<Item = [u8; 2]> + 'a {
        self.ramps.chunks_exact(2).map(|r| [r[0], r[1]])
    }
}
fn validate_ramps(ranges: impl IntoIterator<Item = [u8; 2]>) -> Result<(), LodError> {
    let mut used = [false; 256];
    for [lo, hi] in ranges {
        if lo > hi {
            return Err(LodError::InvalidRecipe);
        }
        for i in lo..=hi {
            if core::mem::replace(&mut used[usize::from(i)], true) {
                return Err(LodError::InvalidRecipe);
            }
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug)]
pub struct LodLevel<'a> {
    step: u16,
    offsets: &'a [u8],
    samples: &'a [u8],
    stride: u8,
}
impl<'a> LodLevel<'a> {
    pub fn step(self) -> u16 {
        self.step
    }
    pub fn layer(self, layer: usize) -> Option<impl ExactSizeIterator<Item = LodCell> + 'a> {
        let i = layer.checked_mul(4)?;
        let bounds = self.offsets.get(i..)?.get(..8)?;
        let width = usize::from(self.stride);
        let span = self
            .samples
            .get(u32_at(bounds, 0).checked_mul(width)?..u32_at(bounds, 4).checked_mul(width)?)?;
        Some(span.chunks_exact(width).map(|s| LodCell {
            x: u16_at(s, 0),
            y: u16_at(s, 2),
            color: if s.len() == 5 {
                LodColor::Indexed(s[4])
            } else {
                LodColor::Rgba(s[4..8].try_into().unwrap())
            },
        }))
    }
}

#[cfg(test)]
mod tests;
