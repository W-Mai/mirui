//! Wire codec for the VECTOR (`chunk_type::VECTOR`) chunk payload.

use alloc::vec::Vec;

use super::{ResourceRef, SceneOp, VectorChunkHeader};
use crate::render::path::PathCmd;
use crate::render::raster::FillRule;
use crate::types::{Color, Fixed, Point, Rect, Transform};

pub const TAG_EOF: u8 = 0x00;
pub const TAG_GROUP_BEGIN: u8 = 0x01;
pub const TAG_GROUP_END: u8 = 0x02;
pub const TAG_FILL_PATH: u8 = 0x03;
pub const TAG_FILL_RECT: u8 = 0x04;
pub const TAG_BORDER: u8 = 0x05;
pub const TAG_LABEL: u8 = 0x06;
pub const TAG_LINE: u8 = 0x07;
pub const TAG_ARC: u8 = 0x08;
pub const TAG_BLIT: u8 = 0x09;

const FIELD_TRANSFORM: u8 = 1 << 0;
const FIELD_QUAD: u8 = 1 << 1;
const FIELD_RADIUS: u8 = 1 << 2;
const FIELD_ALPHA: u8 = 1 << 3;

const SLOT_TRANSFORM: u32 = 1 << 0;
const SLOT_OPACITY: u32 = 1 << 1;
const SLOT_CLIP: u32 = 1 << 2;
const SLOT_MASK: u32 = 1 << 3;
const SLOT_FILTER: u32 = 1 << 4;
const SLOT_DISJOINT_HINT: u32 = 1 << 5;

const RES_KIND_INDEX: u8 = 0;
const RES_KIND_TOKEN: u8 = 1;

const FILL_RULE_EVEN_ODD: u8 = 0;
const FILL_RULE_NON_ZERO: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    UnexpectedEof,
    BadMagic,
    UnknownVersion(u8),
    UnknownTag(u8),
    CrcMismatch { expected: u32, actual: u32 },
    BadFillRule(u8),
    BadResourceKind(u8),
    BadUtf8,
    UnbalancedGroup,
    BadSkipOffset,
    UnsupportedScale(u8),
    UnknownFlags(u8),
}

const VERSION: u8 = 1;
const DEFAULT_SCALE: u8 = 8;

/// Reads little-endian primitives, refusing to read past `buf`.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos.checked_add(n).ok_or(CodecError::UnexpectedEof)?;
        if end > self.buf.len() {
            return Err(CodecError::UnexpectedEof);
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn varuint(&mut self) -> Result<u32, CodecError> {
        let mut value: u32 = 0;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            value |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 32 {
                return Err(CodecError::UnexpectedEof);
            }
        }
    }

    fn fixed(&mut self) -> Result<Fixed, CodecError> {
        let b = self.take(4)?;
        Ok(Fixed::from_raw(i32::from_le_bytes([
            b[0], b[1], b[2], b[3],
        ])))
    }

    fn point(&mut self) -> Result<Point, CodecError> {
        Ok(Point {
            x: self.fixed()?,
            y: self.fixed()?,
        })
    }

    fn rect(&mut self) -> Result<Rect, CodecError> {
        Ok(Rect {
            x: self.fixed()?,
            y: self.fixed()?,
            w: self.fixed()?,
            h: self.fixed()?,
        })
    }

    fn color(&mut self) -> Result<Color, CodecError> {
        let b = self.take(4)?;
        Ok(Color {
            r: b[0],
            g: b[1],
            b: b[2],
            a: b[3],
        })
    }

    fn transform(&mut self) -> Result<Transform, CodecError> {
        Ok(Transform {
            m00: self.fixed()?,
            m01: self.fixed()?,
            tx: self.fixed()?,
            m10: self.fixed()?,
            m11: self.fixed()?,
            ty: self.fixed()?,
        })
    }

    fn quad(&mut self) -> Result<[Point; 4], CodecError> {
        Ok([self.point()?, self.point()?, self.point()?, self.point()?])
    }

    fn resource_ref(&mut self) -> Result<ResourceRef, CodecError> {
        match self.u8()? {
            RES_KIND_INDEX => Ok(ResourceRef::Index(self.u32()?)),
            RES_KIND_TOKEN => {
                let len = self.varuint()? as usize;
                let bytes = self.take(len)?;
                let s = core::str::from_utf8(bytes).map_err(|_| CodecError::BadUtf8)?;
                Ok(ResourceRef::Token(alloc::string::String::from(s).into()))
            }
            other => Err(CodecError::BadResourceKind(other)),
        }
    }
}

fn write_varuint(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn write_fixed(out: &mut Vec<u8>, f: Fixed) {
    out.extend_from_slice(&f.raw().to_le_bytes());
}

fn write_point(out: &mut Vec<u8>, p: Point) {
    write_fixed(out, p.x);
    write_fixed(out, p.y);
}

fn write_rect(out: &mut Vec<u8>, r: Rect) {
    write_fixed(out, r.x);
    write_fixed(out, r.y);
    write_fixed(out, r.w);
    write_fixed(out, r.h);
}

fn write_color(out: &mut Vec<u8>, c: Color) {
    out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
}

fn write_transform(out: &mut Vec<u8>, t: Transform) {
    for f in [t.m00, t.m01, t.tx, t.m10, t.m11, t.ty] {
        write_fixed(out, f);
    }
}

fn write_quad(out: &mut Vec<u8>, q: &[Point; 4]) {
    for p in q {
        write_point(out, *p);
    }
}

fn write_resource_ref(out: &mut Vec<u8>, r: &ResourceRef) {
    match r {
        ResourceRef::Index(i) => {
            out.push(RES_KIND_INDEX);
            out.extend_from_slice(&i.to_le_bytes());
        }
        ResourceRef::Token(s) => {
            out.push(RES_KIND_TOKEN);
            write_varuint(out, s.len() as u32);
            out.extend_from_slice(s.as_bytes());
        }
    }
}

fn fill_rule_to_u8(r: FillRule) -> u8 {
    match r {
        FillRule::EvenOdd => FILL_RULE_EVEN_ODD,
        FillRule::NonZero => FILL_RULE_NON_ZERO,
    }
}

fn fill_rule_from_u8(b: u8) -> Result<FillRule, CodecError> {
    match b {
        FILL_RULE_EVEN_ODD => Ok(FillRule::EvenOdd),
        FILL_RULE_NON_ZERO => Ok(FillRule::NonZero),
        other => Err(CodecError::BadFillRule(other)),
    }
}

fn write_path(out: &mut Vec<u8>, cmds: &[PathCmd]) {
    write_varuint(out, cmds.len() as u32);
    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                out.push(0);
                write_point(out, *p);
            }
            PathCmd::LineTo(p) => {
                out.push(1);
                write_point(out, *p);
            }
            PathCmd::QuadTo { ctrl, end } => {
                out.push(2);
                write_point(out, *ctrl);
                write_point(out, *end);
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                out.push(3);
                write_point(out, *ctrl1);
                write_point(out, *ctrl2);
                write_point(out, *end);
            }
            PathCmd::Close => out.push(4),
        }
    }
}

fn read_path(r: &mut Reader) -> Result<Vec<PathCmd>, CodecError> {
    let count = r.varuint()? as usize;
    let mut cmds = Vec::with_capacity(count);
    for _ in 0..count {
        let cmd = match r.u8()? {
            0 => PathCmd::MoveTo(r.point()?),
            1 => PathCmd::LineTo(r.point()?),
            2 => PathCmd::QuadTo {
                ctrl: r.point()?,
                end: r.point()?,
            },
            3 => PathCmd::CubicTo {
                ctrl1: r.point()?,
                ctrl2: r.point()?,
                end: r.point()?,
            },
            4 => PathCmd::Close,
            other => return Err(CodecError::UnknownTag(other)),
        };
        cmds.push(cmd);
    }
    Ok(cmds)
}

fn write_op(out: &mut Vec<u8>, op: &SceneOp) -> Result<(), CodecError> {
    match op {
        SceneOp::GroupBegin { .. } | SceneOp::GroupEnd => {
            unreachable!("group ops are dispatched by encode_scene, not write_op")
        }
        SceneOp::FillPath {
            path,
            transform,
            color,
            opa,
            fill_rule,
        } => {
            out.push(TAG_FILL_PATH);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_path(out, path);
            write_color(out, *color);
            out.push(*opa);
            out.push(fill_rule_to_u8(*fill_rule));
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::FillRect {
            area,
            transform,
            quad,
            color,
            radius,
            opa,
        } => {
            out.push(TAG_FILL_RECT);
            let bits = field_bits(transform, quad, Some(*radius));
            out.push(bits);
            write_rect(out, *area);
            write_color(out, *color);
            out.push(*opa);
            write_optional(out, bits, transform, quad, Some(*radius));
            Ok(())
        }
        SceneOp::Border {
            area,
            transform,
            quad,
            color,
            width,
            radius,
            opa,
        } => {
            out.push(TAG_BORDER);
            let bits = field_bits(transform, quad, Some(*radius));
            out.push(bits);
            write_rect(out, *area);
            write_fixed(out, *width);
            write_color(out, *color);
            out.push(*opa);
            write_optional(out, bits, transform, quad, Some(*radius));
            Ok(())
        }
        SceneOp::Label {
            font,
            pos,
            transform,
            color,
            opa,
            text,
        } => {
            out.push(TAG_LABEL);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_resource_ref(out, font);
            write_point(out, *pos);
            write_color(out, *color);
            out.push(*opa);
            write_varuint(out, text.len() as u32);
            out.extend_from_slice(text.as_bytes());
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::Line {
            p1,
            p2,
            transform,
            color,
            width,
            opa,
        } => {
            out.push(TAG_LINE);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_point(out, *p1);
            write_point(out, *p2);
            write_fixed(out, *width);
            write_color(out, *color);
            out.push(*opa);
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::Arc {
            center,
            transform,
            radius,
            start_angle,
            end_angle,
            color,
            width,
            opa,
        } => {
            out.push(TAG_ARC);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_point(out, *center);
            write_fixed(out, *radius);
            write_fixed(out, *start_angle);
            write_fixed(out, *end_angle);
            write_fixed(out, *width);
            write_color(out, *color);
            out.push(*opa);
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::Blit {
            texture,
            pos,
            size,
            transform,
            quad,
            opa,
        } => {
            out.push(TAG_BLIT);
            let mut bits = field_bits(transform, quad, None);
            if *opa != 255 {
                bits |= FIELD_ALPHA;
            }
            out.push(bits);
            write_resource_ref(out, texture);
            write_point(out, *pos);
            write_point(out, *size);
            write_optional(out, bits, transform, quad, None);
            if bits & FIELD_ALPHA != 0 {
                out.push(*opa);
            }
            Ok(())
        }
    }
}

fn field_bits(transform: &Transform, quad: &Option<[Point; 4]>, radius: Option<Fixed>) -> u8 {
    let mut bits = 0;
    if !transform.is_identity() {
        bits |= FIELD_TRANSFORM;
    }
    if quad.is_some() {
        bits |= FIELD_QUAD;
    }
    if matches!(radius, Some(r) if r.raw() != 0) {
        bits |= FIELD_RADIUS;
    }
    bits
}

fn write_optional(
    out: &mut Vec<u8>,
    bits: u8,
    transform: &Transform,
    quad: &Option<[Point; 4]>,
    radius: Option<Fixed>,
) {
    if bits & FIELD_TRANSFORM != 0 {
        write_transform(out, *transform);
    }
    if bits & FIELD_QUAD != 0 {
        if let Some(q) = quad {
            write_quad(out, q);
        }
    }
    if bits & FIELD_RADIUS != 0 {
        if let Some(r) = radius {
            write_fixed(out, r);
        }
    }
}

fn read_op(r: &mut Reader, tag: u8) -> Result<SceneOp, CodecError> {
    match tag {
        TAG_FILL_PATH => {
            let bits = r.u8()?;
            let path = read_path(r)?;
            let color = r.color()?;
            let opa = r.u8()?;
            let fill_rule = fill_rule_from_u8(r.u8()?)?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::FillPath {
                path: path.into(),
                transform,
                color,
                opa,
                fill_rule,
            })
        }
        TAG_FILL_RECT => {
            let bits = r.u8()?;
            let area = r.rect()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let (transform, quad, radius) = read_optional(r, bits)?;
            Ok(SceneOp::FillRect {
                area,
                transform,
                quad,
                color,
                radius: radius.unwrap_or(Fixed::ZERO),
                opa,
            })
        }
        TAG_BORDER => {
            let bits = r.u8()?;
            let area = r.rect()?;
            let width = r.fixed()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let (transform, quad, radius) = read_optional(r, bits)?;
            Ok(SceneOp::Border {
                area,
                transform,
                quad,
                color,
                width,
                radius: radius.unwrap_or(Fixed::ZERO),
                opa,
            })
        }
        TAG_LABEL => {
            let bits = r.u8()?;
            let font = r.resource_ref()?;
            let pos = r.point()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let len = r.varuint()? as usize;
            let bytes = r.take(len)?;
            let text = core::str::from_utf8(bytes).map_err(|_| CodecError::BadUtf8)?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::Label {
                font,
                pos,
                transform,
                color,
                opa,
                text: alloc::string::String::from(text).into(),
            })
        }
        TAG_LINE => {
            let bits = r.u8()?;
            let p1 = r.point()?;
            let p2 = r.point()?;
            let width = r.fixed()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            })
        }
        TAG_ARC => {
            let bits = r.u8()?;
            let center = r.point()?;
            let radius = r.fixed()?;
            let start_angle = r.fixed()?;
            let end_angle = r.fixed()?;
            let width = r.fixed()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            })
        }
        TAG_BLIT => {
            let bits = r.u8()?;
            let texture = r.resource_ref()?;
            let pos = r.point()?;
            let size = r.point()?;
            let (transform, quad, _) = read_optional(r, bits)?;
            let opa = if bits & FIELD_ALPHA != 0 {
                r.u8()?
            } else {
                255
            };
            Ok(SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
            })
        }
        TAG_GROUP_BEGIN | TAG_GROUP_END => {
            unreachable!("group tags are dispatched by decode_scene, not read_op")
        }
        other => Err(CodecError::UnknownTag(other)),
    }
}

fn read_transform_opt(r: &mut Reader, bits: u8) -> Result<Transform, CodecError> {
    if bits & FIELD_TRANSFORM != 0 {
        r.transform()
    } else {
        Ok(Transform::IDENTITY)
    }
}

#[allow(clippy::type_complexity)]
fn read_optional(
    r: &mut Reader,
    bits: u8,
) -> Result<(Transform, Option<[Point; 4]>, Option<Fixed>), CodecError> {
    let transform = read_transform_opt(r, bits)?;
    let quad = if bits & FIELD_QUAD != 0 {
        Some(r.quad()?)
    } else {
        None
    };
    let radius = if bits & FIELD_RADIUS != 0 {
        Some(r.fixed()?)
    } else {
        None
    };
    Ok((transform, quad, radius))
}

/// Serialise an op stream into a complete VECTOR chunk payload.
pub fn encode_scene(ops: &[SceneOp]) -> Result<Vec<u8>, CodecError> {
    let mut body = Vec::new();
    let mut group_stack: Vec<usize> = Vec::new();
    for op in ops {
        match op {
            SceneOp::GroupBegin {
                transform,
                opacity,
                clip,
                mask,
                filter,
                disjoint_hint,
            } => {
                body.push(TAG_GROUP_BEGIN);
                let mut bits = 0u32;
                if transform.is_some() {
                    bits |= SLOT_TRANSFORM;
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
                if *disjoint_hint {
                    bits |= SLOT_DISJOINT_HINT;
                }
                write_varuint(&mut body, bits);
                let patch_pos = body.len();
                body.extend_from_slice(&0u32.to_le_bytes());
                if let Some(t) = transform {
                    write_transform(&mut body, *t);
                }
                if let Some(o) = opacity {
                    body.push(*o);
                }
                if let Some(c) = clip {
                    write_resource_ref(&mut body, c);
                }
                if let Some(m) = mask {
                    write_resource_ref(&mut body, m);
                }
                if let Some(f) = filter {
                    write_resource_ref(&mut body, f);
                }
                group_stack.push(patch_pos);
            }
            SceneOp::GroupEnd => {
                let patch_pos = group_stack.pop().ok_or(CodecError::UnbalancedGroup)?;
                body.push(TAG_GROUP_END);
                let target = body.len() as u32;
                body[patch_pos..patch_pos + 4].copy_from_slice(&target.to_le_bytes());
            }
            _ => write_op(&mut body, op)?,
        }
    }
    if !group_stack.is_empty() {
        return Err(CodecError::UnbalancedGroup);
    }
    body.push(TAG_EOF);

    let crc = mirx::crc32(&body);
    let mut out = Vec::with_capacity(VectorChunkHeader::SIZE + body.len());
    out.push(VectorChunkHeader::MAGIC);
    out.push(VERSION);
    out.push(DEFAULT_SCALE);
    out.push(0);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_scene(payload: &[u8]) -> Result<Vec<SceneOp>, CodecError> {
    let mut head = Reader::new(payload);
    if head.u8()? != VectorChunkHeader::MAGIC {
        return Err(CodecError::BadMagic);
    }
    let version = head.u8()?;
    if version != VERSION {
        return Err(CodecError::UnknownVersion(version));
    }
    let scale = head.u8()?;
    if scale != DEFAULT_SCALE {
        return Err(CodecError::UnsupportedScale(scale));
    }
    let flags = head.u8()?;
    if flags != 0 {
        return Err(CodecError::UnknownFlags(flags));
    }
    let stored_crc = head.u32()?;

    let body = &payload[VectorChunkHeader::SIZE..];
    let actual_crc = mirx::crc32(body);
    if stored_crc != actual_crc {
        return Err(CodecError::CrcMismatch {
            expected: stored_crc,
            actual: actual_crc,
        });
    }

    let mut r = Reader::new(body);
    let mut ops = Vec::new();
    let mut depth = 0usize;
    loop {
        let tag_pos = r.pos;
        let tag = r.u8()?;
        match tag {
            TAG_EOF => {
                if depth != 0 {
                    return Err(CodecError::UnbalancedGroup);
                }
                return Ok(ops);
            }
            TAG_GROUP_BEGIN => {
                let bits = r.varuint()?;
                let target = r.u32()? as usize;
                if target <= tag_pos || target > body.len() || body[target - 1] != TAG_GROUP_END {
                    return Err(CodecError::BadSkipOffset);
                }
                let transform = if bits & SLOT_TRANSFORM != 0 {
                    Some(r.transform()?)
                } else {
                    None
                };
                let opacity = if bits & SLOT_OPACITY != 0 {
                    Some(r.u8()?)
                } else {
                    None
                };
                let clip = if bits & SLOT_CLIP != 0 {
                    Some(r.resource_ref()?)
                } else {
                    None
                };
                let mask = if bits & SLOT_MASK != 0 {
                    Some(r.resource_ref()?)
                } else {
                    None
                };
                let filter = if bits & SLOT_FILTER != 0 {
                    Some(r.resource_ref()?)
                } else {
                    None
                };
                let disjoint_hint = bits & SLOT_DISJOINT_HINT != 0;
                depth += 1;
                ops.push(SceneOp::GroupBegin {
                    transform,
                    opacity,
                    clip,
                    mask,
                    filter,
                    disjoint_hint,
                });
            }
            TAG_GROUP_END => {
                if depth == 0 {
                    return Err(CodecError::UnbalancedGroup);
                }
                depth -= 1;
                ops.push(SceneOp::GroupEnd);
            }
            0x40..=0x7F => {
                let len = r.varuint()? as usize;
                let _ = r.take(len)?;
            }
            _ => ops.push(read_op(&mut r, tag)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    fn roundtrip(ops: vec::Vec<SceneOp>) {
        let bytes = encode_scene(&ops).unwrap();
        let back = decode_scene(&bytes).unwrap();
        assert_eq!(back, ops);
    }

    #[test]
    fn fill_rect_identity_roundtrips() {
        roundtrip(vec![SceneOp::FillRect {
            area: Rect {
                x: Fixed::from_int(1),
                y: Fixed::from_int(2),
                w: Fixed::from_int(3),
                h: Fixed::from_int(4),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: red(),
            radius: Fixed::ZERO,
            opa: 200,
        }]);
    }

    #[test]
    fn fill_rect_with_transform_and_radius_roundtrips() {
        let mut t = Transform::IDENTITY;
        t.tx = Fixed::from_int(10);
        roundtrip(vec![SceneOp::FillRect {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(8),
                h: Fixed::from_int(8),
            },
            transform: t,
            quad: None,
            color: red(),
            radius: Fixed::from_int(2),
            opa: 255,
        }]);
    }

    #[test]
    fn blit_with_quad_roundtrips() {
        let q = [
            Point::ZERO,
            Point {
                x: Fixed::from_int(5),
                y: Fixed::ZERO,
            },
            Point {
                x: Fixed::from_int(5),
                y: Fixed::from_int(5),
            },
            Point {
                x: Fixed::ZERO,
                y: Fixed::from_int(5),
            },
        ];
        roundtrip(vec![SceneOp::Blit {
            texture: ResourceRef::Index(7),
            pos: Point::ZERO,
            size: Point {
                x: Fixed::from_int(5),
                y: Fixed::from_int(5),
            },
            transform: Transform::IDENTITY,
            quad: Some(q),
            opa: 255,
        }]);
    }

    #[test]
    fn blit_with_alpha_roundtrips() {
        roundtrip(vec![SceneOp::Blit {
            texture: ResourceRef::Index(3),
            pos: Point::ZERO,
            size: Point {
                x: Fixed::from_int(8),
                y: Fixed::from_int(8),
            },
            transform: Transform::IDENTITY,
            quad: None,
            opa: 128,
        }]);
    }

    #[test]
    fn label_with_token_roundtrips() {
        roundtrip(vec![SceneOp::Label {
            font: ResourceRef::Token("noto-sans".into()),
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: red(),
            opa: 255,
            text: "héllo".into(),
        }]);
    }

    #[test]
    fn fill_path_roundtrips() {
        roundtrip(vec![SceneOp::FillPath {
            path: vec![
                PathCmd::MoveTo(Point::ZERO),
                PathCmd::LineTo(Point {
                    x: Fixed::from_int(4),
                    y: Fixed::ZERO,
                }),
                PathCmd::QuadTo {
                    ctrl: Point {
                        x: Fixed::from_int(4),
                        y: Fixed::from_int(4),
                    },
                    end: Point {
                        x: Fixed::ZERO,
                        y: Fixed::from_int(4),
                    },
                },
                PathCmd::Close,
            ]
            .into(),
            transform: Transform::IDENTITY,
            color: red(),
            opa: 255,
            fill_rule: FillRule::NonZero,
        }]);
    }

    #[test]
    fn multi_op_stream_roundtrips() {
        roundtrip(vec![
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point {
                    x: Fixed::from_int(9),
                    y: Fixed::from_int(9),
                },
                transform: Transform::IDENTITY,
                color: red(),
                width: Fixed::from_int(1),
                opa: 255,
            },
            SceneOp::Arc {
                center: Point::ZERO,
                transform: Transform::IDENTITY,
                radius: Fixed::from_int(10),
                start_angle: Fixed::ZERO,
                end_angle: Fixed::from_int(90),
                color: red(),
                width: Fixed::from_int(2),
                opa: 128,
            },
        ]);
    }

    #[test]
    fn corrupt_crc_is_rejected() {
        let ops = vec![SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::ZERO,
            transform: Transform::IDENTITY,
            color: red(),
            width: Fixed::from_int(1),
            opa: 255,
        }];
        let mut bytes = encode_scene(&ops).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode_scene(&bytes),
            Err(CodecError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let ops = vec![SceneOp::FillRect {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(4),
                h: Fixed::from_int(4),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: red(),
            radius: Fixed::ZERO,
            opa: 255,
        }];
        let bytes = encode_scene(&ops).unwrap();
        assert!(matches!(
            decode_scene(&bytes[..bytes.len() - 4]),
            Err(CodecError::CrcMismatch { .. }) | Err(CodecError::UnexpectedEof)
        ));
    }

    fn group_begin_transform() -> SceneOp {
        let mut t = Transform::IDENTITY;
        t.tx = Fixed::from_int(3);
        SceneOp::GroupBegin {
            transform: Some(t),
            opacity: Some(128),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        }
    }

    #[test]
    fn nested_groups_roundtrip() {
        roundtrip(vec![
            group_begin_transform(),
            SceneOp::FillRect {
                area: Rect {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    w: Fixed::from_int(4),
                    h: Fixed::from_int(4),
                },
                transform: Transform::IDENTITY,
                quad: None,
                color: red(),
                radius: Fixed::ZERO,
                opa: 255,
            },
            SceneOp::GroupBegin {
                transform: None,
                opacity: None,
                clip: Some(ResourceRef::Index(2)),
                mask: None,
                filter: Some(ResourceRef::Token("blur".into())),
                disjoint_hint: false,
            },
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::ZERO,
                transform: Transform::IDENTITY,
                color: red(),
                width: Fixed::from_int(1),
                opa: 255,
            },
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ]);
    }

    #[test]
    fn unbalanced_group_end_is_rejected_at_encode() {
        assert!(matches!(
            encode_scene(&[SceneOp::GroupEnd]),
            Err(CodecError::UnbalancedGroup)
        ));
    }

    #[test]
    fn unclosed_group_is_rejected_at_encode() {
        assert!(matches!(
            encode_scene(&[group_begin_transform()]),
            Err(CodecError::UnbalancedGroup)
        ));
    }

    #[test]
    fn corrupt_skip_offset_is_rejected() {
        let ops = vec![group_begin_transform(), SceneOp::GroupEnd];
        let mut bytes = encode_scene(&ops).unwrap();
        let skip_off = VectorChunkHeader::SIZE + 1 + 1;
        bytes[skip_off..skip_off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let crc = mirx::crc32(&bytes[VectorChunkHeader::SIZE..]);
        bytes[4..8].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            decode_scene(&bytes),
            Err(CodecError::BadSkipOffset)
        ));
    }

    #[test]
    fn golden_bytes_are_stable() {
        let ops = vec![
            SceneOp::FillRect {
                area: Rect {
                    x: Fixed::from_int(1),
                    y: Fixed::from_int(2),
                    w: Fixed::from_int(3),
                    h: Fixed::from_int(4),
                },
                transform: Transform::IDENTITY,
                quad: None,
                color: Color {
                    r: 0x11,
                    g: 0x22,
                    b: 0x33,
                    a: 0x44,
                },
                radius: Fixed::ZERO,
                opa: 0xAB,
            },
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point {
                    x: Fixed::from_int(5),
                    y: Fixed::ZERO,
                },
                transform: Transform::IDENTITY,
                color: Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4,
                },
                width: Fixed::from_int(1),
                opa: 0xFF,
            },
        ];
        let golden: &[u8] = &[
            0x03, 0x01, 0x08, 0x00, 0x84, 0xcc, 0x5e, 0xd3, 0x04, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x11, 0x22,
            0x33, 0x44, 0xab, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x02, 0x03,
            0x04, 0xff, 0x00,
        ];
        assert_eq!(encode_scene(&ops).unwrap(), golden);
        assert_eq!(decode_scene(golden).unwrap(), ops);
    }

    #[test]
    fn skippable_extension_op_is_skipped() {
        let line = vec![SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::ZERO,
            transform: Transform::IDENTITY,
            color: red(),
            width: Fixed::from_int(1),
            opa: 255,
        }];
        let mut bytes = encode_scene(&line).unwrap();
        let body_start = VectorChunkHeader::SIZE;
        let injected: &[u8] = &[0x40, 0x03, 0xAA, 0xBB, 0xCC];
        bytes.splice(body_start..body_start, injected.iter().copied());
        let new_crc = mirx::crc32(&bytes[body_start..]);
        bytes[4..8].copy_from_slice(&new_crc.to_le_bytes());

        assert_eq!(decode_scene(&bytes).unwrap(), line);
    }

    #[test]
    fn unsupported_scale_is_rejected() {
        let mut bytes = encode_scene(&[]).unwrap();
        bytes[2] = 7;
        let new_crc = mirx::crc32(&bytes[VectorChunkHeader::SIZE..]);
        bytes[4..8].copy_from_slice(&new_crc.to_le_bytes());
        assert!(matches!(
            decode_scene(&bytes),
            Err(CodecError::UnsupportedScale(7))
        ));
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let mut bytes = encode_scene(&[]).unwrap();
        bytes[3] = VectorChunkHeader::FLAG_HAS_RESOURCE_TABLE;
        let new_crc = mirx::crc32(&bytes[VectorChunkHeader::SIZE..]);
        bytes[4..8].copy_from_slice(&new_crc.to_le_bytes());
        assert!(matches!(
            decode_scene(&bytes),
            Err(CodecError::UnknownFlags(_))
        ));
    }

    #[test]
    fn group_disjoint_hint_roundtrips() {
        let mk = |hint: bool| {
            vec![
                SceneOp::GroupBegin {
                    transform: None,
                    opacity: Some(200),
                    clip: None,
                    mask: None,
                    filter: None,
                    disjoint_hint: hint,
                },
                SceneOp::GroupEnd,
            ]
        };
        for hint in [false, true] {
            let ops = mk(hint);
            let bytes = encode_scene(&ops).unwrap();
            let back = decode_scene(&bytes).unwrap();
            assert_eq!(back, ops);
            match &back[0] {
                SceneOp::GroupBegin { disjoint_hint, .. } => assert_eq!(*disjoint_hint, hint),
                _ => panic!("expected GroupBegin"),
            }
        }
    }
}
