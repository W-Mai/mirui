use alloc::vec::Vec;

use super::{
    ByteSink, CodecError, DEFAULT_SCALE, FIELD_ALPHA, FIELD_COMPOSITE, FIELD_QUAD, FIELD_RADIUS,
    FIELD_TRANSFORM, SLOT_CLIP, SLOT_DISJOINT_HINT, SLOT_FILTER, SLOT_MASK, SLOT_OPACITY,
    SLOT_PROJECTIVE, SLOT_TRANSFORM, TAG_EOF, TAG_GROUP_BEGIN, TAG_GROUP_END, VERSION, field_bits,
    write_op, write_resource_ref, write_transform, write_transform_3d, write_varuint,
};
use crate::path::{Path, PathCmd};
use crate::scene::{Paint, ResourceRef, Scene, SceneOp, VectorChunkHeader, VectorReadError};

/// Failure while encoding a canonical MIRX VECTOR payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VectorEncodeError {
    /// The scene cannot produce a structurally valid VECTOR payload.
    InvalidPayload(VectorReadError),
    /// The caller-provided output slice cannot hold the complete payload.
    BufferTooSmall { needed: usize, available: usize },
    /// The exact output allocation could not be reserved.
    AllocationFailed,
}

impl From<VectorReadError> for VectorEncodeError {
    fn from(value: VectorReadError) -> Self {
        Self::InvalidPayload(value)
    }
}

impl Scene {
    /// Returns the exact size of this scene's canonical VECTOR payload.
    pub fn encoded_payload_len(&self) -> Result<usize, VectorEncodeError> {
        Ok(self.payload_plan()?.encoded_len())
    }

    /// Encodes a canonical VECTOR payload into the start of `out`.
    ///
    /// The complete scene is validated before output capacity is inspected.
    /// Errors leave `out` unchanged, and success preserves any unused suffix.
    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, VectorEncodeError> {
        self.payload_plan()?.copy_payload_into(out)
    }

    /// Allocates and encodes one exact-length canonical VECTOR payload.
    pub fn encode_payload(&self) -> Result<Vec<u8>, VectorEncodeError> {
        self.payload_plan()?.payload_to_vec()
    }

    pub(crate) fn payload_plan(&self) -> Result<VectorPayloadPlan<'_>, VectorEncodeError> {
        VectorPayloadPlan::new(self)
    }

    #[cfg(test)]
    fn encode_payload_with(
        &self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), VectorEncodeError>,
    ) -> Result<Vec<u8>, VectorEncodeError> {
        self.payload_plan()?.payload_to_vec_with(reserve)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VectorPayloadPlan<'a> {
    scene: &'a Scene,
    body_len: usize,
    payload_len: usize,
}

impl<'a> VectorPayloadPlan<'a> {
    fn new(scene: &'a Scene) -> Result<Self, VectorEncodeError> {
        let body_len = checked_body_len(scene)?;
        let payload_len = checked_payload_len(body_len)?;
        Ok(Self {
            scene,
            body_len,
            payload_len,
        })
    }

    pub(crate) const fn encoded_len(self) -> usize {
        self.payload_len
    }

    pub(crate) fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, VectorEncodeError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(VectorEncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        self.emit_payload(&mut out[..needed]);
        Ok(needed)
    }

    pub(crate) fn payload_to_vec(self) -> Result<Vec<u8>, VectorEncodeError> {
        self.payload_to_vec_with(|out, needed| {
            out.try_reserve_exact(needed)
                .map_err(|_| VectorEncodeError::AllocationFailed)
        })
    }

    fn payload_to_vec_with(
        self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), VectorEncodeError>,
    ) -> Result<Vec<u8>, VectorEncodeError> {
        let needed = self.encoded_len();
        let mut out = Vec::new();
        reserve(&mut out, needed)?;
        out.resize(needed, 0);
        self.emit_payload(&mut out);
        Ok(out)
    }

    fn emit_payload(self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), self.payload_len);
        out.fill(0);
        let (header, body) = out.split_at_mut(VectorChunkHeader::SIZE);

        {
            let mut writer = SliceWriter::new(body);
            let mut group_head = None;
            for op in &self.scene.ops {
                match op {
                    SceneOp::GroupBegin {
                        transform,
                        projective,
                        opacity,
                        clip,
                        mask,
                        filter,
                        disjoint_hint,
                    } => {
                        writer.push(TAG_GROUP_BEGIN);
                        let bits = group_bits(
                            transform,
                            projective,
                            opacity,
                            clip,
                            mask,
                            filter,
                            *disjoint_hint,
                        );
                        write_varuint(&mut writer, bits);
                        let patch_pos = writer.position();
                        let previous = group_head
                            .map(|position| {
                                u32::try_from(position).expect("validated VECTOR group offset")
                            })
                            .unwrap_or(0);
                        writer.extend_from_slice(&previous.to_le_bytes());
                        if let Some(transform) = transform {
                            write_transform(&mut writer, *transform);
                        }
                        if let Some(value) = projective.filter(|value| !value.is_identity()) {
                            write_transform_3d(&mut writer, value);
                        }
                        if let Some(opacity) = opacity {
                            writer.push(*opacity);
                        }
                        if let Some(clip) = clip {
                            write_resource_ref(&mut writer, clip);
                        }
                        if let Some(mask) = mask {
                            write_resource_ref(&mut writer, mask);
                        }
                        if let Some(filter) = filter {
                            write_resource_ref(&mut writer, filter);
                        }
                        group_head = Some(patch_pos);
                    }
                    SceneOp::GroupEnd => {
                        let patch_pos = group_head.expect("validated VECTOR group balance");
                        let previous = writer.read_u32(patch_pos);
                        group_head = if previous == 0 {
                            None
                        } else {
                            Some(usize::try_from(previous).expect("u32 fits usize"))
                        };
                        writer.push(TAG_GROUP_END);
                        let target = u32::try_from(writer.position())
                            .expect("validated VECTOR group target");
                        writer.patch_u32(patch_pos, target);
                    }
                    _ => write_op(&mut writer, op).expect("validated non-group VECTOR op"),
                }
            }
            debug_assert!(group_head.is_none());
            writer.push(TAG_EOF);
            debug_assert_eq!(writer.position(), self.body_len);
        }

        let crc = crate::crc32::compute(body);
        header[0] = VectorChunkHeader::MAGIC;
        header[1] = VERSION;
        header[2] = DEFAULT_SCALE;
        header[3] = 0;
        header[4..8].copy_from_slice(&crc.to_le_bytes());
    }
}

#[derive(Clone, Copy, Default)]
struct CheckedLen(usize);

impl CheckedLen {
    const fn new(value: usize) -> Self {
        Self(value)
    }

    fn add(&mut self, bytes: usize) -> Result<(), VectorEncodeError> {
        self.0 = self.0.checked_add(bytes).ok_or_else(size_overflow)?;
        Ok(())
    }

    fn add_product(&mut self, count: usize, item_size: usize) -> Result<(), VectorEncodeError> {
        let bytes = count.checked_mul(item_size).ok_or_else(size_overflow)?;
        self.add(bytes)
    }

    const fn get(self) -> usize {
        self.0
    }
}

fn checked_body_len(scene: &Scene) -> Result<usize, VectorEncodeError> {
    let mut len = CheckedLen::default();
    let mut depth = 0usize;
    for op in &scene.ops {
        match op {
            SceneOp::GroupBegin { .. } => {
                depth = depth.checked_add(1).ok_or_else(size_overflow)?;
                len.add(group_begin_len(op)?)?;
            }
            SceneOp::GroupEnd => {
                if depth == 0 {
                    return Err(invalid_scene(CodecError::UnbalancedGroup));
                }
                depth -= 1;
                len.add(1)?;
            }
            _ => len.add(op_len(op)?)?,
        }
    }
    if depth != 0 {
        return Err(invalid_scene(CodecError::UnbalancedGroup));
    }
    len.add(1)?;
    Ok(len.get())
}

fn checked_payload_len(body_len: usize) -> Result<usize, VectorEncodeError> {
    let payload_len = VectorChunkHeader::SIZE
        .checked_add(body_len)
        .ok_or_else(size_overflow)?;
    u32::try_from(payload_len).map_err(|_| size_overflow())?;
    Ok(payload_len)
}

fn group_begin_len(op: &SceneOp) -> Result<usize, VectorEncodeError> {
    let SceneOp::GroupBegin {
        transform,
        projective,
        opacity,
        clip,
        mask,
        filter,
        disjoint_hint,
    } = op
    else {
        unreachable!("group size requires a group begin")
    };
    let bits = group_bits(
        transform,
        projective,
        opacity,
        clip,
        mask,
        filter,
        *disjoint_hint,
    );
    let mut len = CheckedLen::new(1 + varuint_len(bits) + 4);
    if transform.is_some() {
        len.add(24)?;
    }
    if projective.is_some_and(|value| !value.is_identity()) {
        len.add(72)?;
    }
    if opacity.is_some() {
        len.add(1)?;
    }
    if let Some(resource) = clip {
        len.add(resource_ref_len(resource)?)?;
    }
    if let Some(resource) = mask {
        len.add(resource_ref_len(resource)?)?;
    }
    if let Some(resource) = filter {
        len.add(resource_ref_len(resource)?)?;
    }
    Ok(len.get())
}

fn op_len(op: &SceneOp) -> Result<usize, VectorEncodeError> {
    let mut len = CheckedLen::new(2);
    match op {
        SceneOp::GroupBegin { .. } | SceneOp::GroupEnd => {
            unreachable!("group ops have dedicated sizing")
        }
        SceneOp::FillPath {
            path,
            transform,
            paint,
            ..
        } => {
            len.add(path_len(path)?)?;
            len.add(paint_len(paint)?)?;
            len.add(2)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::StrokePath {
            path,
            transform,
            paint,
            dash,
            ..
        } => {
            len.add(path_len(path)?)?;
            len.add(paint_len(paint)?)?;
            len.add(11)?;
            len.add(wire_collection_len(dash.len(), 4, 4)?)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::PushClip {
            path, transform, ..
        } => {
            len.add(path_len(path)?)?;
            len.add(1)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::PopClip => return Ok(1),
        SceneOp::FillRect {
            transform,
            quad,
            radius,
            ..
        } => {
            len.add(21)?;
            len.add(optional_len(field_bits(transform, quad, Some(*radius))))?;
        }
        SceneOp::Border {
            transform,
            quad,
            radius,
            ..
        } => {
            len.add(25)?;
            len.add(optional_len(field_bits(transform, quad, Some(*radius))))?;
        }
        SceneOp::GlyphRun {
            font,
            ppem,
            transform,
            glyphs,
            ..
        } => {
            if *ppem == 0 {
                return Err(invalid_scene(CodecError::InvalidPpem));
            }
            len.add(resource_ref_len(font)?)?;
            len.add(15)?;
            let count = checked_wire_len(glyphs.len())?;
            len.add(wire_collection_len(glyphs.len(), varuint_len(count), 18)?)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::PosedGlyphRun {
            font,
            ppem,
            transform,
            glyphs,
            ..
        } => {
            if *ppem == 0 {
                return Err(invalid_scene(CodecError::InvalidPpem));
            }
            if glyphs.iter().any(|glyph| !glyph.has_unit_tangent()) {
                return Err(invalid_scene(CodecError::InvalidGlyphDirection));
            }
            len.add(resource_ref_len(font)?)?;
            len.add(15)?;
            let count = checked_wire_len(glyphs.len())?;
            len.add(wire_collection_len(glyphs.len(), varuint_len(count), 18)?)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::Line { transform, .. } => {
            len.add(25)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::Arc { transform, .. } => {
            len.add(29)?;
            if !transform.is_identity() {
                len.add(24)?;
            }
        }
        SceneOp::Blit {
            texture,
            transform,
            quad,
            opa,
            radius,
            composite,
            ..
        } => {
            len.add(resource_ref_len(texture)?)?;
            len.add(16)?;
            let bits = field_bits(transform, quad, Some(*radius))
                | if *opa != 255 { FIELD_ALPHA } else { 0 }
                | if matches!(composite, crate::scene::CompositeMode::SourceOver) {
                    0
                } else {
                    FIELD_COMPOSITE
                };
            len.add(optional_len(bits))?;
            if bits & FIELD_ALPHA != 0 {
                len.add(1)?;
            }
            if bits & FIELD_COMPOSITE != 0 {
                len.add(1)?;
            }
        }
    }
    Ok(len.get())
}

fn path_len(path: &Path) -> Result<usize, VectorEncodeError> {
    let count = checked_wire_len(path.cmds.len())?;
    let mut len = CheckedLen::new(varuint_len(count));
    for command in &path.cmds {
        len.add(match command {
            PathCmd::MoveTo(_) | PathCmd::LineTo(_) => 9,
            PathCmd::QuadTo { .. } => 17,
            PathCmd::CubicTo { .. } => 25,
            PathCmd::Close => 1,
        })?;
    }
    Ok(len.get())
}

fn paint_len(paint: &Paint) -> Result<usize, VectorEncodeError> {
    match paint {
        Paint::Color(_) => Ok(5),
        Paint::LinearGradient(gradient) => {
            let mut len = CheckedLen::new(43);
            len.add(wire_collection_len(gradient.stops.len(), 4, 8)?)?;
            Ok(len.get())
        }
        Paint::RadialGradient(gradient) => {
            let mut len = CheckedLen::new(51);
            len.add(wire_collection_len(gradient.stops.len(), 4, 8)?)?;
            Ok(len.get())
        }
    }
}

fn resource_ref_len(resource: &ResourceRef) -> Result<usize, VectorEncodeError> {
    match resource {
        ResourceRef::Index(_) => Ok(5),
        ResourceRef::Token(token) => {
            let mut len = CheckedLen::new(1);
            len.add(string_len(token.len())?)?;
            Ok(len.get())
        }
        ResourceRef::Inline(path) => {
            let mut len = CheckedLen::new(1);
            len.add(path_len(path)?)?;
            Ok(len.get())
        }
    }
}

fn string_len(bytes: usize) -> Result<usize, VectorEncodeError> {
    let bytes_u32 = checked_wire_len(bytes)?;
    let mut len = CheckedLen::new(varuint_len(bytes_u32));
    len.add(bytes)?;
    Ok(len.get())
}

fn wire_collection_len(
    count: usize,
    prefix_len: usize,
    item_size: usize,
) -> Result<usize, VectorEncodeError> {
    checked_wire_len(count)?;
    let mut len = CheckedLen::new(prefix_len);
    len.add_product(count, item_size)?;
    Ok(len.get())
}

fn checked_wire_len(len: usize) -> Result<u32, VectorEncodeError> {
    u32::try_from(len).map_err(|_| size_overflow())
}

fn varuint_len(mut value: u32) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn optional_len(bits: u8) -> usize {
    let mut len = 0;
    if bits & FIELD_TRANSFORM != 0 {
        len += 24;
    }
    if bits & FIELD_QUAD != 0 {
        len += 32;
    }
    if bits & FIELD_RADIUS != 0 {
        len += 4;
    }
    len
}

fn group_bits(
    transform: &Option<crate::types::Transform>,
    projective: &Option<crate::types::Transform3D>,
    opacity: &Option<u8>,
    clip: &Option<ResourceRef>,
    mask: &Option<ResourceRef>,
    filter: &Option<ResourceRef>,
    disjoint_hint: bool,
) -> u32 {
    let mut bits = 0;
    if transform.is_some() {
        bits |= SLOT_TRANSFORM;
    }
    if projective.is_some_and(|value| !value.is_identity()) {
        bits |= SLOT_PROJECTIVE;
    }
    if opacity.is_some() {
        bits |= SLOT_OPACITY;
    }
    if clip.is_some() {
        bits |= SLOT_CLIP;
    }
    if mask.is_some() {
        bits |= SLOT_MASK;
    }
    if filter.is_some() {
        bits |= SLOT_FILTER;
    }
    if disjoint_hint {
        bits |= SLOT_DISJOINT_HINT;
    }
    bits
}

fn invalid_scene(error: CodecError) -> VectorEncodeError {
    VectorReadError::Codec(error).into()
}

fn size_overflow() -> VectorEncodeError {
    VectorReadError::SizeOverflow.into()
}

struct SliceWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }

    const fn position(&self) -> usize {
        self.pos
    }

    fn read_u32(&self, offset: usize) -> u32 {
        let bytes: [u8; 4] = self.out[offset..offset + 4]
            .try_into()
            .expect("validated VECTOR patch range");
        u32::from_le_bytes(bytes)
    }

    fn patch_u32(&mut self, offset: usize, value: u32) {
        self.out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

impl ByteSink for SliceWriter<'_> {
    fn push(&mut self, byte: u8) {
        self.out[self.pos] = byte;
        self.pos += 1;
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        let end = self
            .pos
            .checked_add(bytes.len())
            .expect("validated VECTOR output size");
        self.out[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::PayloadLimits;
    use crate::path::PathCmd;
    use crate::scene::codec::TAG_POP_CLIP;
    use crate::scene::{
        CompositeMode, FillRule, GradientStop, GradientUnits, LineCap, LineJoin, LinearGradient,
        RadialGradient, SpreadMode,
    };
    use crate::types::{Color, Fixed, Fixed64, Point, Rect, Transform, Transform3D};

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
        Color::rgba(value, value.wrapping_add(1), value.wrapping_add(2), 255)
    }

    fn close_path() -> Path {
        Path::from_cmds(vec![PathCmd::Close])
    }

    fn empty_group() -> SceneOp {
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        }
    }

    fn representative_scene() -> Scene {
        let transform = Transform::translate(fixed(1), fixed(2));
        let quad = Some([point(0, 0), point(2, 0), point(2, 2), point(0, 2)]);
        Scene::from_ops(vec![
            SceneOp::GroupBegin {
                transform: Some(transform),
                projective: Some(Transform3D {
                    m20: Fixed64::from_ratio(1, 800),
                    ..Transform3D::IDENTITY
                }),
                opacity: Some(200),
                clip: Some(ResourceRef::Token(String::from("clip"))),
                mask: Some(ResourceRef::Inline(close_path())),
                filter: Some(ResourceRef::Index(7)),
                disjoint_hint: true,
            },
            empty_group(),
            SceneOp::GroupEnd,
            SceneOp::PushClip {
                path: close_path(),
                transform,
                fill_rule: FillRule::EvenOdd,
            },
            SceneOp::PopClip,
            SceneOp::FillPath {
                path: Path::from_cmds(vec![
                    PathCmd::MoveTo(point(0, 0)),
                    PathCmd::LineTo(point(3, 4)),
                    PathCmd::QuadTo {
                        ctrl: point(1, 2),
                        end: point(4, 5),
                    },
                    PathCmd::CubicTo {
                        ctrl1: point(1, 1),
                        ctrl2: point(2, 2),
                        end: point(5, 5),
                    },
                    PathCmd::Close,
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
            SceneOp::FillPath {
                path: Path::default(),
                transform: Transform::IDENTITY,
                paint: Paint::Color(color(3)),
                opa: 255,
                fill_rule: FillRule::EvenOdd,
            },
            SceneOp::StrokePath {
                path: close_path(),
                transform,
                paint: Paint::RadialGradient(RadialGradient {
                    center: point(2, 2),
                    radius: fixed(2),
                    focal: point(1, 1),
                    focal_radius: fixed(1),
                    stops: Cow::Owned(vec![GradientStop {
                        offset: Fixed::ZERO,
                        color: color(4),
                    }]),
                    spread: SpreadMode::Reflect,
                    units: GradientUnits::ObjectBoundingBox,
                    transform,
                }),
                width: fixed(1),
                opa: 230,
                line_cap: LineCap::Round,
                line_join: LineJoin::Bevel,
                miter_limit: fixed(4),
                dash: Cow::Owned(vec![fixed(1), fixed(2)]),
            },
            SceneOp::FillRect {
                area: Rect::new(fixed(0), fixed(0), fixed(4), fixed(3)),
                transform,
                quad,
                color: color(5),
                radius: fixed(1),
                opa: 220,
            },
            SceneOp::Border {
                area: Rect::new(fixed(0), fixed(0), fixed(4), fixed(3)),
                transform: Transform::IDENTITY,
                quad: None,
                color: color(6),
                width: fixed(1),
                radius: Fixed::ZERO,
                opa: 210,
            },
            SceneOp::GlyphRun {
                font: ResourceRef::Index(3),
                ppem: 18,
                pos: point(2, 3),
                transform,
                color: color(8),
                opa: 195,
                glyphs: vec![
                    crate::scene::GlyphPlacement::new(42, point(0, 14)),
                    crate::scene::GlyphPlacement::new(43, point(9, 14)).with_offset(point(0, -1)),
                ],
            },
            SceneOp::Line {
                p1: point(0, 0),
                p2: point(3, 3),
                transform,
                color: color(8),
                width: fixed(1),
                opa: 190,
            },
            SceneOp::Arc {
                center: point(2, 2),
                transform,
                radius: fixed(2),
                start_angle: Fixed::ZERO,
                end_angle: fixed(1),
                color: color(9),
                width: fixed(1),
                opa: 180,
            },
            SceneOp::Blit {
                texture: ResourceRef::Inline(close_path()),
                pos: point(0, 0),
                size: point(2, 2),
                transform,
                quad,
                opa: 170,
                radius: fixed(1),
                composite: CompositeMode::Multiply,
            },
            SceneOp::Blit {
                texture: ResourceRef::Index(9),
                pos: Point::ZERO,
                size: point(1, 1),
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            },
            SceneOp::GroupEnd,
        ])
    }

    fn assert_same_bytes(scene: &Scene) {
        assert_eq!(scene.encode_payload().unwrap(), scene.encode().unwrap());
    }

    #[test]
    fn checked_encoder_matches_legacy_for_every_wire_shape() {
        let scene = representative_scene();
        let legacy = scene.encode().unwrap();
        let needed = scene.encoded_payload_len().unwrap();
        assert_eq!(needed, legacy.len());
        assert_eq!(scene.encode_payload().unwrap(), legacy);
        assert_eq!(
            scene.encode_payload().unwrap(),
            scene.encode_payload().unwrap()
        );

        let mut out = vec![0xa5; needed + 3];
        assert_eq!(scene.encode_payload_into(&mut out), Ok(needed));
        assert_eq!(&out[..needed], legacy);
        assert_eq!(&out[needed..], &[0xa5; 3]);
        assert_eq!(Scene::preflight(&legacy, &PayloadLimits::HOST), Ok(()));
        assert_eq!(Scene::decode(&legacy).unwrap(), scene);
    }

    #[test]
    fn projective_group_costs_one_shared_matrix_and_omits_identity() {
        let without = Scene::from_ops(vec![empty_group(), SceneOp::GroupEnd]);
        let with_identity = Scene::from_ops(vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(Transform3D::IDENTITY),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupEnd,
        ]);
        let with_perspective = Scene::from_ops(vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(Transform3D {
                    m20: Fixed64::from_ratio(1, 800),
                    ..Transform3D::IDENTITY
                }),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupEnd,
        ]);

        assert_eq!(
            without.encode_payload().unwrap(),
            with_identity.encode_payload().unwrap()
        );
        assert_eq!(
            with_perspective.encoded_payload_len().unwrap(),
            without.encoded_payload_len().unwrap() + 72
        );
        assert_eq!(
            Scene::decode(&with_perspective.encode_payload().unwrap()).unwrap(),
            with_perspective
        );
    }

    #[test]
    fn posed_records_are_fixed_size_and_validate_directions_before_output() {
        let make_scene = |glyphs| {
            Scene::from_ops(vec![SceneOp::PosedGlyphRun {
                font: ResourceRef::Index(1),
                ppem: 16,
                pos: Point::ZERO,
                transform: Transform::IDENTITY,
                color: color(1),
                opa: 255,
                glyphs,
            }])
        };
        let empty = make_scene(vec![]);
        let one = make_scene(vec![crate::scene::GlyphPose::new(
            5,
            point(2, 3),
            Point::new(Fixed::ONE, Fixed::ZERO),
        )]);
        assert_eq!(
            one.encoded_payload_len().unwrap(),
            empty.encoded_payload_len().unwrap() + 18
        );

        let invalid = make_scene(vec![crate::scene::GlyphPose::new(
            5,
            point(2, 3),
            Point::ZERO,
        )]);
        assert_eq!(
            invalid.encoded_payload_len(),
            Err(VectorEncodeError::InvalidPayload(VectorReadError::Codec(
                CodecError::InvalidGlyphDirection
            )))
        );
        let mut output = [0xa5; 64];
        let before = output;
        assert!(invalid.encode_payload_into(&mut output).is_err());
        assert_eq!(output, before);
    }

    #[test]
    fn empty_scene_checked_bytes_have_an_independent_golden() {
        let expected = [
            VectorChunkHeader::MAGIC,
            VERSION,
            DEFAULT_SCALE,
            0,
            0x8d,
            0xef,
            0x02,
            0xd2,
            TAG_EOF,
        ];
        let scene = Scene::default();
        assert_eq!(scene.encoded_payload_len(), Ok(expected.len()));
        assert_eq!(scene.encode_payload().as_deref(), Ok(expected.as_slice()));
    }

    #[test]
    fn enum_values_keep_the_canonical_legacy_encoding() {
        for fill_rule in [FillRule::EvenOdd, FillRule::NonZero] {
            assert_same_bytes(&Scene::from_ops(vec![SceneOp::FillPath {
                path: Path::default(),
                transform: Transform::IDENTITY,
                paint: Paint::Color(color(1)),
                opa: 255,
                fill_rule,
            }]));
        }

        for spread in [SpreadMode::Pad, SpreadMode::Reflect, SpreadMode::Repeat] {
            for units in [
                GradientUnits::UserSpaceOnUse,
                GradientUnits::ObjectBoundingBox,
            ] {
                assert_same_bytes(&Scene::from_ops(vec![SceneOp::FillPath {
                    path: Path::default(),
                    transform: Transform::IDENTITY,
                    paint: Paint::LinearGradient(LinearGradient {
                        start: Point::ZERO,
                        end: point(1, 1),
                        stops: Cow::Owned(vec![GradientStop {
                            offset: Fixed::ZERO,
                            color: color(1),
                        }]),
                        spread,
                        units,
                        transform: Transform::IDENTITY,
                    }),
                    opa: 255,
                    fill_rule: FillRule::NonZero,
                }]));
            }
        }

        for line_cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            for line_join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                assert_same_bytes(&Scene::from_ops(vec![SceneOp::StrokePath {
                    path: Path::default(),
                    transform: Transform::IDENTITY,
                    paint: Paint::Color(color(2)),
                    width: fixed(1),
                    opa: 255,
                    line_cap,
                    line_join,
                    miter_limit: fixed(4),
                    dash: Cow::Borrowed(&[]),
                }]));
            }
        }

        for composite in [
            CompositeMode::SourceOver,
            CompositeMode::Add,
            CompositeMode::Screen,
            CompositeMode::Multiply,
            CompositeMode::Darken,
            CompositeMode::Lighten,
            CompositeMode::Difference,
        ] {
            assert_same_bytes(&Scene::from_ops(vec![SceneOp::Blit {
                texture: ResourceRef::Index(1),
                pos: Point::ZERO,
                size: point(1, 1),
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
                radius: Fixed::ZERO,
                composite,
            }]));
        }
    }

    #[test]
    fn nested_group_targets_are_body_relative_and_crc_covers_final_patches() {
        let scene = Scene::from_ops(vec![
            empty_group(),
            empty_group(),
            SceneOp::PopClip,
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ]);
        let payload = scene.encode_payload().unwrap();
        let body = &payload[VectorChunkHeader::SIZE..];
        assert_eq!(body[0], TAG_GROUP_BEGIN);
        assert_eq!(body[6], TAG_GROUP_BEGIN);
        assert_eq!(body[12], TAG_POP_CLIP);
        assert_eq!(body[13], TAG_GROUP_END);
        assert_eq!(body[14], TAG_GROUP_END);
        assert_eq!(body[15], TAG_EOF);
        assert_eq!(u32::from_le_bytes(body[2..6].try_into().unwrap()), 15);
        assert_eq!(u32::from_le_bytes(body[8..12].try_into().unwrap()), 14);
        assert_eq!(
            u32::from_le_bytes(payload[4..8].try_into().unwrap()),
            crate::crc32::compute(body)
        );
        assert_eq!(payload, scene.encode().unwrap());
    }

    #[test]
    fn planning_and_capacity_failures_leave_the_complete_backing_unchanged() {
        let group_error =
            VectorEncodeError::InvalidPayload(VectorReadError::Codec(CodecError::UnbalancedGroup));
        for invalid in [
            Scene::from_ops(vec![SceneOp::GroupEnd]),
            Scene::from_ops(vec![empty_group()]),
        ] {
            assert_eq!(invalid.encoded_payload_len(), Err(group_error));

            let mut reserve_called = false;
            assert_eq!(
                invalid.encode_payload_with(|_, _| {
                    reserve_called = true;
                    Err(VectorEncodeError::AllocationFailed)
                }),
                Err(group_error)
            );
            assert!(!reserve_called);

            let mut backing = [0xa5; 16];
            let before = backing;
            assert_eq!(
                invalid.encode_payload_into(&mut backing[3..13]),
                Err(group_error)
            );
            assert_eq!(backing, before);
        }

        let scene = representative_scene();
        let needed = scene.encoded_payload_len().unwrap();
        let mut backing = vec![0xa5; needed + 8];
        let before = backing.clone();
        assert_eq!(
            scene.encode_payload_into(&mut backing[3..3 + needed - 1]),
            Err(VectorEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(backing, before);

        assert_eq!(
            scene.encode_payload_into(&mut backing[3..3 + needed + 2]),
            Ok(needed)
        );
        assert_eq!(&backing[..3], &[0xa5; 3]);
        assert_eq!(&backing[3 + needed..], &[0xa5; 5]);
    }

    #[test]
    fn allocation_failure_is_reported_after_exact_sizing() {
        let scene = representative_scene();
        let needed = scene.encoded_payload_len().unwrap();
        let mut reserve_called = false;
        assert_eq!(
            scene.encode_payload_with(|out, requested| {
                reserve_called = true;
                assert!(out.is_empty());
                assert_eq!(requested, needed);
                Err(VectorEncodeError::AllocationFailed)
            }),
            Err(VectorEncodeError::AllocationFailed)
        );
        assert!(reserve_called);
    }

    #[test]
    fn sizing_helpers_cover_varuint_and_wire_boundaries_without_allocating() {
        for (value, expected) in [
            (0, 1),
            (1, 1),
            (127, 1),
            (128, 2),
            (16_383, 2),
            (16_384, 3),
            (u32::MAX, 5),
        ] {
            assert_eq!(varuint_len(value), expected);
            let mut encoded = Vec::new();
            write_varuint(&mut encoded, value);
            assert_eq!(encoded.len(), expected);
        }
        for bytes in [0usize, 1, 127, 128, 16_383, 16_384] {
            assert_eq!(
                string_len(bytes),
                Ok(bytes + varuint_len(u32::try_from(bytes).unwrap()))
            );
        }

        assert_eq!(checked_wire_len(u32::MAX as usize), Ok(u32::MAX));
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            checked_wire_len(u32::MAX as usize + 1),
            Err(size_overflow())
        );

        assert_eq!(
            checked_payload_len(u32::MAX as usize - VectorChunkHeader::SIZE),
            Ok(u32::MAX as usize)
        );
        assert_eq!(
            checked_payload_len(u32::MAX as usize - VectorChunkHeader::SIZE + 1),
            Err(size_overflow())
        );
        assert_eq!(wire_collection_len(2, 4, 8), Ok(20));

        let mut len = CheckedLen::new(usize::MAX);
        assert_eq!(len.add(1), Err(size_overflow()));
        let mut len = CheckedLen::default();
        assert_eq!(len.add_product(usize::MAX, 2), Err(size_overflow()));
    }
}
