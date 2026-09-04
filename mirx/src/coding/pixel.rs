use crate::{
    image::SampleLayout,
    media::{CodingId, CodingRecord},
};

const RGB: u8 = 0x3e;
const RGBA: u8 = 0x3f;
const MAX_RUN: usize = 62;

#[cfg(test)]
mod tests;

/// Lossless RGB/RGBA pixel coding with a color cache, deltas, and previous runs.
///
/// Every stream resets its state. Inputs and outputs are tight RGB888 or
/// RGBA8888 samples; row padding, integrity, and unit geometry belong to callers.
/// This is a MIRX sample stream, not a standard QOI file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pixel {
    layout: SampleLayout,
}

impl Pixel {
    pub fn new(layout: SampleLayout) -> Result<Self, PixelError> {
        if !matches!(layout, SampleLayout::RGB888 | SampleLayout::RGBA8888) {
            return Err(PixelError::UnsupportedLayout(layout));
        }
        Ok(Self { layout })
    }

    pub fn from_record(record: CodingRecord<'_>, layout: SampleLayout) -> Result<Self, PixelError> {
        if record.id() != CodingId::PIXEL {
            return Err(PixelError::UnexpectedCoding(record.id()));
        }
        if record.revision() != 1 {
            return Err(PixelError::UnsupportedRevision(record.revision()));
        }
        if !record.params().is_empty() {
            return Err(PixelError::UnexpectedParameters);
        }
        Self::new(layout)
    }

    pub const fn layout(self) -> SampleLayout {
        self.layout
    }

    pub const fn record(self) -> CodingRecord<'static> {
        CodingRecord::new(CodingId::PIXEL, 1, &[])
    }

    /// Checked upper bound for canonical encoding of `pixel_count` samples.
    pub fn encoded_bound(self, pixel_count: usize) -> Result<usize, PixelError> {
        pixel_count
            .checked_mul(self.channels() + 1)
            .ok_or(PixelError::SizeOverflow)
    }

    /// Exact canonical byte count without allocation or output writes.
    pub fn encoded_len(self, samples: &[u8]) -> Result<usize, PixelError> {
        self.encode(samples, &mut Emitter::count())
    }

    /// Encodes tight samples after validating geometry and exact capacity.
    ///
    /// Errors leave all output unchanged; bytes after the encoded prefix remain
    /// untouched. Counting and emission use the same canonical token generator.
    pub fn encode_into(self, samples: &[u8], output: &mut [u8]) -> Result<usize, PixelError> {
        let needed = self.encoded_len(samples)?;
        if output.len() < needed {
            return Err(PixelError::OutputTooSmall {
                needed,
                available: output.len(),
            });
        }
        self.encode(
            samples,
            &mut Emitter {
                output: Some(&mut output[..needed]),
                position: 0,
            },
        )
        .expect("validated pixel encoding");
        Ok(needed)
    }

    /// Validates a complete independent stream without decoding into an output.
    ///
    /// Runs are checked as tokens, not expanded during validation. The plan
    /// borrows immutable input and can replay it without fallible parsing.
    /// Media integrity must be checked separately before trusting sample bytes.
    pub fn plan(self, input: &[u8], pixel_count: usize) -> Result<PixelDecodePlan<'_>, PixelError> {
        let decoded_len = pixel_count
            .checked_mul(self.channels())
            .ok_or(PixelError::SizeOverflow)?;
        let mut cursor = Cursor::new(input);
        let mut state = State::new(self);
        let mut remaining = pixel_count;
        while remaining > 0 {
            let (_, count) = state.read(&mut cursor)?;
            remaining = remaining
                .checked_sub(count)
                .ok_or(PixelError::RunOverflow { remaining, count })?;
        }
        if cursor.position != input.len() {
            return Err(PixelError::TrailingData {
                offset: cursor.position,
            });
        }
        Ok(PixelDecodePlan {
            codec: self,
            input,
            decoded_len,
        })
    }

    fn channels(self) -> usize {
        usize::from(
            self.layout
                .color_format()
                .expect("pixel layout")
                .bits_per_pixel(),
        ) / 8
    }

    fn encode(self, samples: &[u8], emitter: &mut Emitter<'_>) -> Result<usize, PixelError> {
        let channels = self.channels();
        if samples.len() % channels != 0 {
            return Err(PixelError::PartialPixel {
                bytes: samples.len(),
                channels: channels as u8,
            });
        }
        let pixels = samples.len() / channels;
        let mut state = State::new(self);
        let mut index = 0;
        while index < pixels {
            let value = self.sample(samples, index);
            if value == state.previous {
                let mut run = 1;
                while run < MAX_RUN
                    && run < pixels - index
                    && self.sample(samples, index + run) == value
                {
                    run += 1;
                }
                emitter.put(&[(run - 1) as u8])?;
                state.remember(value);
                index += run;
                continue;
            }
            let slot = State::slot(value);
            if state.cache[slot] == value {
                emitter.put(&[0x40 | slot as u8])?;
            } else if value[3] != state.previous[3] {
                emitter.put(&[RGBA])?;
                emitter.put(&value)?;
            } else {
                let dr = value[0].wrapping_sub(state.previous[0]) as i8 as i16;
                let dg = value[1].wrapping_sub(state.previous[1]) as i8 as i16;
                let db = value[2].wrapping_sub(state.previous[2]) as i8 as i16;
                let rg = (dr - dg) as i8 as i16;
                let bg = (db - dg) as i8 as i16;
                if (-2..=1).contains(&dr) && (-2..=1).contains(&dg) && (-2..=1).contains(&db) {
                    emitter.put(&[0x80
                        | ((dr + 2) as u8) << 4
                        | ((dg + 2) as u8) << 2
                        | (db + 2) as u8])?;
                } else if (-32..=31).contains(&dg)
                    && (-8..=7).contains(&rg)
                    && (-8..=7).contains(&bg)
                {
                    emitter.put(&[
                        0xc0 | (dg + 32) as u8,
                        ((rg + 8) as u8) << 4 | (bg + 8) as u8,
                    ])?;
                } else {
                    emitter.put(&[RGB])?;
                    emitter.put(&value[..3])?;
                }
            }
            state.remember(value);
            index += 1;
        }
        Ok(emitter.position)
    }

    fn sample(self, bytes: &[u8], index: usize) -> [u8; 4] {
        let channels = self.channels();
        let sample = &bytes[index * channels..(index + 1) * channels];
        [
            sample[0],
            sample[1],
            sample[2],
            if channels == 4 { sample[3] } else { 255 },
        ]
    }
}

/// Validated independent pixel stream and exact tight output size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelDecodePlan<'a> {
    codec: Pixel,
    input: &'a [u8],
    decoded_len: usize,
}
impl<'a> PixelDecodePlan<'a> {
    pub const fn codec(self) -> Pixel {
        self.codec
    }
    pub const fn input(self) -> &'a [u8] {
        self.input
    }
    pub const fn decoded_len(self) -> usize {
        self.decoded_len
    }
    pub fn pixel_count(self) -> usize {
        self.decoded_len / self.codec.channels()
    }

    /// Writes tight samples without allocation, preserving the output suffix.
    ///
    /// All fallible checks precede writes. A failed capacity check leaves the
    /// destination unchanged; malformed streams cannot produce this plan.
    pub fn decode_into(self, output: &mut [u8]) -> Result<usize, PixelError> {
        if output.len() < self.decoded_len {
            return Err(PixelError::OutputTooSmall {
                needed: self.decoded_len,
                available: output.len(),
            });
        }
        let channels = self.codec.channels();
        let mut position = 0;
        self.for_each_run(|value, count| {
            let end = position + count * channels;
            for target in output[position..end].chunks_exact_mut(channels) {
                target.copy_from_slice(&value[..channels]);
            }
            position = end;
        });
        Ok(self.decoded_len)
    }

    pub(crate) fn for_each_run(self, mut consume: impl FnMut([u8; 4], usize)) {
        let mut state = State::new(self.codec);
        let mut cursor = Cursor::new(self.input);
        while cursor.position < self.input.len() {
            let (value, count) = state.read(&mut cursor).expect("validated pixel stream");
            consume(value, count);
        }
    }
}

/// Invalid pixel profile, sample geometry, bitstream, or caller buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PixelError {
    UnsupportedLayout(SampleLayout),
    UnexpectedCoding(CodingId),
    UnsupportedRevision(u16),
    UnexpectedParameters,
    PartialPixel { bytes: usize, channels: u8 },
    SizeOverflow,
    OutputTooSmall { needed: usize, available: usize },
    Truncated { offset: usize },
    InvalidOpcode { offset: usize, opcode: u8 },
    InvalidAlpha { offset: usize },
    RunOverflow { remaining: usize, count: usize },
    TrailingData { offset: usize },
}

struct State {
    codec: Pixel,
    previous: [u8; 4],
    cache: [[u8; 4]; 64],
}
impl State {
    fn new(codec: Pixel) -> Self {
        Self {
            codec,
            previous: [0, 0, 0, 255],
            cache: [[0; 4]; 64],
        }
    }
    fn slot(value: [u8; 4]) -> usize {
        (usize::from(value[0]) * 3
            + usize::from(value[1]) * 5
            + usize::from(value[2]) * 7
            + usize::from(value[3]) * 11)
            & 63
    }
    fn remember(&mut self, value: [u8; 4]) {
        self.previous = value;
        self.cache[Self::slot(value)] = value;
    }
    fn read(&mut self, cursor: &mut Cursor<'_>) -> Result<([u8; 4], usize), PixelError> {
        let offset = cursor.position;
        let op = cursor.byte()?;
        let mut value = self.previous;
        let mut count = 1;
        match op {
            RGB => value[..3].copy_from_slice(cursor.take(3)?),
            RGBA => {
                if self.codec.channels() != 4 {
                    return Err(PixelError::InvalidOpcode { offset, opcode: op });
                }
                value.copy_from_slice(cursor.take(4)?);
            }
            0..=0x3d => count = usize::from(op) + 1,
            0x40..=0x7f => value = self.cache[usize::from(op & 63)],
            0x80..=0xbf => {
                value[0] = value[0].wrapping_add(((op >> 4) & 3).wrapping_sub(2));
                value[1] = value[1].wrapping_add(((op >> 2) & 3).wrapping_sub(2));
                value[2] = value[2].wrapping_add((op & 3).wrapping_sub(2));
            }
            _ => {
                let green = (op & 63).wrapping_sub(32);
                let deltas = cursor.byte()?;
                value[0] = value[0]
                    .wrapping_add(green)
                    .wrapping_add((deltas >> 4).wrapping_sub(8));
                value[1] = value[1].wrapping_add(green);
                value[2] = value[2]
                    .wrapping_add(green)
                    .wrapping_add((deltas & 15).wrapping_sub(8));
            }
        }
        if self.codec.channels() == 3 && value[3] != 255 {
            return Err(PixelError::InvalidAlpha { offset });
        }
        self.remember(value);
        Ok((value, count))
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], PixelError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(PixelError::SizeOverflow)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(PixelError::Truncated {
                offset: self.position,
            })?;
        self.position = end;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, PixelError> {
        Ok(self.take(1)?[0])
    }
}
struct Emitter<'a> {
    output: Option<&'a mut [u8]>,
    position: usize,
}
impl Emitter<'_> {
    fn count() -> Self {
        Self {
            output: None,
            position: 0,
        }
    }
    fn put(&mut self, bytes: &[u8]) -> Result<(), PixelError> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or(PixelError::SizeOverflow)?;
        if let Some(output) = &mut self.output {
            output[self.position..end].copy_from_slice(bytes);
        }
        self.position = end;
        Ok(())
    }
}
