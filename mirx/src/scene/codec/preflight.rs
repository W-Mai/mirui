use core::{mem::size_of, str};

use super::{
    CheckedDecodeAllocator, CodecError, DEFAULT_SCALE, DecodeAllocator, FIELD_ALPHA,
    FIELD_COMPOSITE, FIELD_QUAD, FIELD_RADIUS, FIELD_TRANSFORM, PAINT_KIND_COLOR,
    PAINT_KIND_LINEAR, PAINT_KIND_RADIAL, RES_KIND_INDEX, RES_KIND_INLINE, RES_KIND_TOKEN,
    SLOT_CLIP, SLOT_FILTER, SLOT_MASK, SLOT_OPACITY, SLOT_PROJECTIVE, SLOT_TRANSFORM, TAG_ARC,
    TAG_BLIT, TAG_BORDER, TAG_EOF, TAG_FILL_PATH, TAG_FILL_RECT, TAG_GLYPH_RUN, TAG_GROUP_BEGIN,
    TAG_GROUP_END, TAG_LINE, TAG_POP_CLIP, TAG_POSED_GLYPH_RUN, TAG_PUSH_CLIP, TAG_STROKE_PATH,
    VERSION, composite_from_u8, decode_body_with, fill_rule_from_u8, line_cap_from_u8,
    line_join_from_u8, spread_from_u8, units_from_u8,
};
use crate::path::{Path, PathCmd};
use crate::reader::PayloadLimits;
use crate::scene::{
    GlyphPlacement, GlyphPose, GradientStop, Paint, ResourceRef, Scene, SceneOp, VectorChunkHeader,
};
use crate::types::Fixed;

/// Failure while validating or reading a MIRX VECTOR payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VectorReadError {
    /// The shipped VECTOR codec rejected a wire value or structural relation.
    Codec(CodecError),
    /// The first end marker did not coincide with the end of the payload.
    PayloadLengthMismatch { expected: usize, actual: usize },
    /// VECTOR records exceeded the configured scene-operation scan budget.
    TooManySceneOps { count: u32, limit: u32 },
    /// Path commands exceeded the aggregate per-payload budget.
    TooManyPathCommands { count: u32, limit: u32 },
    /// Gradient stops exceeded the aggregate per-payload budget.
    TooManyGradientStops { count: u32, limit: u32 },
    /// Stroke dash elements exceeded the aggregate per-payload budget.
    TooManyDashElements { count: u32, limit: u32 },
    /// Positioned glyphs exceeded the aggregate per-payload budget.
    TooManyPositionedGlyphs { count: u32, limit: u32 },
    /// Owned token and label bytes exceeded the aggregate per-payload budget.
    StringBytesLimitExceeded { needed: usize, limit: usize },
    /// Owned scene components exceeded the aggregate decoded-byte budget.
    DecodedBytesLimitExceeded { needed: usize, limit: usize },
    /// An owned VECTOR component could not reserve its exact decoded capacity.
    AllocationFailed,
    /// A count, byte length, or decoded-size calculation overflowed.
    SizeOverflow,
}

impl From<CodecError> for VectorReadError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl Scene {
    /// Validates one complete VECTOR payload without allocating.
    ///
    /// The scan checks the existing wire contract, internal CRC, group and skip
    /// structure, UTF-8 strings, exact payload end, and every configured
    /// aggregate resource budget.
    pub fn preflight(payload: &[u8], limits: &PayloadLimits) -> Result<(), VectorReadError> {
        validate_payload(payload, limits)
    }

    /// Validates and decodes one complete VECTOR payload with bounded allocation.
    ///
    /// Complete wire and budget validation finishes before any owned scene
    /// component reserves memory.
    pub fn decode_with_limits(
        payload: &[u8],
        limits: &PayloadLimits,
    ) -> Result<Self, VectorReadError> {
        decode_payload(payload, limits)
    }

    pub(crate) fn validate_limits(&self, limits: &PayloadLimits) -> Result<(), VectorReadError> {
        validate_scene_limits(self, limits)
    }
}

pub(super) fn validate_payload(
    payload: &[u8],
    limits: &PayloadLimits,
) -> Result<(), VectorReadError> {
    validate_payload_layout(payload, limits).map(|_| ())
}

struct ValidatedVectorPayload<'a> {
    body: &'a [u8],
    decoded_ops: usize,
}

fn validate_payload_layout<'a>(
    payload: &'a [u8],
    limits: &PayloadLimits,
) -> Result<ValidatedVectorPayload<'a>, VectorReadError> {
    let mut header = Cursor::new(payload);
    if header.u8()? != VectorChunkHeader::MAGIC {
        return Err(CodecError::BadMagic.into());
    }
    let version = header.u8()?;
    if version != VERSION {
        return Err(CodecError::UnknownVersion(version).into());
    }
    let scale = header.u8()?;
    if scale != DEFAULT_SCALE {
        return Err(CodecError::UnsupportedScale(scale).into());
    }
    let flags = header.u8()?;
    if flags != 0 {
        return Err(CodecError::UnknownFlags(flags).into());
    }
    let stored_crc = header.u32()?;

    let body = &payload[VectorChunkHeader::SIZE..];
    let actual_crc = crate::crc32::compute(body);
    if stored_crc != actual_crc {
        return Err(CodecError::CrcMismatch {
            expected: stored_crc,
            actual: actual_crc,
        }
        .into());
    }

    let budget = Scanner::new(body, *limits).scan(payload.len())?;
    let decoded_ops =
        usize::try_from(budget.decoded_ops).map_err(|_| VectorReadError::SizeOverflow)?;
    Ok(ValidatedVectorPayload { body, decoded_ops })
}

fn decode_payload(payload: &[u8], limits: &PayloadLimits) -> Result<Scene, VectorReadError> {
    decode_payload_with_allocator(payload, limits, &mut CheckedDecodeAllocator)
}

fn decode_payload_with_allocator<A>(
    payload: &[u8],
    limits: &PayloadLimits,
    allocator: &mut A,
) -> Result<Scene, VectorReadError>
where
    A: DecodeAllocator<Error = VectorReadError>,
{
    let validated = validate_payload_layout(payload, limits)?;
    decode_body_with(validated.body, Some(validated.decoded_ops), allocator)
}

#[derive(Clone, Copy)]
enum ItemKind {
    PathCommand,
    GradientStop,
    DashElement,
    PositionedGlyph,
}

struct Budget {
    limits: PayloadLimits,
    wire_ops: u32,
    decoded_ops: u32,
    path_commands: u32,
    gradient_stops: u32,
    dash_elements: u32,
    positioned_glyphs: u32,
    string_bytes: usize,
    decoded_bytes: usize,
}

impl Budget {
    fn new(limits: PayloadLimits) -> Self {
        Self {
            limits,
            wire_ops: 0,
            decoded_ops: 0,
            path_commands: 0,
            gradient_stops: 0,
            dash_elements: 0,
            positioned_glyphs: 0,
            string_bytes: 0,
            decoded_bytes: 0,
        }
    }

    fn add_wire_op(&mut self) -> Result<(), VectorReadError> {
        let count = self
            .wire_ops
            .checked_add(1)
            .ok_or(VectorReadError::SizeOverflow)?;
        let limit = self.limits.max_scene_ops();
        if count > limit {
            return Err(VectorReadError::TooManySceneOps { count, limit });
        }
        self.wire_ops = count;
        Ok(())
    }

    fn add_decoded_op(&mut self) -> Result<(), VectorReadError> {
        let count = self
            .decoded_ops
            .checked_add(1)
            .ok_or(VectorReadError::SizeOverflow)?;
        self.add_decoded_bytes(size_of::<SceneOp>())?;
        self.decoded_ops = count;
        Ok(())
    }

    fn add_items(
        &mut self,
        kind: ItemKind,
        count: u32,
        item_size: usize,
    ) -> Result<(), VectorReadError> {
        let (current, limit) = match kind {
            ItemKind::PathCommand => (self.path_commands, self.limits.max_path_commands()),
            ItemKind::GradientStop => (self.gradient_stops, self.limits.max_gradient_stops()),
            ItemKind::DashElement => (self.dash_elements, self.limits.max_dash_elements()),
            ItemKind::PositionedGlyph => {
                (self.positioned_glyphs, self.limits.max_positioned_glyphs())
            }
        };
        let aggregate = current
            .checked_add(count)
            .ok_or(VectorReadError::SizeOverflow)?;
        if aggregate > limit {
            return Err(match kind {
                ItemKind::PathCommand => VectorReadError::TooManyPathCommands {
                    count: aggregate,
                    limit,
                },
                ItemKind::GradientStop => VectorReadError::TooManyGradientStops {
                    count: aggregate,
                    limit,
                },
                ItemKind::DashElement => VectorReadError::TooManyDashElements {
                    count: aggregate,
                    limit,
                },
                ItemKind::PositionedGlyph => VectorReadError::TooManyPositionedGlyphs {
                    count: aggregate,
                    limit,
                },
            });
        }
        match kind {
            ItemKind::PathCommand => self.path_commands = aggregate,
            ItemKind::GradientStop => self.gradient_stops = aggregate,
            ItemKind::DashElement => self.dash_elements = aggregate,
            ItemKind::PositionedGlyph => self.positioned_glyphs = aggregate,
        }

        let count = usize::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
        let bytes = count
            .checked_mul(item_size)
            .ok_or(VectorReadError::SizeOverflow)?;
        self.add_decoded_bytes(bytes)
    }

    fn add_string_bytes(&mut self, bytes: usize) -> Result<(), VectorReadError> {
        let needed = self
            .string_bytes
            .checked_add(bytes)
            .ok_or(VectorReadError::SizeOverflow)?;
        let limit = self.limits.max_string_bytes();
        if needed > limit {
            return Err(VectorReadError::StringBytesLimitExceeded { needed, limit });
        }
        self.string_bytes = needed;
        self.add_decoded_bytes(bytes)
    }

    fn add_decoded_bytes(&mut self, bytes: usize) -> Result<(), VectorReadError> {
        let needed = self
            .decoded_bytes
            .checked_add(bytes)
            .ok_or(VectorReadError::SizeOverflow)?;
        let limit = self.limits.max_decoded_bytes();
        if needed > limit {
            return Err(VectorReadError::DecodedBytesLimitExceeded { needed, limit });
        }
        self.decoded_bytes = needed;
        Ok(())
    }
}

fn validate_scene_limits(scene: &Scene, limits: &PayloadLimits) -> Result<(), VectorReadError> {
    let mut budget = Budget::new(*limits);
    for op in &scene.ops {
        budget.add_wire_op()?;
        budget.add_decoded_op()?;
        match op {
            SceneOp::GroupBegin {
                clip, mask, filter, ..
            } => {
                if let Some(resource) = clip {
                    add_resource_ref(&mut budget, resource)?;
                }
                if let Some(resource) = mask {
                    add_resource_ref(&mut budget, resource)?;
                }
                if let Some(resource) = filter {
                    add_resource_ref(&mut budget, resource)?;
                }
            }
            SceneOp::FillPath { path, paint, .. } => {
                add_path(&mut budget, path)?;
                add_paint(&mut budget, paint)?;
            }
            SceneOp::StrokePath {
                path, paint, dash, ..
            } => {
                add_path(&mut budget, path)?;
                add_paint(&mut budget, paint)?;
                add_items(
                    &mut budget,
                    ItemKind::DashElement,
                    dash.len(),
                    size_of::<Fixed>(),
                )?;
            }
            SceneOp::PushClip { path, .. } => add_path(&mut budget, path)?,
            SceneOp::GlyphRun {
                font, ppem, glyphs, ..
            } => {
                if *ppem == 0 {
                    return Err(CodecError::InvalidPpem.into());
                }
                add_resource_ref(&mut budget, font)?;
                add_items(
                    &mut budget,
                    ItemKind::PositionedGlyph,
                    glyphs.len(),
                    size_of::<GlyphPlacement>(),
                )?;
            }
            SceneOp::PosedGlyphRun {
                font, ppem, glyphs, ..
            } => {
                if *ppem == 0 {
                    return Err(CodecError::InvalidPpem.into());
                }
                if glyphs.iter().any(|glyph| !glyph.has_unit_tangent()) {
                    return Err(CodecError::InvalidGlyphDirection.into());
                }
                add_resource_ref(&mut budget, font)?;
                add_items(
                    &mut budget,
                    ItemKind::PositionedGlyph,
                    glyphs.len(),
                    size_of::<GlyphPose>(),
                )?;
            }
            SceneOp::Blit { texture, .. } => add_resource_ref(&mut budget, texture)?,
            SceneOp::GroupEnd
            | SceneOp::PopClip
            | SceneOp::FillRect { .. }
            | SceneOp::Border { .. }
            | SceneOp::Line { .. }
            | SceneOp::Arc { .. } => {}
        }
    }
    Ok(())
}

fn add_resource_ref(budget: &mut Budget, resource: &ResourceRef) -> Result<(), VectorReadError> {
    match resource {
        ResourceRef::Token(token) => budget.add_string_bytes(token.len()),
        ResourceRef::Index(_) => Ok(()),
        ResourceRef::Inline(path) => add_path(budget, path),
    }
}

fn add_path(budget: &mut Budget, path: &Path) -> Result<(), VectorReadError> {
    add_items(
        budget,
        ItemKind::PathCommand,
        path.cmds.len(),
        size_of::<PathCmd>(),
    )
}

fn add_paint(budget: &mut Budget, paint: &Paint) -> Result<(), VectorReadError> {
    let stops = match paint {
        Paint::Color(_) => return Ok(()),
        Paint::LinearGradient(gradient) => &gradient.stops,
        Paint::RadialGradient(gradient) => &gradient.stops,
    };
    if !GradientStop::sequence_is_valid(stops) {
        return Err(CodecError::InvalidGradientStops.into());
    }
    add_items(
        budget,
        ItemKind::GradientStop,
        stops.len(),
        size_of::<GradientStop>(),
    )
}

fn add_items(
    budget: &mut Budget,
    kind: ItemKind,
    count: usize,
    item_size: usize,
) -> Result<(), VectorReadError> {
    let count = u32::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
    budget.add_items(kind, count, item_size)
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], VectorReadError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(VectorReadError::SizeOverflow)?;
        if end > self.buf.len() {
            return Err(CodecError::UnexpectedEof.into());
        }
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn ensure_remaining(&self, len: usize) -> Result<(), VectorReadError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(VectorReadError::SizeOverflow)?;
        if end > self.buf.len() {
            return Err(CodecError::UnexpectedEof.into());
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, VectorReadError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, VectorReadError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, VectorReadError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn varuint(&mut self) -> Result<u32, VectorReadError> {
        let mut value = 0u32;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            if shift == 28 && byte & 0xf0 != 0 {
                return Err(VectorReadError::SizeOverflow);
            }
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 32 {
                return Err(CodecError::UnexpectedEof.into());
            }
        }
    }

    fn byte_at(&self, index: usize) -> Option<u8> {
        self.buf.get(index).copied()
    }
}

struct Scanner<'a> {
    cursor: Cursor<'a>,
    budget: Budget,
    depth: u32,
}

impl<'a> Scanner<'a> {
    fn new(body: &'a [u8], limits: PayloadLimits) -> Self {
        Self {
            cursor: Cursor::new(body),
            budget: Budget::new(limits),
            depth: 0,
        }
    }

    fn scan(mut self, payload_len: usize) -> Result<Budget, VectorReadError> {
        loop {
            let tag_pos = self.cursor.pos;
            let tag = self.cursor.u8()?;
            match tag {
                TAG_EOF => {
                    if self.depth != 0 {
                        return Err(CodecError::UnbalancedGroup.into());
                    }
                    let expected = VectorChunkHeader::SIZE
                        .checked_add(self.cursor.pos)
                        .ok_or(VectorReadError::SizeOverflow)?;
                    if expected != payload_len {
                        return Err(VectorReadError::PayloadLengthMismatch {
                            expected,
                            actual: payload_len,
                        });
                    }
                    return Ok(self.budget);
                }
                TAG_GROUP_BEGIN => {
                    self.begin_decoded_op()?;
                    self.scan_group_begin(tag_pos)?;
                }
                TAG_GROUP_END => {
                    if self.depth == 0 {
                        return Err(CodecError::UnbalancedGroup.into());
                    }
                    self.begin_decoded_op()?;
                    self.depth -= 1;
                }
                TAG_FILL_PATH | TAG_STROKE_PATH | TAG_PUSH_CLIP | TAG_POP_CLIP | TAG_FILL_RECT
                | TAG_BORDER | TAG_GLYPH_RUN | TAG_POSED_GLYPH_RUN | TAG_LINE | TAG_ARC
                | TAG_BLIT => {
                    self.begin_decoded_op()?;
                    self.scan_op(tag)?;
                }
                0x40..=0x7f => {
                    self.budget.add_wire_op()?;
                    let len = self.cursor.varuint()?;
                    let len = usize::try_from(len).map_err(|_| VectorReadError::SizeOverflow)?;
                    let _ = self.cursor.take(len)?;
                }
                other => return Err(CodecError::UnknownTag(other).into()),
            }
        }
    }

    fn begin_decoded_op(&mut self) -> Result<(), VectorReadError> {
        self.budget.add_wire_op()?;
        self.budget.add_decoded_op()
    }

    fn scan_group_begin(&mut self, tag_pos: usize) -> Result<(), VectorReadError> {
        let bits = self.cursor.varuint()?;
        let target =
            usize::try_from(self.cursor.u32()?).map_err(|_| VectorReadError::SizeOverflow)?;
        if target <= tag_pos
            || target > self.cursor.buf.len()
            || self.cursor.byte_at(target - 1) != Some(TAG_GROUP_END)
        {
            return Err(CodecError::BadSkipOffset.into());
        }
        if bits & SLOT_TRANSFORM != 0 {
            self.skip_transform()?;
        }
        if bits & SLOT_PROJECTIVE != 0 {
            self.cursor.take(72)?;
        }
        if bits & SLOT_OPACITY != 0 {
            let _ = self.cursor.u8()?;
        }
        if bits & SLOT_CLIP != 0 {
            self.scan_resource_ref()?;
        }
        if bits & SLOT_MASK != 0 {
            self.scan_resource_ref()?;
        }
        if bits & SLOT_FILTER != 0 {
            self.scan_resource_ref()?;
        }
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or(VectorReadError::SizeOverflow)?;
        Ok(())
    }

    fn scan_op(&mut self, tag: u8) -> Result<(), VectorReadError> {
        match tag {
            TAG_FILL_PATH => {
                let bits = self.cursor.u8()?;
                self.scan_path()?;
                self.scan_paint()?;
                let _ = self.cursor.u8()?;
                let _ = fill_rule_from_u8(self.cursor.u8()?)?;
                self.skip_transform_if(bits)
            }
            TAG_STROKE_PATH => {
                let bits = self.cursor.u8()?;
                self.scan_path()?;
                self.scan_paint()?;
                let _ = self.cursor.take(4)?;
                let _ = self.cursor.u8()?;
                let _ = line_cap_from_u8(self.cursor.u8()?)?;
                let _ = line_join_from_u8(self.cursor.u8()?)?;
                let _ = self.cursor.take(4)?;
                self.scan_dash_elements()?;
                self.skip_transform_if(bits)
            }
            TAG_PUSH_CLIP => {
                let bits = self.cursor.u8()?;
                self.scan_path()?;
                let _ = fill_rule_from_u8(self.cursor.u8()?)?;
                self.skip_transform_if(bits)
            }
            TAG_POP_CLIP => Ok(()),
            TAG_FILL_RECT => {
                let bits = self.cursor.u8()?;
                let _ = self.cursor.take(16 + 4 + 1)?;
                self.skip_optional(bits)
            }
            TAG_BORDER => {
                let bits = self.cursor.u8()?;
                let _ = self.cursor.take(16 + 4 + 4 + 1)?;
                self.skip_optional(bits)
            }
            TAG_GLYPH_RUN | TAG_POSED_GLYPH_RUN => {
                let bits = self.cursor.u8()?;
                self.scan_resource_ref()?;
                let ppem = self.cursor.u16()?;
                if ppem == 0 {
                    return Err(CodecError::InvalidPpem.into());
                }
                let _ = self.cursor.take(8 + 4 + 1)?;
                let count = self.cursor.varuint()?;
                let count_usize =
                    usize::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
                if tag == TAG_POSED_GLYPH_RUN {
                    self.cursor.ensure_remaining(
                        count_usize
                            .checked_mul(18)
                            .ok_or(VectorReadError::SizeOverflow)?,
                    )?;
                    for _ in 0..count_usize {
                        let record = self.cursor.take(18)?;
                        let tangent = GlyphPose::new(
                            0,
                            crate::types::Point::ZERO,
                            crate::types::Point::new(
                                Fixed::from_le_bytes(record[10..14].try_into().unwrap()),
                                Fixed::from_le_bytes(record[14..18].try_into().unwrap()),
                            ),
                        );
                        if !tangent.has_unit_tangent() {
                            return Err(CodecError::InvalidGlyphDirection.into());
                        }
                    }
                } else {
                    let bytes = count_usize
                        .checked_mul(18)
                        .ok_or(VectorReadError::SizeOverflow)?;
                    let _ = self.cursor.take(bytes)?;
                }
                self.budget.add_items(
                    ItemKind::PositionedGlyph,
                    count,
                    if tag == TAG_GLYPH_RUN {
                        size_of::<GlyphPlacement>()
                    } else {
                        size_of::<GlyphPose>()
                    },
                )?;
                self.skip_transform_if(bits)
            }
            TAG_LINE => {
                let bits = self.cursor.u8()?;
                let _ = self.cursor.take(8 + 8 + 4 + 4 + 1)?;
                self.skip_transform_if(bits)
            }
            TAG_ARC => {
                let bits = self.cursor.u8()?;
                let _ = self.cursor.take(8 + 4 + 4 + 4 + 4 + 4 + 1)?;
                self.skip_transform_if(bits)
            }
            TAG_BLIT => {
                let bits = self.cursor.u8()?;
                self.scan_resource_ref()?;
                let _ = self.cursor.take(8 + 8)?;
                self.skip_optional(bits)?;
                if bits & FIELD_ALPHA != 0 {
                    let _ = self.cursor.u8()?;
                }
                if bits & FIELD_COMPOSITE != 0 {
                    let _ = composite_from_u8(self.cursor.u8()?)?;
                }
                Ok(())
            }
            _ => Err(CodecError::UnknownTag(tag).into()),
        }
    }

    fn scan_resource_ref(&mut self) -> Result<(), VectorReadError> {
        match self.cursor.u8()? {
            RES_KIND_INDEX => {
                let _ = self.cursor.take(4)?;
                Ok(())
            }
            RES_KIND_TOKEN => self.scan_string(),
            RES_KIND_INLINE => self.scan_path(),
            other => Err(CodecError::BadResourceKind(other).into()),
        }
    }

    fn scan_string(&mut self) -> Result<(), VectorReadError> {
        let len = self.cursor.varuint()?;
        let len = usize::try_from(len).map_err(|_| VectorReadError::SizeOverflow)?;
        let bytes = self.cursor.take(len)?;
        self.budget.add_string_bytes(len)?;
        str::from_utf8(bytes)
            .map(|_| ())
            .map_err(|_| CodecError::BadUtf8.into())
    }

    fn scan_path(&mut self) -> Result<(), VectorReadError> {
        let count = self.cursor.varuint()?;
        let minimum = usize::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
        self.cursor.ensure_remaining(minimum)?;
        self.budget
            .add_items(ItemKind::PathCommand, count, size_of::<PathCmd>())?;
        for _ in 0..count {
            let bytes = match self.cursor.u8()? {
                0 | 1 => 8,
                2 => 16,
                3 => 24,
                4 => 0,
                other => return Err(CodecError::UnknownTag(other).into()),
            };
            let _ = self.cursor.take(bytes)?;
        }
        Ok(())
    }

    fn scan_paint(&mut self) -> Result<(), VectorReadError> {
        match self.cursor.u8()? {
            PAINT_KIND_COLOR => {
                let _ = self.cursor.take(4)?;
                Ok(())
            }
            PAINT_KIND_LINEAR => {
                let _ = self.cursor.take(8 + 8)?;
                self.scan_gradient_stops()?;
                let _ = spread_from_u8(self.cursor.u8()?)?;
                let _ = units_from_u8(self.cursor.u8()?)?;
                self.skip_transform()
            }
            PAINT_KIND_RADIAL => {
                let _ = self.cursor.take(8 + 4 + 8 + 4)?;
                self.scan_gradient_stops()?;
                let _ = spread_from_u8(self.cursor.u8()?)?;
                let _ = units_from_u8(self.cursor.u8()?)?;
                self.skip_transform()
            }
            other => Err(CodecError::UnknownTag(other).into()),
        }
    }

    fn scan_gradient_stops(&mut self) -> Result<(), VectorReadError> {
        let count = self.cursor.u32()?;
        let count_usize = usize::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
        let bytes = count_usize
            .checked_mul(8)
            .ok_or(VectorReadError::SizeOverflow)?;
        let encoded = self.cursor.take(bytes)?;
        if count == 0 {
            return Err(CodecError::InvalidGradientStops.into());
        }
        let mut previous = Fixed::ZERO;
        for stop in encoded.chunks_exact(8) {
            let offset = Fixed::from_le_bytes([stop[0], stop[1], stop[2], stop[3]]);
            if !GradientStop::offset_follows(previous, offset) {
                return Err(CodecError::InvalidGradientStops.into());
            }
            previous = offset;
        }
        self.budget
            .add_items(ItemKind::GradientStop, count, size_of::<GradientStop>())
    }

    fn scan_dash_elements(&mut self) -> Result<(), VectorReadError> {
        let count = self.cursor.u32()?;
        let count_usize = usize::try_from(count).map_err(|_| VectorReadError::SizeOverflow)?;
        let bytes = count_usize
            .checked_mul(4)
            .ok_or(VectorReadError::SizeOverflow)?;
        let _ = self.cursor.take(bytes)?;
        self.budget
            .add_items(ItemKind::DashElement, count, size_of::<Fixed>())
    }

    fn skip_transform_if(&mut self, bits: u8) -> Result<(), VectorReadError> {
        if bits & FIELD_TRANSFORM != 0 {
            self.skip_transform()?;
        }
        Ok(())
    }

    fn skip_transform(&mut self) -> Result<(), VectorReadError> {
        let _ = self.cursor.take(24)?;
        Ok(())
    }

    fn skip_optional(&mut self, bits: u8) -> Result<(), VectorReadError> {
        if bits & FIELD_TRANSFORM != 0 {
            self.skip_transform()?;
        }
        if bits & FIELD_QUAD != 0 {
            let _ = self.cursor.take(32)?;
        }
        if bits & FIELD_RADIUS != 0 {
            let _ = self.cursor.take(4)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::path::Path;
    use crate::scene::{
        CompositeMode, FillRule, GradientUnits, LinearGradient, Paint, RadialGradient, ResourceRef,
        SpreadMode,
    };
    use crate::types::{Color, Point, Rect, Transform};

    const OP_COUNT: u32 = 12;
    const PATH_COUNT: u32 = 6;
    const STOP_COUNT: u32 = 3;
    const DASH_COUNT: u32 = 2;
    const GLYPH_COUNT: u32 = 2;
    const STRING_BYTES: usize = 1;

    fn fixed(value: i32) -> Fixed {
        Fixed::from_int(value)
    }

    fn point(x: i32, y: i32) -> Point {
        Point {
            x: fixed(x),
            y: fixed(y),
        }
    }

    fn color(value: u8) -> Color {
        Color {
            r: value,
            g: value.wrapping_add(1),
            b: value.wrapping_add(2),
            a: 255,
        }
    }

    fn one_command_path() -> Path {
        Path::from_cmds(vec![PathCmd::Close])
    }

    fn representative_scene() -> Scene {
        let transform = Transform::translate(fixed(1), fixed(2));
        let quad = Some([point(0, 0), point(2, 0), point(2, 2), point(0, 2)]);
        Scene::from_ops(vec![
            SceneOp::GroupBegin {
                transform: Some(transform),
                projective: None,
                opacity: Some(200),
                clip: Some(ResourceRef::Token(String::from("c"))),
                mask: Some(ResourceRef::Inline(one_command_path())),
                filter: Some(ResourceRef::Index(7)),
                disjoint_hint: true,
            },
            SceneOp::PushClip {
                path: one_command_path(),
                transform,
                fill_rule: FillRule::EvenOdd,
            },
            SceneOp::PopClip,
            SceneOp::FillPath {
                path: Path::from_cmds(vec![
                    PathCmd::MoveTo(point(0, 0)),
                    PathCmd::LineTo(point(3, 4)),
                ]),
                transform,
                paint: Paint::LinearGradient(LinearGradient {
                    start: point(0, 0),
                    end: point(4, 4),
                    stops: Cow::Owned(vec![
                        GradientStop {
                            offset: Fixed::ZERO,
                            color: color(1),
                        },
                        GradientStop {
                            offset: Fixed::ONE,
                            color: color(2),
                        },
                    ]),
                    spread: SpreadMode::Pad,
                    units: GradientUnits::UserSpaceOnUse,
                    transform,
                }),
                opa: 240,
                fill_rule: FillRule::NonZero,
            },
            SceneOp::StrokePath {
                path: one_command_path(),
                transform,
                paint: Paint::RadialGradient(RadialGradient {
                    center: point(2, 2),
                    radius: fixed(2),
                    focal: point(1, 1),
                    focal_radius: fixed(1),
                    stops: Cow::Owned(vec![GradientStop {
                        offset: Fixed::ZERO,
                        color: color(3),
                    }]),
                    spread: SpreadMode::Reflect,
                    units: GradientUnits::ObjectBoundingBox,
                    transform,
                }),
                width: fixed(1),
                opa: 230,
                line_cap: crate::scene::LineCap::Round,
                line_join: crate::scene::LineJoin::Bevel,
                miter_limit: fixed(4),
                dash: Cow::Owned(vec![fixed(1), fixed(2)]),
            },
            SceneOp::FillRect {
                area: Rect {
                    x: fixed(0),
                    y: fixed(0),
                    w: fixed(4),
                    h: fixed(3),
                },
                transform,
                quad,
                color: color(4),
                radius: fixed(1),
                opa: 220,
            },
            SceneOp::Border {
                area: Rect {
                    x: fixed(0),
                    y: fixed(0),
                    w: fixed(4),
                    h: fixed(3),
                },
                transform,
                quad,
                color: color(5),
                width: fixed(1),
                radius: fixed(1),
                opa: 210,
            },
            SceneOp::GlyphRun {
                font: ResourceRef::Index(3),
                ppem: 18,
                pos: point(1, 2),
                transform,
                color: color(7),
                opa: 195,
                glyphs: vec![
                    GlyphPlacement::new(42, point(0, 14)),
                    GlyphPlacement::new(43, point(9, 14)).with_offset(point(0, -1)),
                ],
            },
            SceneOp::Line {
                p1: point(0, 0),
                p2: point(3, 3),
                transform,
                color: color(7),
                width: fixed(1),
                opa: 190,
            },
            SceneOp::Arc {
                center: point(2, 2),
                transform,
                radius: fixed(2),
                start_angle: Fixed::ZERO,
                end_angle: fixed(1),
                color: color(8),
                width: fixed(1),
                opa: 180,
            },
            SceneOp::Blit {
                texture: ResourceRef::Inline(one_command_path()),
                pos: point(0, 0),
                size: point(2, 2),
                transform,
                quad,
                opa: 170,
                radius: fixed(1),
                composite: CompositeMode::Multiply,
            },
            SceneOp::GroupEnd,
        ])
    }

    fn decoded_bytes() -> usize {
        OP_COUNT as usize * size_of::<SceneOp>()
            + PATH_COUNT as usize * size_of::<PathCmd>()
            + STOP_COUNT as usize * size_of::<GradientStop>()
            + DASH_COUNT as usize * size_of::<Fixed>()
            + GLYPH_COUNT as usize * size_of::<GlyphPlacement>()
            + STRING_BYTES
    }

    fn exact_limits() -> PayloadLimits {
        PayloadLimits::HOST
            .with_max_scene_ops(OP_COUNT)
            .with_max_path_commands(PATH_COUNT)
            .with_max_gradient_stops(STOP_COUNT)
            .with_max_dash_elements(DASH_COUNT)
            .with_max_positioned_glyphs(GLYPH_COUNT)
            .with_max_string_bytes(STRING_BYTES)
            .with_max_decoded_bytes(decoded_bytes())
    }

    fn payload_from_body(body: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(VectorChunkHeader::SIZE + body.len());
        payload.push(VectorChunkHeader::MAGIC);
        payload.push(VERSION);
        payload.push(DEFAULT_SCALE);
        payload.push(0);
        payload.extend_from_slice(&crate::crc32::compute(body).to_le_bytes());
        payload.extend_from_slice(body);
        payload
    }

    fn refresh_payload_crc(payload: &mut [u8]) {
        let crc = crate::crc32::compute(&payload[VectorChunkHeader::SIZE..]);
        payload[4..8].copy_from_slice(&crc.to_le_bytes());
    }

    #[derive(Default)]
    struct TrackingAllocator {
        calls: usize,
        fail_at: Option<usize>,
        reserve_requests: Vec<usize>,
        string_requests: Vec<usize>,
    }

    impl TrackingAllocator {
        fn failing_at(call: usize) -> Self {
            Self {
                fail_at: Some(call),
                ..Self::default()
            }
        }

        fn begin_call(&mut self) -> Result<(), VectorReadError> {
            let call = self.calls;
            self.calls += 1;
            if self.fail_at == Some(call) {
                Err(VectorReadError::AllocationFailed)
            } else {
                Ok(())
            }
        }
    }

    impl DecodeAllocator for TrackingAllocator {
        type Error = VectorReadError;

        fn reserve<T>(
            &mut self,
            values: &mut Vec<T>,
            additional: usize,
        ) -> Result<(), Self::Error> {
            self.reserve_requests.push(additional);
            self.begin_call()?;
            values
                .try_reserve_exact(additional)
                .map_err(|_| VectorReadError::AllocationFailed)
        }

        fn copy_string(&mut self, value: &str) -> Result<String, Self::Error> {
            self.string_requests.push(value.len());
            self.begin_call()?;
            let mut string = String::new();
            string
                .try_reserve_exact(value.len())
                .map_err(|_| VectorReadError::AllocationFailed)?;
            string.push_str(value);
            Ok(string)
        }
    }

    #[test]
    fn accepts_every_shipped_op_and_exact_component_budgets() {
        let scene = representative_scene();
        let payload = scene.encode().unwrap();
        assert_eq!(Scene::preflight(&payload, &exact_limits()), Ok(()));
        assert_eq!(
            Scene::decode_with_limits(&payload, &exact_limits()),
            Ok(scene)
        );
    }

    #[test]
    fn typed_scene_validation_uses_the_preflight_budget_model() {
        let scene = representative_scene();
        let exact = exact_limits();
        assert_eq!(scene.validate_limits(&exact), Ok(()));
        assert_eq!(
            scene.validate_limits(&exact.with_max_scene_ops(OP_COUNT - 1)),
            Err(VectorReadError::TooManySceneOps {
                count: OP_COUNT,
                limit: OP_COUNT - 1,
            })
        );
        assert_eq!(
            scene.validate_limits(&exact.with_max_path_commands(PATH_COUNT - 1)),
            Err(VectorReadError::TooManyPathCommands {
                count: PATH_COUNT,
                limit: PATH_COUNT - 1,
            })
        );
        assert_eq!(
            scene.validate_limits(&exact.with_max_gradient_stops(STOP_COUNT - 1)),
            Err(VectorReadError::TooManyGradientStops {
                count: STOP_COUNT,
                limit: STOP_COUNT - 1,
            })
        );
        assert_eq!(
            scene.validate_limits(&exact.with_max_dash_elements(DASH_COUNT - 1)),
            Err(VectorReadError::TooManyDashElements {
                count: DASH_COUNT,
                limit: DASH_COUNT - 1,
            })
        );
        assert_eq!(
            scene.validate_limits(&exact.with_max_string_bytes(STRING_BYTES - 1)),
            Err(VectorReadError::StringBytesLimitExceeded {
                needed: STRING_BYTES,
                limit: STRING_BYTES - 1,
            })
        );
        assert_eq!(
            scene.validate_limits(&exact.with_max_decoded_bytes(decoded_bytes() - 1)),
            Err(VectorReadError::DecodedBytesLimitExceeded {
                needed: decoded_bytes(),
                limit: decoded_bytes() - 1,
            })
        );
    }

    #[test]
    fn bounded_decode_reports_each_owned_reserve_failure() {
        let scene = representative_scene();
        let payload = scene.encode().unwrap();
        let mut successful = TrackingAllocator::default();
        assert_eq!(
            decode_payload_with_allocator(&payload, &exact_limits(), &mut successful),
            Ok(scene)
        );
        assert!(successful.calls > 1);

        for fail_at in 0..successful.calls {
            let mut failing = TrackingAllocator::failing_at(fail_at);
            assert_eq!(
                decode_payload_with_allocator(&payload, &exact_limits(), &mut failing),
                Err(VectorReadError::AllocationFailed),
                "allocation call {fail_at} did not fail"
            );
            assert_eq!(failing.calls, fail_at + 1);
        }
    }

    #[test]
    fn bounded_decode_completes_preflight_before_the_first_reserve() {
        let payload = representative_scene().encode().unwrap();
        let limits = exact_limits().with_max_scene_ops(OP_COUNT - 1);
        let mut allocator = TrackingAllocator::failing_at(0);
        assert_eq!(
            decode_payload_with_allocator(&payload, &limits, &mut allocator),
            Err(VectorReadError::TooManySceneOps {
                count: OP_COUNT,
                limit: OP_COUNT - 1,
            })
        );
        assert_eq!(allocator.calls, 0);
    }

    #[test]
    fn enforces_every_aggregate_budget() {
        let payload = representative_scene().encode().unwrap();
        let exact = exact_limits();

        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_scene_ops(OP_COUNT - 1)),
            Err(VectorReadError::TooManySceneOps {
                count: OP_COUNT,
                limit: OP_COUNT - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_path_commands(PATH_COUNT - 1),),
            Err(VectorReadError::TooManyPathCommands {
                count: PATH_COUNT,
                limit: PATH_COUNT - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_gradient_stops(STOP_COUNT - 1),),
            Err(VectorReadError::TooManyGradientStops {
                count: STOP_COUNT,
                limit: STOP_COUNT - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_dash_elements(DASH_COUNT - 1),),
            Err(VectorReadError::TooManyDashElements {
                count: DASH_COUNT,
                limit: DASH_COUNT - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_positioned_glyphs(GLYPH_COUNT - 1),),
            Err(VectorReadError::TooManyPositionedGlyphs {
                count: GLYPH_COUNT,
                limit: GLYPH_COUNT - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_string_bytes(STRING_BYTES - 1),),
            Err(VectorReadError::StringBytesLimitExceeded {
                needed: STRING_BYTES,
                limit: STRING_BYTES - 1,
            })
        );
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_decoded_bytes(decoded_bytes() - 1),),
            Err(VectorReadError::DecodedBytesLimitExceeded {
                needed: decoded_bytes(),
                limit: decoded_bytes() - 1,
            })
        );
    }

    #[test]
    fn validates_fixed_header_crc_and_exact_payload_end() {
        let valid = payload_from_body(&[TAG_EOF]);
        let zero = PayloadLimits::HOST
            .with_max_scene_ops(0)
            .with_max_path_commands(0)
            .with_max_gradient_stops(0)
            .with_max_dash_elements(0)
            .with_max_string_bytes(0)
            .with_max_decoded_bytes(0);
        assert_eq!(Scene::preflight(&valid, &zero), Ok(()));
        for available in 0..VectorChunkHeader::SIZE {
            assert_eq!(
                Scene::preflight(&valid[..available], &PayloadLimits::HOST),
                Err(VectorReadError::Codec(CodecError::UnexpectedEof))
            );
        }

        let mut magic = valid.clone();
        magic[0] = 0xff;
        assert_eq!(
            Scene::preflight(&magic, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::BadMagic))
        );

        let mut version = valid.clone();
        version[1] = VERSION + 1;
        assert_eq!(
            Scene::preflight(&version, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnknownVersion(
                VERSION + 1
            )))
        );

        let mut scale = valid.clone();
        scale[2] = DEFAULT_SCALE + 1;
        assert_eq!(
            Scene::preflight(&scale, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnsupportedScale(
                DEFAULT_SCALE + 1
            )))
        );

        let mut flags = valid.clone();
        flags[3] = 1;
        assert_eq!(
            Scene::preflight(&flags, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnknownFlags(1)))
        );

        let mut crc = valid.clone();
        crc[8] = TAG_POP_CLIP;
        assert!(matches!(
            Scene::preflight(&crc, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::CrcMismatch { .. }))
        ));

        let trailing = payload_from_body(&[TAG_EOF, 0xaa]);
        assert_eq!(
            Scene::preflight(&trailing, &PayloadLimits::HOST),
            Err(VectorReadError::PayloadLengthMismatch {
                expected: VectorChunkHeader::SIZE + 1,
                actual: VectorChunkHeader::SIZE + 2,
            })
        );
        assert_eq!(Scene::decode(&trailing), Ok(Scene::default()));

        let empty_body = payload_from_body(&[]);
        assert_eq!(
            Scene::preflight(&empty_body, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnexpectedEof))
        );
    }

    #[test]
    fn bounds_extension_records_without_charging_decoded_bytes() {
        let payload = payload_from_body(&[0x40, 0, 0x7f, 1, 0xaa, TAG_EOF]);
        let exact = PayloadLimits::HOST
            .with_max_scene_ops(2)
            .with_max_decoded_bytes(0);
        assert_eq!(Scene::preflight(&payload, &exact), Ok(()));
        let mut allocator = TrackingAllocator::default();
        assert_eq!(
            decode_payload_with_allocator(&payload, &exact, &mut allocator),
            Ok(Scene::default())
        );
        assert_eq!(allocator.reserve_requests, vec![0]);
        assert!(allocator.string_requests.is_empty());
        assert_eq!(
            Scene::preflight(&payload, &exact.with_max_scene_ops(1)),
            Err(VectorReadError::TooManySceneOps { count: 2, limit: 1 })
        );

        let truncated = payload_from_body(&[0x40, 2, 0xaa]);
        assert_eq!(
            Scene::preflight(&truncated, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnexpectedEof))
        );
    }

    #[test]
    fn declared_lengths_must_fit_before_caller_limits_are_applied() {
        let mut path = vec![TAG_PUSH_CLIP, 0];
        super::super::write_varuint(&mut path, u32::MAX);
        let path = payload_from_body(&path);
        assert_eq!(
            Scene::preflight(&path, &PayloadLimits::HOST.with_max_path_commands(0),),
            Err(VectorReadError::Codec(CodecError::UnexpectedEof))
        );

        let mut token = vec![TAG_GLYPH_RUN, 0, RES_KIND_TOKEN];
        super::super::write_varuint(&mut token, u32::MAX);
        let token = payload_from_body(&token);
        assert_eq!(
            Scene::preflight(&token, &PayloadLimits::HOST.with_max_string_bytes(0)),
            Err(VectorReadError::Codec(CodecError::UnexpectedEof))
        );

        let overflow = payload_from_body(&[0x40, 0xff, 0xff, 0xff, 0xff, 0x10]);
        assert_eq!(
            Scene::preflight(&overflow, &PayloadLimits::HOST),
            Err(VectorReadError::SizeOverflow)
        );
    }

    #[test]
    fn validates_utf8_tags_groups_and_skip_offsets() {
        let invalid_token =
            payload_from_body(&[TAG_GLYPH_RUN, 0, RES_KIND_TOKEN, 1, 0xff, TAG_EOF]);
        assert_eq!(
            Scene::preflight(&invalid_token, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::BadUtf8))
        );

        let unknown = payload_from_body(&[0x3f, TAG_EOF]);
        assert_eq!(
            Scene::preflight(&unknown, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnknownTag(0x3f)))
        );

        let unbalanced = payload_from_body(&[TAG_GROUP_END, TAG_EOF]);
        assert_eq!(
            Scene::preflight(&unbalanced, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::UnbalancedGroup))
        );

        let mut bad_skip = vec![TAG_GROUP_BEGIN, 0];
        bad_skip.extend_from_slice(&2u32.to_le_bytes());
        bad_skip.push(TAG_EOF);
        let bad_skip = payload_from_body(&bad_skip);
        assert_eq!(
            Scene::preflight(&bad_skip, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::BadSkipOffset))
        );
    }

    #[test]
    fn every_truncated_complex_body_is_rejected_without_panicking() {
        let payload = representative_scene().encode().unwrap();
        let body = &payload[VectorChunkHeader::SIZE..];
        for available in 0..body.len() {
            let truncated = payload_from_body(&body[..available]);
            assert!(
                Scene::preflight(&truncated, &PayloadLimits::HOST).is_err(),
                "accepted body prefix of {available} bytes"
            );
        }
    }

    #[test]
    fn posed_run_preflight_rejects_truncation_and_non_unit_direction() {
        let scene = Scene::from_ops(vec![SceneOp::PosedGlyphRun {
            font: ResourceRef::Index(1),
            ppem: 16,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: color(1),
            opa: 255,
            glyphs: vec![GlyphPose::new(
                7,
                point(2, 3),
                Point::new(Fixed::ONE, Fixed::ZERO),
            )],
        }]);
        let payload = scene.encode().unwrap();
        let body = &payload[VectorChunkHeader::SIZE..];
        for available in 0..body.len() {
            let truncated = payload_from_body(&body[..available]);
            assert!(Scene::preflight(&truncated, &PayloadLimits::HOST).is_err());
        }

        let mut invalid = payload;
        invalid[VectorChunkHeader::SIZE + 33..VectorChunkHeader::SIZE + 41].fill(0);
        refresh_payload_crc(&mut invalid);
        assert_eq!(
            Scene::preflight(&invalid, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::InvalidGlyphDirection))
        );
    }

    #[test]
    fn retained_field_bits_and_nonminimal_varuint_match_the_legacy_codec() {
        let scene = Scene::from_ops(vec![SceneOp::FillRect {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::ONE,
                h: Fixed::ONE,
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: color(9),
            radius: Fixed::ZERO,
            opa: 255,
        }]);
        let mut unknown_field = scene.encode().unwrap();
        unknown_field[VectorChunkHeader::SIZE + 1] |= 0x80;
        refresh_payload_crc(&mut unknown_field);
        assert_eq!(
            Scene::preflight(&unknown_field, &PayloadLimits::HOST),
            Ok(())
        );
        assert!(Scene::decode(&unknown_field).is_ok());

        let unknown_slot = payload_from_body(&[
            TAG_GROUP_BEGIN,
            0x80,
            0x01,
            8,
            0,
            0,
            0,
            TAG_GROUP_END,
            TAG_EOF,
        ]);
        assert_eq!(
            Scene::preflight(&unknown_slot, &PayloadLimits::HOST),
            Ok(())
        );
        assert!(Scene::decode(&unknown_slot).is_ok());

        let nonminimal = payload_from_body(&[0x40, 0x80, 0, TAG_EOF]);
        assert_eq!(Scene::preflight(&nonminimal, &PayloadLimits::HOST), Ok(()));
        assert!(Scene::decode(&nonminimal).is_ok());
    }

    #[test]
    fn gradient_stops_fail_before_allocating_or_writing_output() {
        let mut scene = representative_scene();
        let gradient = scene
            .ops
            .iter_mut()
            .find_map(|op| match op {
                SceneOp::FillPath {
                    paint: Paint::LinearGradient(gradient),
                    ..
                } => Some(gradient),
                _ => None,
            })
            .unwrap();
        gradient.stops = Cow::Owned(vec![
            GradientStop {
                offset: Fixed::ONE,
                color: color(1),
            },
            GradientStop {
                offset: Fixed::ZERO,
                color: color(2),
            },
        ]);

        assert_eq!(scene.encode(), Err(CodecError::InvalidGradientStops));
        assert_eq!(
            scene.validate_limits(&PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::InvalidGradientStops))
        );
        let mut output = [0xa5; 512];
        assert!(scene.encode_payload_into(&mut output).is_err());
        assert!(output.iter().all(|byte| *byte == 0xa5));

        let stop_bytes = |offsets: &[Fixed]| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(offsets.len() as u32).to_le_bytes());
            for offset in offsets {
                bytes.extend_from_slice(&offset.to_le_bytes());
                bytes.extend_from_slice(&[1, 2, 3, 4]);
            }
            bytes
        };
        for offsets in [
            &[][..],
            &[Fixed::from_int(-1)][..],
            &[Fixed::from_int(2)][..],
            &[Fixed::ONE, Fixed::ZERO][..],
        ] {
            let bytes = stop_bytes(offsets);
            assert_eq!(
                Scanner::new(&bytes, PayloadLimits::HOST).scan_gradient_stops(),
                Err(VectorReadError::Codec(CodecError::InvalidGradientStops))
            );
        }
        let valid = stop_bytes(&[Fixed::ZERO, Fixed::ZERO, Fixed::ONE]);
        assert_eq!(
            Scanner::new(&valid, PayloadLimits::HOST).scan_gradient_stops(),
            Ok(())
        );

        let mut payload = representative_scene().encode().unwrap();
        let encoded_stops = [
            2, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 255, 0, 1, 0, 0, 2, 3, 4, 255,
        ];
        let offset = payload
            .windows(encoded_stops.len())
            .position(|window| window == encoded_stops)
            .unwrap();
        payload[offset + 4..offset + 8].copy_from_slice(&Fixed::from_int(2).to_le_bytes());
        refresh_payload_crc(&mut payload);
        assert_eq!(
            Scene::preflight(&payload, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::InvalidGradientStops))
        );
        assert_eq!(
            Scene::decode_with_limits(&payload, &PayloadLimits::HOST),
            Err(VectorReadError::Codec(CodecError::InvalidGradientStops))
        );
        assert_eq!(
            Scene::decode(&payload),
            Err(CodecError::InvalidGradientStops)
        );
    }
}
