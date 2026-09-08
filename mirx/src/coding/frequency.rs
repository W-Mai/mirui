use super::{
    CodingId,
    buffer::{BufferError, Cursor, Emitter},
};
use crate::{image::SampleLayout, media::CodingRecord};

#[cfg(test)]
mod tests;

const BLOCK: usize = 8;
const COEFFICIENTS: usize = BLOCK * BLOCK;
const MAX_COMPONENTS: usize = 4;

/// One revision-1 frequency profile.
///
/// Reversible streams reconstruct every admitted sample exactly. Quantized
/// streams carry an explicit quality from 1 through 100; alpha and indexes
/// remain lossless at every quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frequency {
    quality: Option<u8>,
}

impl Frequency {
    pub const fn reversible() -> Self {
        Self { quality: None }
    }

    pub fn quantized(quality: u8) -> Result<Self, FrequencyError> {
        if !(1..=100).contains(&quality) {
            return Err(FrequencyError::InvalidQuality(quality));
        }
        Ok(Self {
            quality: Some(quality),
        })
    }

    pub const fn is_reversible(self) -> bool {
        self.quality.is_none()
    }

    pub const fn quality(self) -> Option<u8> {
        self.quality
    }

    pub const fn coding_id(self) -> CodingId {
        match self.quality {
            None => CodingId::FREQUENCY_REVERSIBLE,
            Some(_) => CodingId::FREQUENCY_QUANTIZED,
        }
    }

    /// Writes profile parameters and returns a record borrowing that storage.
    ///
    /// Reversible coding borrows an empty prefix. Quantized coding writes its
    /// one-byte quality. This keeps the record allocation-free without hiding
    /// parameter ownership inside the coding value.
    pub fn record_into<'a>(self, params: &'a mut [u8; 1]) -> CodingRecord<'a> {
        let bytes = match self.quality {
            None => &params[..0],
            Some(quality) => {
                params[0] = quality;
                &params[..1]
            }
        };
        CodingRecord::new(self.coding_id(), 1, bytes)
    }

    pub fn from_record(record: CodingRecord<'_>) -> Result<Self, FrequencyError> {
        if !matches!(
            record.id(),
            CodingId::FREQUENCY_REVERSIBLE | CodingId::FREQUENCY_QUANTIZED
        ) {
            return Err(FrequencyError::UnexpectedCoding(record.id()));
        }
        if record.revision() != 1 {
            return Err(FrequencyError::UnsupportedRevision {
                coding: record.id(),
                revision: record.revision(),
            });
        }
        match record.id() {
            CodingId::FREQUENCY_REVERSIBLE => {
                if !record.params().is_empty() {
                    return Err(FrequencyError::UnexpectedParameters(record.id()));
                }
                Ok(Self::reversible())
            }
            CodingId::FREQUENCY_QUANTIZED => match record.params() {
                [quality] => Self::quantized(*quality),
                _ => Err(FrequencyError::UnexpectedParameters(record.id())),
            },
            _ => unreachable!("coding identifier checked above"),
        }
    }

    /// Conservative canonical bound: one control plus a five-byte signed value
    /// per coefficient. Actual encoder coefficients are materially smaller.
    pub fn encoded_bound(self, geometry: FrequencyGeometry) -> Result<usize, FrequencyError> {
        geometry
            .coefficient_count()?
            .checked_mul(6)
            .ok_or(FrequencyError::SizeOverflow)
    }

    /// Counts exact canonical bytes without allocating or mutating output.
    pub fn encoded_len(
        self,
        geometry: FrequencyGeometry,
        samples: &[u8],
    ) -> Result<usize, FrequencyError> {
        let mut emitter = Emitter::count();
        self.encode(geometry, samples, &mut emitter)?;
        Ok(emitter.position)
    }

    /// Encodes tight plane samples into a deterministic coefficient stream.
    ///
    /// Validation and exact sizing precede all writes. The output suffix is
    /// preserved, and any error leaves the entire output unchanged.
    pub fn encode_into(
        self,
        geometry: FrequencyGeometry,
        samples: &[u8],
        output: &mut [u8],
    ) -> Result<usize, FrequencyError> {
        let needed = self.encoded_len(geometry, samples)?;
        if output.len() < needed {
            return Err(FrequencyError::OutputTooSmall {
                needed,
                available: output.len(),
            });
        }
        self.encode(
            geometry,
            samples,
            &mut Emitter {
                output: Some(&mut output[..needed]),
                position: 0,
            },
        )?;
        Ok(needed)
    }

    /// Validates one exact plane stream and its reconstructed sample range.
    pub fn plan<'a>(
        self,
        input: &'a [u8],
        geometry: FrequencyGeometry,
    ) -> Result<FrequencyDecodePlan<'a>, FrequencyError> {
        let mut cursor = Cursor::new(input);
        self.replay(geometry, &mut cursor, None)?;
        if !cursor.is_empty() {
            return Err(FrequencyError::TrailingData {
                offset: cursor.position,
            });
        }
        Ok(FrequencyDecodePlan {
            codec: self,
            geometry,
            input,
        })
    }

    pub(crate) fn plan_prefix<'a>(
        self,
        input: &'a [u8],
        geometry: FrequencyGeometry,
    ) -> Result<(FrequencyDecodePlan<'a>, usize), FrequencyError> {
        let mut cursor = Cursor::new(input);
        self.replay(geometry, &mut cursor, None)?;
        let consumed = cursor.position;
        Ok((
            FrequencyDecodePlan {
                codec: self,
                geometry,
                input: &input[..consumed],
            },
            consumed,
        ))
    }

    fn encode(
        self,
        geometry: FrequencyGeometry,
        samples: &[u8],
        emitter: &mut Emitter<'_>,
    ) -> Result<(), FrequencyError> {
        let expected = geometry.decoded_len()?;
        if samples.len() != expected {
            return Err(FrequencyError::SampleLengthMismatch {
                expected,
                actual: samples.len(),
            });
        }
        let blocks_x = geometry.width.div_ceil(BLOCK as u32);
        let blocks_y = geometry.height.div_ceil(BLOCK as u32);
        for block_y in 0..blocks_y {
            for block_x in 0..blocks_x {
                for component in 0..geometry.components {
                    let mut values = [0i64; COEFFICIENTS];
                    geometry.load_component(samples, block_x, block_y, component, &mut values);
                    forward_2d(&mut values);
                    for (index, coefficient) in values.iter_mut().enumerate() {
                        *coefficient = self.quantize(*coefficient, geometry, component, index)?;
                    }
                    emit_coefficients(&values, emitter)?;
                }
            }
        }
        Ok(())
    }

    fn replay(
        self,
        geometry: FrequencyGeometry,
        cursor: &mut Cursor<'_>,
        mut output: Option<&mut [u8]>,
    ) -> Result<(), FrequencyError> {
        let blocks_x = geometry.width.div_ceil(BLOCK as u32);
        let blocks_y = geometry.height.div_ceil(BLOCK as u32);
        for block_y in 0..blocks_y {
            for block_x in 0..blocks_x {
                let mut components = [[0i64; COEFFICIENTS]; MAX_COMPONENTS];
                for component in 0..geometry.components {
                    let values = &mut components[usize::from(component)];
                    read_coefficients(cursor, values)?;
                    for (index, coefficient) in values.iter_mut().enumerate() {
                        *coefficient = self.dequantize(*coefficient, geometry, component, index)?;
                    }
                    inverse_2d(values);
                }
                geometry.store_block(
                    &components,
                    block_x,
                    block_y,
                    self.is_reversible(),
                    output.as_deref_mut(),
                )?;
            }
        }
        Ok(())
    }

    fn quantize(
        self,
        coefficient: i64,
        geometry: FrequencyGeometry,
        component: u8,
        index: usize,
    ) -> Result<i64, FrequencyError> {
        let step = i64::from(self.step(geometry, component, index));
        let magnitude = coefficient
            .checked_abs()
            .ok_or(FrequencyError::CoefficientOverflow)?;
        let value = magnitude
            .checked_add(step / 2)
            .ok_or(FrequencyError::CoefficientOverflow)?
            / step;
        let value = if coefficient < 0 { -value } else { value };
        i32::try_from(value)
            .map(i64::from)
            .map_err(|_| FrequencyError::CoefficientOverflow)
    }

    fn dequantize(
        self,
        coefficient: i64,
        geometry: FrequencyGeometry,
        component: u8,
        index: usize,
    ) -> Result<i64, FrequencyError> {
        coefficient
            .checked_mul(i64::from(self.step(geometry, component, index)))
            .ok_or(FrequencyError::CoefficientOverflow)
    }

    fn step(self, geometry: FrequencyGeometry, component: u8, index: usize) -> u16 {
        let Some(quality) = self.quality else {
            return 1;
        };
        if index == 0 || geometry.lossless_components & (1 << component) != 0 {
            return 1;
        }
        let x = index % BLOCK;
        let y = index / BLOCK;
        let extent = x.max(y);
        let band_weight = if extent <= 1 {
            1u16
        } else if extent <= 3 {
            2
        } else {
            4
        };
        let component_weight = if geometry.is_chroma_component(component) {
            2u16
        } else {
            1
        };
        let loss = u16::from(101 - quality);
        1 + (loss * band_weight * component_weight).div_ceil(32)
    }
}

/// Tight dimensions and component interpretation for one coded plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrequencyGeometry {
    layout: SampleLayout,
    plane_index: u8,
    width: u32,
    height: u32,
    components: u8,
    color: ColorTransform,
    lossless_components: u8,
    planar_chroma: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ColorTransform {
    None,
    Rgb,
    Bgr,
}

impl FrequencyGeometry {
    /// Defines one local plane. Width and height are the plane/unit extents,
    /// not the parent surface dimensions.
    pub fn for_plane(
        layout: SampleLayout,
        plane_index: u8,
        width: u32,
        height: u32,
    ) -> Result<Self, FrequencyError> {
        let (components, color, lossless_components, planar_chroma) = match (layout, plane_index) {
            (SampleLayout::I8 | SampleLayout::A8, 0) => (1, ColorTransform::None, 1, false),
            (SampleLayout::L8, 0) => (1, ColorTransform::None, 0, false),
            (SampleLayout::RGB888, 0) => (3, ColorTransform::Rgb, 0, false),
            (SampleLayout::RGBA8888, 0) => (4, ColorTransform::Rgb, 1 << 3, false),
            (SampleLayout::BGRA8888, 0) => (4, ColorTransform::Bgr, 1 << 3, false),
            (SampleLayout::I420 | SampleLayout::YV12, 0) => (1, ColorTransform::None, 0, false),
            (SampleLayout::I420 | SampleLayout::YV12, 1 | 2) => (1, ColorTransform::None, 0, true),
            (SampleLayout::NV12 | SampleLayout::NV21, 0) => (1, ColorTransform::None, 0, false),
            (SampleLayout::NV12 | SampleLayout::NV21, 1) => (2, ColorTransform::None, 0, true),
            _ => {
                return Err(FrequencyError::UnsupportedLayout {
                    layout,
                    plane_index,
                });
            }
        };
        let geometry = Self {
            layout,
            plane_index,
            width,
            height,
            components,
            color,
            lossless_components,
            planar_chroma,
        };
        geometry.decoded_len()?;
        Ok(geometry)
    }

    pub const fn layout(self) -> SampleLayout {
        self.layout
    }

    pub const fn plane_index(self) -> u8 {
        self.plane_index
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn components(self) -> u8 {
        self.components
    }

    pub fn decoded_len(self) -> Result<usize, FrequencyError> {
        usize::try_from(u64::from(self.width) * u64::from(self.height))
            .ok()
            .and_then(|samples| samples.checked_mul(usize::from(self.components)))
            .ok_or(FrequencyError::SizeOverflow)
    }

    pub(crate) fn coefficient_count(self) -> Result<usize, FrequencyError> {
        let blocks = u64::from(self.width.div_ceil(BLOCK as u32))
            .checked_mul(u64::from(self.height.div_ceil(BLOCK as u32)))
            .and_then(|count| count.checked_mul(u64::from(self.components)))
            .and_then(|count| count.checked_mul(COEFFICIENTS as u64))
            .ok_or(FrequencyError::SizeOverflow)?;
        usize::try_from(blocks).map_err(|_| FrequencyError::SizeOverflow)
    }

    fn is_chroma_component(self, component: u8) -> bool {
        self.planar_chroma || self.color != ColorTransform::None && matches!(component, 1 | 2)
    }

    fn load_component(
        self,
        samples: &[u8],
        block_x: u32,
        block_y: u32,
        component: u8,
        values: &mut [i64; COEFFICIENTS],
    ) {
        for y in 0..BLOCK {
            let sample_y = (block_y * BLOCK as u32 + y as u32).min(self.height - 1);
            for x in 0..BLOCK {
                let sample_x = (block_x * BLOCK as u32 + x as u32).min(self.width - 1);
                let pixel = (sample_y as usize * self.width as usize + sample_x as usize)
                    * usize::from(self.components);
                values[y * BLOCK + x] = self.load_sample(samples, pixel, component);
            }
        }
    }

    fn load_sample(self, samples: &[u8], pixel: usize, component: u8) -> i64 {
        match self.color {
            ColorTransform::None => i64::from(samples[pixel + usize::from(component)]) - 128,
            ColorTransform::Rgb | ColorTransform::Bgr => {
                let (r, g, b) = match self.color {
                    ColorTransform::Rgb => (
                        i64::from(samples[pixel]),
                        i64::from(samples[pixel + 1]),
                        i64::from(samples[pixel + 2]),
                    ),
                    ColorTransform::Bgr => (
                        i64::from(samples[pixel + 2]),
                        i64::from(samples[pixel + 1]),
                        i64::from(samples[pixel]),
                    ),
                    ColorTransform::None => unreachable!(),
                };
                let co = r - b;
                let temporary = b + (co >> 1);
                let cg = g - temporary;
                let y = temporary + (cg >> 1);
                match component {
                    0 => y - 128,
                    1 => co,
                    2 => cg,
                    _ => i64::from(samples[pixel + usize::from(component)]) - 128,
                }
            }
        }
    }

    fn store_block(
        self,
        components: &[[i64; COEFFICIENTS]; MAX_COMPONENTS],
        block_x: u32,
        block_y: u32,
        reversible: bool,
        mut output: Option<&mut [u8]>,
    ) -> Result<(), FrequencyError> {
        for y in 0..BLOCK {
            let sample_y = block_y * BLOCK as u32 + y as u32;
            if sample_y >= self.height {
                continue;
            }
            for x in 0..BLOCK {
                let sample_x = block_x * BLOCK as u32 + x as u32;
                if sample_x >= self.width {
                    continue;
                }
                let index = y * BLOCK + x;
                let pixel = (sample_y as usize * self.width as usize + sample_x as usize)
                    * usize::from(self.components);
                let mut bytes = [0u8; MAX_COMPONENTS];
                self.reconstruct_pixel(components, index, reversible, &mut bytes)?;
                if let Some(output) = output.as_deref_mut() {
                    let len = usize::from(self.components);
                    output[pixel..pixel + len].copy_from_slice(&bytes[..len]);
                }
            }
        }
        Ok(())
    }

    fn reconstruct_pixel(
        self,
        components: &[[i64; COEFFICIENTS]; MAX_COMPONENTS],
        index: usize,
        reversible: bool,
        output: &mut [u8; MAX_COMPONENTS],
    ) -> Result<(), FrequencyError> {
        match self.color {
            ColorTransform::None => {
                for component in 0..self.components {
                    output[usize::from(component)] = reconstructed_byte(
                        components[usize::from(component)][index] + 128,
                        reversible || self.lossless_components & (1 << component) != 0,
                    )?;
                }
            }
            ColorTransform::Rgb | ColorTransform::Bgr => {
                let y = components[0][index] + 128;
                let co = components[1][index];
                let cg = components[2][index];
                let temporary = y - (cg >> 1);
                let g = cg + temporary;
                let b = temporary - (co >> 1);
                let r = b + co;
                let exact = reversible;
                let r = reconstructed_byte(r, exact)?;
                let g = reconstructed_byte(g, exact)?;
                let b = reconstructed_byte(b, exact)?;
                match self.color {
                    ColorTransform::Rgb => output[..3].copy_from_slice(&[r, g, b]),
                    ColorTransform::Bgr => output[..3].copy_from_slice(&[b, g, r]),
                    ColorTransform::None => unreachable!(),
                }
                if self.components == 4 {
                    output[3] = reconstructed_byte(components[3][index] + 128, true)?;
                }
            }
        }
        Ok(())
    }
}

/// Validated exact frequency stream and tight output requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrequencyDecodePlan<'a> {
    codec: Frequency,
    geometry: FrequencyGeometry,
    input: &'a [u8],
}

impl<'a> FrequencyDecodePlan<'a> {
    pub const fn codec(self) -> Frequency {
        self.codec
    }

    pub const fn geometry(self) -> FrequencyGeometry {
        self.geometry
    }

    pub const fn input(self) -> &'a [u8] {
        self.input
    }

    pub fn decoded_len(self) -> usize {
        self.geometry
            .decoded_len()
            .expect("validated frequency geometry")
    }

    /// Replays validated blocks into tight caller storage.
    pub fn decode_into(self, output: &mut [u8]) -> Result<usize, FrequencyError> {
        let needed = self.decoded_len();
        if output.len() < needed {
            return Err(FrequencyError::OutputTooSmall {
                needed,
                available: output.len(),
            });
        }
        let mut cursor = Cursor::new(self.input);
        self.codec
            .replay(self.geometry, &mut cursor, Some(&mut output[..needed]))?;
        debug_assert!(cursor.is_empty());
        Ok(needed)
    }
}

fn emit_coefficients(
    coefficients: &[i64; COEFFICIENTS],
    emitter: &mut Emitter<'_>,
) -> Result<(), FrequencyError> {
    let mut index = 0;
    while index < COEFFICIENTS {
        if coefficients[index] == 0 {
            let start = index;
            while index < COEFFICIENTS && coefficients[index] == 0 && index - start < 128 {
                index += 1;
            }
            emitter.put(&[(index - start - 1) as u8])?;
            continue;
        }
        let start = index;
        while index < COEFFICIENTS && coefficients[index] != 0 && index - start < 128 {
            index += 1;
        }
        emitter.put(&[0x80 | (index - start - 1) as u8])?;
        for coefficient in &coefficients[start..index] {
            let value =
                i32::try_from(*coefficient).map_err(|_| FrequencyError::CoefficientOverflow)?;
            emit_varint(zigzag(value), emitter)?;
        }
    }
    Ok(())
}

fn read_coefficients(
    cursor: &mut Cursor<'_>,
    coefficients: &mut [i64; COEFFICIENTS],
) -> Result<(), FrequencyError> {
    let mut index = 0;
    while index < COEFFICIENTS {
        let op = cursor.byte()?;
        let count = usize::from(op & 0x7f) + 1;
        let end = index
            .checked_add(count)
            .filter(|end| *end <= COEFFICIENTS)
            .ok_or(FrequencyError::CoefficientRunOverflow {
                remaining: COEFFICIENTS - index,
                count,
            })?;
        if op & 0x80 == 0 {
            coefficients[index..end].fill(0);
        } else {
            for coefficient in &mut coefficients[index..end] {
                let value = unzigzag(read_varint(cursor)?) as i64;
                if value == 0 {
                    return Err(FrequencyError::LiteralZero);
                }
                *coefficient = value;
            }
        }
        index = end;
    }
    Ok(())
}

fn emit_varint(mut value: u32, emitter: &mut Emitter<'_>) -> Result<(), BufferError> {
    let mut bytes = [0u8; 5];
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes[len] = byte;
        len += 1;
        if value == 0 {
            return emitter.put(&bytes[..len]);
        }
    }
}

fn read_varint(cursor: &mut Cursor<'_>) -> Result<u32, FrequencyError> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = cursor.byte()?;
        let payload = u32::from(byte & 0x7f);
        if index == 4 && payload > 0x0f {
            return Err(FrequencyError::VarintOverflow);
        }
        value |= payload << (index * 7);
        if byte & 0x80 == 0 {
            if index != 0 && payload == 0 {
                return Err(FrequencyError::NonCanonicalVarint);
            }
            return Ok(value);
        }
    }
    Err(FrequencyError::VarintOverflow)
}

const fn zigzag(value: i32) -> u32 {
    ((value as u32) << 1) ^ ((value >> 31) as u32)
}

const fn unzigzag(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

fn forward_2d(values: &mut [i64; COEFFICIENTS]) {
    for row in values.chunks_exact_mut(BLOCK) {
        let row: &mut [i64; BLOCK] = row.try_into().expect("eight-wide row");
        forward_1d(row);
    }
    for x in 0..BLOCK {
        let mut column = [0i64; BLOCK];
        for y in 0..BLOCK {
            column[y] = values[y * BLOCK + x];
        }
        forward_1d(&mut column);
        for y in 0..BLOCK {
            values[y * BLOCK + x] = column[y];
        }
    }
}

fn inverse_2d(values: &mut [i64; COEFFICIENTS]) {
    for x in 0..BLOCK {
        let mut column = [0i64; BLOCK];
        for y in 0..BLOCK {
            column[y] = values[y * BLOCK + x];
        }
        inverse_1d(&mut column);
        for y in 0..BLOCK {
            values[y * BLOCK + x] = column[y];
        }
    }
    for row in values.chunks_exact_mut(BLOCK) {
        let row: &mut [i64; BLOCK] = row.try_into().expect("eight-wide row");
        inverse_1d(row);
    }
}

fn forward_1d(values: &mut [i64; BLOCK]) {
    let mut active = BLOCK;
    let mut temporary = [0i64; BLOCK];
    while active > 1 {
        let half = active / 2;
        for index in 0..half {
            let a = values[index * 2];
            let b = values[index * 2 + 1];
            let high = a - b;
            temporary[index] = b + (high >> 1);
            temporary[half + index] = high;
        }
        values[..active].copy_from_slice(&temporary[..active]);
        active = half;
    }
}

fn inverse_1d(values: &mut [i64; BLOCK]) {
    let mut active = 2;
    let mut temporary = [0i64; BLOCK];
    while active <= BLOCK {
        let half = active / 2;
        for index in 0..half {
            let low = values[index];
            let high = values[half + index];
            let b = low - (high >> 1);
            temporary[index * 2] = high + b;
            temporary[index * 2 + 1] = b;
        }
        values[..active].copy_from_slice(&temporary[..active]);
        active *= 2;
    }
}

fn reconstructed_byte(value: i64, exact: bool) -> Result<u8, FrequencyError> {
    if exact {
        u8::try_from(value).map_err(|_| FrequencyError::ReconstructedOutOfRange(value))
    } else {
        Ok(value.clamp(0, 255) as u8)
    }
}

/// Invalid profile, geometry, coefficient stream, or caller storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrequencyError {
    UnexpectedCoding(CodingId),
    UnsupportedRevision {
        coding: CodingId,
        revision: u16,
    },
    UnexpectedParameters(CodingId),
    InvalidQuality(u8),
    UnsupportedLayout {
        layout: SampleLayout,
        plane_index: u8,
    },
    SizeOverflow,
    SampleLengthMismatch {
        expected: usize,
        actual: usize,
    },
    OutputTooSmall {
        needed: usize,
        available: usize,
    },
    Truncated {
        offset: usize,
    },
    CoefficientRunOverflow {
        remaining: usize,
        count: usize,
    },
    LiteralZero,
    NonCanonicalVarint,
    VarintOverflow,
    CoefficientOverflow,
    ReconstructedOutOfRange(i64),
    TrailingData {
        offset: usize,
    },
}

impl From<BufferError> for FrequencyError {
    fn from(error: BufferError) -> Self {
        match error {
            BufferError::SizeOverflow => Self::SizeOverflow,
            BufferError::Truncated { offset } => Self::Truncated { offset },
        }
    }
}
