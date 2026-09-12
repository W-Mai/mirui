use alloc::string::String;
use alloc::vec::Vec;

use crate::crc32;
use crate::path::{Path, PathCmd};
use crate::scene::header::VectorChunkHeader;
use crate::scene::op::{
    CompositeMode, FillRule, GlyphPlacement, GlyphPose, LineCap, LineJoin, ResourceRef, Scene,
    SceneOp,
};
use crate::scene::paint::{
    GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode,
};
use crate::types::{Color, Fixed, Fixed64, Point, Rect, Transform, Transform3D};

mod encoder;
mod preflight;

pub use encoder::VectorEncodeError;
pub use preflight::VectorReadError;

const TAG_EOF: u8 = 0x00;
const TAG_GROUP_BEGIN: u8 = 0x01;
const TAG_GROUP_END: u8 = 0x02;
const TAG_FILL_PATH: u8 = 0x03;
const TAG_FILL_RECT: u8 = 0x04;
const TAG_BORDER: u8 = 0x05;
const TAG_LINE: u8 = 0x07;
const TAG_ARC: u8 = 0x08;
const TAG_BLIT: u8 = 0x09;
const TAG_STROKE_PATH: u8 = 0x0A;
const TAG_PUSH_CLIP: u8 = 0x0B;
const TAG_POP_CLIP: u8 = 0x0C;
const TAG_GLYPH_RUN: u8 = 0x0D;
const TAG_POSED_GLYPH_RUN: u8 = 0x0E;

const FIELD_TRANSFORM: u8 = 1 << 0;
const FIELD_QUAD: u8 = 1 << 1;
const FIELD_RADIUS: u8 = 1 << 2;
const FIELD_ALPHA: u8 = 1 << 3;
const FIELD_COMPOSITE: u8 = 1 << 4;

const SLOT_TRANSFORM: u32 = 1 << 0;
const SLOT_OPACITY: u32 = 1 << 1;
const SLOT_CLIP: u32 = 1 << 2;
const SLOT_MASK: u32 = 1 << 3;
const SLOT_FILTER: u32 = 1 << 4;
const SLOT_DISJOINT_HINT: u32 = 1 << 5;
const SLOT_PROJECTIVE: u32 = 1 << 6;

const RES_KIND_INDEX: u8 = 0;
const RES_KIND_TOKEN: u8 = 1;
const RES_KIND_INLINE: u8 = 2;

const FILL_RULE_EVEN_ODD: u8 = 0;
const FILL_RULE_NON_ZERO: u8 = 1;

const COMPOSITE_SOURCE_OVER: u8 = 0;
const COMPOSITE_ADD: u8 = 1;
const COMPOSITE_SCREEN: u8 = 2;
const COMPOSITE_MULTIPLY: u8 = 3;
const COMPOSITE_DARKEN: u8 = 4;
const COMPOSITE_LIGHTEN: u8 = 5;
const COMPOSITE_DIFFERENCE: u8 = 6;

const LINE_CAP_BUTT: u8 = 0;
const LINE_CAP_ROUND: u8 = 1;
const LINE_CAP_SQUARE: u8 = 2;

const LINE_JOIN_MITER: u8 = 0;
const LINE_JOIN_ROUND: u8 = 1;
const LINE_JOIN_BEVEL: u8 = 2;

fn line_cap_to_u8(c: LineCap) -> u8 {
    match c {
        LineCap::Butt => LINE_CAP_BUTT,
        LineCap::Round => LINE_CAP_ROUND,
        LineCap::Square => LINE_CAP_SQUARE,
    }
}

fn line_cap_from_u8(v: u8) -> Result<LineCap, CodecError> {
    match v {
        LINE_CAP_BUTT => Ok(LineCap::Butt),
        LINE_CAP_ROUND => Ok(LineCap::Round),
        LINE_CAP_SQUARE => Ok(LineCap::Square),
        other => Err(CodecError::BadComposite(other)),
    }
}

fn line_join_to_u8(j: LineJoin) -> u8 {
    match j {
        LineJoin::Miter => LINE_JOIN_MITER,
        LineJoin::Round => LINE_JOIN_ROUND,
        LineJoin::Bevel => LINE_JOIN_BEVEL,
    }
}

fn line_join_from_u8(v: u8) -> Result<LineJoin, CodecError> {
    match v {
        LINE_JOIN_MITER => Ok(LineJoin::Miter),
        LINE_JOIN_ROUND => Ok(LineJoin::Round),
        LINE_JOIN_BEVEL => Ok(LineJoin::Bevel),
        other => Err(CodecError::BadComposite(other)),
    }
}

const VERSION: u8 = 1;
const DEFAULT_SCALE: u8 = 8;

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
    BadComposite(u8),
    InvalidPpem,
    InvalidGlyphDirection,
}

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

    fn u16(&mut self) -> Result<u16, CodecError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
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
        Ok(Fixed::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn fixed64(&mut self) -> Result<Fixed64, CodecError> {
        let b = self.take(8)?;
        Ok(Fixed64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
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

    fn transform_3d(&mut self) -> Result<Transform3D, CodecError> {
        Ok(Transform3D {
            m00: self.fixed64()?,
            m01: self.fixed64()?,
            m02: self.fixed64()?,
            m10: self.fixed64()?,
            m11: self.fixed64()?,
            m12: self.fixed64()?,
            m20: self.fixed64()?,
            m21: self.fixed64()?,
            m22: self.fixed64()?,
        })
    }

    fn quad(&mut self) -> Result<[Point; 4], CodecError> {
        Ok([self.point()?, self.point()?, self.point()?, self.point()?])
    }
}

trait DecodeAllocator {
    type Error: From<CodecError>;

    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<(), Self::Error>;
    fn copy_string(&mut self, value: &str) -> Result<String, Self::Error>;
}

struct LegacyDecodeAllocator;

impl DecodeAllocator for LegacyDecodeAllocator {
    type Error = CodecError;

    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<(), Self::Error> {
        values.reserve_exact(additional);
        Ok(())
    }

    fn copy_string(&mut self, value: &str) -> Result<String, Self::Error> {
        Ok(String::from(value))
    }
}

struct CheckedDecodeAllocator;

impl DecodeAllocator for CheckedDecodeAllocator {
    type Error = VectorReadError;

    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<(), Self::Error> {
        values
            .try_reserve_exact(additional)
            .map_err(|_| VectorReadError::AllocationFailed)
    }

    fn copy_string(&mut self, value: &str) -> Result<String, Self::Error> {
        let mut string = String::new();
        string
            .try_reserve_exact(value.len())
            .map_err(|_| VectorReadError::AllocationFailed)?;
        string.push_str(value);
        Ok(string)
    }
}

pub(super) trait ByteSink {
    fn push(&mut self, byte: u8);
    fn extend_from_slice(&mut self, bytes: &[u8]);
}

impl ByteSink for Vec<u8> {
    fn push(&mut self, byte: u8) {
        Vec::push(self, byte);
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        Vec::extend_from_slice(self, bytes);
    }
}

pub(super) fn write_varuint<W: ByteSink>(out: &mut W, mut value: u32) {
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

fn write_fixed<W: ByteSink>(out: &mut W, f: Fixed) {
    out.extend_from_slice(&f.to_le_bytes());
}

fn write_point<W: ByteSink>(out: &mut W, p: Point) {
    write_fixed(out, p.x);
    write_fixed(out, p.y);
}

fn write_rect<W: ByteSink>(out: &mut W, r: Rect) {
    write_fixed(out, r.x);
    write_fixed(out, r.y);
    write_fixed(out, r.w);
    write_fixed(out, r.h);
}

fn write_color<W: ByteSink>(out: &mut W, c: Color) {
    out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
}

const PAINT_KIND_COLOR: u8 = 0;
const PAINT_KIND_LINEAR: u8 = 1;
const PAINT_KIND_RADIAL: u8 = 2;

const SPREAD_PAD: u8 = 0;
const SPREAD_REFLECT: u8 = 1;
const SPREAD_REPEAT: u8 = 2;

const UNITS_USER: u8 = 0;
const UNITS_BBOX: u8 = 1;

fn spread_to_u8(s: SpreadMode) -> u8 {
    match s {
        SpreadMode::Pad => SPREAD_PAD,
        SpreadMode::Reflect => SPREAD_REFLECT,
        SpreadMode::Repeat => SPREAD_REPEAT,
    }
}

fn spread_from_u8(v: u8) -> Result<SpreadMode, CodecError> {
    match v {
        SPREAD_PAD => Ok(SpreadMode::Pad),
        SPREAD_REFLECT => Ok(SpreadMode::Reflect),
        SPREAD_REPEAT => Ok(SpreadMode::Repeat),
        other => Err(CodecError::BadComposite(other)),
    }
}

fn units_to_u8(u: GradientUnits) -> u8 {
    match u {
        GradientUnits::UserSpaceOnUse => UNITS_USER,
        GradientUnits::ObjectBoundingBox => UNITS_BBOX,
    }
}

fn units_from_u8(v: u8) -> Result<GradientUnits, CodecError> {
    match v {
        UNITS_USER => Ok(GradientUnits::UserSpaceOnUse),
        UNITS_BBOX => Ok(GradientUnits::ObjectBoundingBox),
        other => Err(CodecError::BadComposite(other)),
    }
}

fn write_gradient_stops<W: ByteSink>(out: &mut W, stops: &[GradientStop]) {
    out.extend_from_slice(&(stops.len() as u32).to_le_bytes());
    for s in stops {
        write_fixed(out, s.offset);
        write_color(out, s.color);
    }
}

fn read_gradient_stops_with<A: DecodeAllocator>(
    r: &mut Reader,
    allocator: &mut A,
) -> Result<Vec<GradientStop>, A::Error> {
    let count = r.u32()? as usize;
    let mut stops = Vec::new();
    allocator.reserve(&mut stops, count)?;
    for _ in 0..count {
        let offset = r.fixed()?;
        let color = r.color()?;
        stops.push(GradientStop { offset, color });
    }
    Ok(stops)
}

fn write_paint<W: ByteSink>(out: &mut W, paint: &Paint) {
    match paint {
        Paint::Color(c) => {
            out.push(PAINT_KIND_COLOR);
            write_color(out, *c);
        }
        Paint::LinearGradient(g) => {
            out.push(PAINT_KIND_LINEAR);
            write_point(out, g.start);
            write_point(out, g.end);
            write_gradient_stops(out, &g.stops);
            out.push(spread_to_u8(g.spread));
            out.push(units_to_u8(g.units));
            write_transform(out, g.transform);
        }
        Paint::RadialGradient(g) => {
            out.push(PAINT_KIND_RADIAL);
            write_point(out, g.center);
            write_fixed(out, g.radius);
            write_point(out, g.focal);
            write_fixed(out, g.focal_radius);
            write_gradient_stops(out, &g.stops);
            out.push(spread_to_u8(g.spread));
            out.push(units_to_u8(g.units));
            write_transform(out, g.transform);
        }
    }
}

fn read_paint_with<A: DecodeAllocator>(
    r: &mut Reader,
    allocator: &mut A,
) -> Result<Paint, A::Error> {
    let kind = r.u8()?;
    match kind {
        PAINT_KIND_COLOR => Ok(Paint::Color(r.color()?)),
        PAINT_KIND_LINEAR => {
            let start = r.point()?;
            let end = r.point()?;
            let stops = alloc::borrow::Cow::Owned(read_gradient_stops_with(r, allocator)?);
            let spread = spread_from_u8(r.u8()?)?;
            let units = units_from_u8(r.u8()?)?;
            let transform = read_transform_raw(r)?;
            Ok(Paint::LinearGradient(LinearGradient {
                start,
                end,
                stops,
                spread,
                units,
                transform,
            }))
        }
        PAINT_KIND_RADIAL => {
            let center = r.point()?;
            let radius = r.fixed()?;
            let focal = r.point()?;
            let focal_radius = r.fixed()?;
            let stops = alloc::borrow::Cow::Owned(read_gradient_stops_with(r, allocator)?);
            let spread = spread_from_u8(r.u8()?)?;
            let units = units_from_u8(r.u8()?)?;
            let transform = read_transform_raw(r)?;
            Ok(Paint::RadialGradient(RadialGradient {
                center,
                radius,
                focal,
                focal_radius,
                stops,
                spread,
                units,
                transform,
            }))
        }
        other => Err(CodecError::UnknownTag(other).into()),
    }
}

fn read_transform_raw(r: &mut Reader) -> Result<Transform, CodecError> {
    Ok(Transform {
        m00: r.fixed()?,
        m01: r.fixed()?,
        tx: r.fixed()?,
        m10: r.fixed()?,
        m11: r.fixed()?,
        ty: r.fixed()?,
    })
}

pub(super) fn write_transform<W: ByteSink>(out: &mut W, t: Transform) {
    for f in [t.m00, t.m01, t.tx, t.m10, t.m11, t.ty] {
        write_fixed(out, f);
    }
}

pub(super) fn write_transform_3d<W: ByteSink>(out: &mut W, t: Transform3D) {
    for value in [
        t.m00, t.m01, t.m02, t.m10, t.m11, t.m12, t.m20, t.m21, t.m22,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_quad<W: ByteSink>(out: &mut W, q: &[Point; 4]) {
    for p in q {
        write_point(out, *p);
    }
}

pub(super) fn write_resource_ref<W: ByteSink>(out: &mut W, r: &ResourceRef) {
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
        ResourceRef::Inline(p) => {
            out.push(RES_KIND_INLINE);
            write_path(out, &p.cmds);
        }
    }
}

fn read_resource_ref_with<A: DecodeAllocator>(
    r: &mut Reader,
    allocator: &mut A,
) -> Result<ResourceRef, A::Error> {
    match r.u8()? {
        RES_KIND_INDEX => Ok(ResourceRef::Index(r.u32()?)),
        RES_KIND_TOKEN => {
            let len = r.varuint()? as usize;
            let bytes = r.take(len)?;
            let value = core::str::from_utf8(bytes).map_err(|_| CodecError::BadUtf8)?;
            Ok(ResourceRef::Token(allocator.copy_string(value)?))
        }
        RES_KIND_INLINE => Ok(ResourceRef::Inline(Path::from_cmds(read_path_with(
            r, allocator,
        )?))),
        other => Err(CodecError::BadResourceKind(other).into()),
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

fn composite_to_u8(m: CompositeMode) -> u8 {
    match m {
        CompositeMode::SourceOver => COMPOSITE_SOURCE_OVER,
        CompositeMode::Add => COMPOSITE_ADD,
        CompositeMode::Screen => COMPOSITE_SCREEN,
        CompositeMode::Multiply => COMPOSITE_MULTIPLY,
        CompositeMode::Darken => COMPOSITE_DARKEN,
        CompositeMode::Lighten => COMPOSITE_LIGHTEN,
        CompositeMode::Difference => COMPOSITE_DIFFERENCE,
    }
}

fn composite_from_u8(b: u8) -> Result<CompositeMode, CodecError> {
    match b {
        COMPOSITE_SOURCE_OVER => Ok(CompositeMode::SourceOver),
        COMPOSITE_ADD => Ok(CompositeMode::Add),
        COMPOSITE_SCREEN => Ok(CompositeMode::Screen),
        COMPOSITE_MULTIPLY => Ok(CompositeMode::Multiply),
        COMPOSITE_DARKEN => Ok(CompositeMode::Darken),
        COMPOSITE_LIGHTEN => Ok(CompositeMode::Lighten),
        COMPOSITE_DIFFERENCE => Ok(CompositeMode::Difference),
        other => Err(CodecError::BadComposite(other)),
    }
}

fn write_path<W: ByteSink>(out: &mut W, cmds: &[PathCmd]) {
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

fn read_path_with<A: DecodeAllocator>(
    r: &mut Reader,
    allocator: &mut A,
) -> Result<Vec<PathCmd>, A::Error> {
    let count = r.varuint()? as usize;
    let mut cmds = Vec::new();
    allocator.reserve(&mut cmds, count)?;
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
            other => return Err(CodecError::UnknownTag(other).into()),
        };
        cmds.push(cmd);
    }
    Ok(cmds)
}

pub(super) fn write_op<W: ByteSink>(out: &mut W, op: &SceneOp) -> Result<(), CodecError> {
    match op {
        SceneOp::GroupBegin { .. } | SceneOp::GroupEnd => {
            unreachable!("group ops are dispatched by encode, not write_op")
        }
        SceneOp::FillPath {
            path,
            transform,
            paint,
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
            write_path(out, &path.cmds);
            write_paint(out, paint);
            out.push(*opa);
            out.push(fill_rule_to_u8(*fill_rule));
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::StrokePath {
            path,
            transform,
            paint,
            width,
            opa,
            line_cap,
            line_join,
            miter_limit,
            dash,
        } => {
            out.push(TAG_STROKE_PATH);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_path(out, &path.cmds);
            write_paint(out, paint);
            write_fixed(out, *width);
            out.push(*opa);
            out.push(line_cap_to_u8(*line_cap));
            out.push(line_join_to_u8(*line_join));
            write_fixed(out, *miter_limit);
            out.extend_from_slice(&(dash.len() as u32).to_le_bytes());
            for d in dash.iter() {
                write_fixed(out, *d);
            }
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::PushClip {
            path,
            transform,
            fill_rule,
        } => {
            out.push(TAG_PUSH_CLIP);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_path(out, &path.cmds);
            out.push(fill_rule_to_u8(*fill_rule));
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::PopClip => {
            out.push(TAG_POP_CLIP);
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
        SceneOp::GlyphRun {
            font,
            ppem,
            pos,
            transform,
            color,
            opa,
            glyphs,
        } => {
            if *ppem == 0 {
                return Err(CodecError::InvalidPpem);
            }
            out.push(TAG_GLYPH_RUN);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_resource_ref(out, font);
            out.extend_from_slice(&ppem.to_le_bytes());
            write_point(out, *pos);
            write_color(out, *color);
            out.push(*opa);
            write_varuint(out, glyphs.len() as u32);
            for glyph in glyphs {
                out.extend_from_slice(&glyph.glyph_id().to_le_bytes());
                write_point(out, glyph.origin());
                write_point(out, glyph.offset());
            }
            if bits & FIELD_TRANSFORM != 0 {
                write_transform(out, *transform);
            }
            Ok(())
        }
        SceneOp::PosedGlyphRun {
            font,
            ppem,
            pos,
            transform,
            color,
            opa,
            glyphs,
        } => {
            if *ppem == 0 {
                return Err(CodecError::InvalidPpem);
            }
            if glyphs.iter().any(|glyph| !glyph.has_unit_tangent()) {
                return Err(CodecError::InvalidGlyphDirection);
            }
            out.push(TAG_POSED_GLYPH_RUN);
            let bits = if transform.is_identity() {
                0
            } else {
                FIELD_TRANSFORM
            };
            out.push(bits);
            write_resource_ref(out, font);
            out.extend_from_slice(&ppem.to_le_bytes());
            write_point(out, *pos);
            write_color(out, *color);
            out.push(*opa);
            write_varuint(out, glyphs.len() as u32);
            for glyph in glyphs {
                out.extend_from_slice(&glyph.glyph_id().to_le_bytes());
                write_point(out, glyph.origin());
                write_point(out, glyph.tangent());
            }
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
            radius,
            composite,
        } => {
            out.push(TAG_BLIT);
            let mut bits = field_bits(transform, quad, Some(*radius));
            if *opa != 255 {
                bits |= FIELD_ALPHA;
            }
            if !matches!(composite, CompositeMode::SourceOver) {
                bits |= FIELD_COMPOSITE;
            }
            out.push(bits);
            write_resource_ref(out, texture);
            write_point(out, *pos);
            write_point(out, *size);
            write_optional(out, bits, transform, quad, Some(*radius));
            if bits & FIELD_ALPHA != 0 {
                out.push(*opa);
            }
            if bits & FIELD_COMPOSITE != 0 {
                out.push(composite_to_u8(*composite));
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
    if matches!(radius, Some(r) if r != Fixed::ZERO) {
        bits |= FIELD_RADIUS;
    }
    bits
}

fn write_optional<W: ByteSink>(
    out: &mut W,
    bits: u8,
    transform: &Transform,
    quad: &Option<[Point; 4]>,
    radius: Option<Fixed>,
) {
    if bits & FIELD_TRANSFORM != 0 {
        write_transform(out, *transform);
    }
    if let (true, Some(q)) = (bits & FIELD_QUAD != 0, quad) {
        write_quad(out, q);
    }
    if let (true, Some(r)) = (bits & FIELD_RADIUS != 0, radius) {
        write_fixed(out, r);
    }
}

fn read_op_with<A: DecodeAllocator>(
    r: &mut Reader,
    tag: u8,
    allocator: &mut A,
) -> Result<SceneOp, A::Error> {
    match tag {
        TAG_FILL_PATH => {
            let bits = r.u8()?;
            let path = Path::from_cmds(read_path_with(r, allocator)?);
            let paint = read_paint_with(r, allocator)?;
            let opa = r.u8()?;
            let fill_rule = fill_rule_from_u8(r.u8()?)?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
            })
        }
        TAG_STROKE_PATH => {
            let bits = r.u8()?;
            let path = Path::from_cmds(read_path_with(r, allocator)?);
            let paint = read_paint_with(r, allocator)?;
            let width = r.fixed()?;
            let opa = r.u8()?;
            let line_cap = line_cap_from_u8(r.u8()?)?;
            let line_join = line_join_from_u8(r.u8()?)?;
            let miter_limit = r.fixed()?;
            let dash_count = r.u32()? as usize;
            let mut dash = Vec::new();
            allocator.reserve(&mut dash, dash_count)?;
            for _ in 0..dash_count {
                dash.push(r.fixed()?);
            }
            let dash = alloc::borrow::Cow::Owned(dash);
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            })
        }
        TAG_PUSH_CLIP => {
            let bits = r.u8()?;
            let path = Path::from_cmds(read_path_with(r, allocator)?);
            let fill_rule = fill_rule_from_u8(r.u8()?)?;
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::PushClip {
                path,
                transform,
                fill_rule,
            })
        }
        TAG_POP_CLIP => Ok(SceneOp::PopClip),
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
        TAG_GLYPH_RUN => {
            let bits = r.u8()?;
            let font = read_resource_ref_with(r, allocator)?;
            let ppem = r.u16()?;
            if ppem == 0 {
                return Err(CodecError::InvalidPpem.into());
            }
            let pos = r.point()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let count = r.varuint()? as usize;
            let mut glyphs = Vec::new();
            allocator.reserve(&mut glyphs, count)?;
            for _ in 0..count {
                glyphs.push(GlyphPlacement::new(r.u16()?, r.point()?).with_offset(r.point()?));
            }
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::GlyphRun {
                font,
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
            })
        }
        TAG_POSED_GLYPH_RUN => {
            let bits = r.u8()?;
            let font = read_resource_ref_with(r, allocator)?;
            let ppem = r.u16()?;
            if ppem == 0 {
                return Err(CodecError::InvalidPpem.into());
            }
            let pos = r.point()?;
            let color = r.color()?;
            let opa = r.u8()?;
            let count = r.varuint()? as usize;
            let mut glyphs = Vec::new();
            allocator.reserve(&mut glyphs, count)?;
            for _ in 0..count {
                let glyph = GlyphPose::new(r.u16()?, r.point()?, r.point()?);
                if !glyph.has_unit_tangent() {
                    return Err(CodecError::InvalidGlyphDirection.into());
                }
                glyphs.push(glyph);
            }
            let transform = read_transform_opt(r, bits)?;
            Ok(SceneOp::PosedGlyphRun {
                font,
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
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
            let texture = read_resource_ref_with(r, allocator)?;
            let pos = r.point()?;
            let size = r.point()?;
            let (transform, quad, radius_opt) = read_optional(r, bits)?;
            let opa = if bits & FIELD_ALPHA != 0 {
                r.u8()?
            } else {
                255
            };
            let composite = if bits & FIELD_COMPOSITE != 0 {
                composite_from_u8(r.u8()?)?
            } else {
                CompositeMode::SourceOver
            };
            Ok(SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
                radius: radius_opt.unwrap_or(Fixed::ZERO),
                composite,
            })
        }
        TAG_GROUP_BEGIN | TAG_GROUP_END => {
            unreachable!("group tags are dispatched by decode, not read_op")
        }
        other => Err(CodecError::UnknownTag(other).into()),
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

fn decode_body_with<A: DecodeAllocator>(
    body: &[u8],
    decoded_ops: Option<usize>,
    allocator: &mut A,
) -> Result<Scene, A::Error> {
    let mut r = Reader::new(body);
    let mut ops = Vec::new();
    if let Some(count) = decoded_ops {
        allocator.reserve(&mut ops, count)?;
    }
    let mut depth = 0usize;
    loop {
        let tag_pos = r.pos;
        let tag = r.u8()?;
        match tag {
            TAG_EOF => {
                if depth != 0 {
                    return Err(CodecError::UnbalancedGroup.into());
                }
                return Ok(Scene { ops });
            }
            TAG_GROUP_BEGIN => {
                let bits = r.varuint()?;
                let target = r.u32()? as usize;
                if target <= tag_pos || target > body.len() || body[target - 1] != TAG_GROUP_END {
                    return Err(CodecError::BadSkipOffset.into());
                }
                let transform = if bits & SLOT_TRANSFORM != 0 {
                    Some(r.transform()?)
                } else {
                    None
                };
                let projective = if bits & SLOT_PROJECTIVE != 0 {
                    Some(r.transform_3d()?)
                } else {
                    None
                };
                let opacity = if bits & SLOT_OPACITY != 0 {
                    Some(r.u8()?)
                } else {
                    None
                };
                let clip = if bits & SLOT_CLIP != 0 {
                    Some(read_resource_ref_with(&mut r, allocator)?)
                } else {
                    None
                };
                let mask = if bits & SLOT_MASK != 0 {
                    Some(read_resource_ref_with(&mut r, allocator)?)
                } else {
                    None
                };
                let filter = if bits & SLOT_FILTER != 0 {
                    Some(read_resource_ref_with(&mut r, allocator)?)
                } else {
                    None
                };
                let disjoint_hint = bits & SLOT_DISJOINT_HINT != 0;
                depth += 1;
                ops.push(SceneOp::GroupBegin {
                    transform,
                    projective,
                    opacity,
                    clip,
                    mask,
                    filter,
                    disjoint_hint,
                });
            }
            TAG_GROUP_END => {
                if depth == 0 {
                    return Err(CodecError::UnbalancedGroup.into());
                }
                depth -= 1;
                ops.push(SceneOp::GroupEnd);
            }
            0x40..=0x7F => {
                let len = r.varuint()? as usize;
                let _ = r.take(len)?;
            }
            _ => ops.push(read_op_with(&mut r, tag, allocator)?),
        }
    }
}

impl Scene {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let mut body = Vec::new();
        let mut group_stack: Vec<usize> = Vec::new();
        for op in &self.ops {
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
                    body.push(TAG_GROUP_BEGIN);
                    let mut bits = 0u32;
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
                    if *disjoint_hint {
                        bits |= SLOT_DISJOINT_HINT;
                    }
                    write_varuint(&mut body, bits);
                    let patch_pos = body.len();
                    body.extend_from_slice(&0u32.to_le_bytes());
                    if let Some(t) = transform {
                        write_transform(&mut body, *t);
                    }
                    if let Some(value) = projective.filter(|value| !value.is_identity()) {
                        write_transform_3d(&mut body, value);
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

        let crc = crc32::compute(&body);
        let mut out = Vec::with_capacity(VectorChunkHeader::SIZE + body.len());
        out.push(VectorChunkHeader::MAGIC);
        out.push(VERSION);
        out.push(DEFAULT_SCALE);
        out.push(0);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode(payload: &[u8]) -> Result<Self, CodecError> {
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
        let actual_crc = crc32::compute(body);
        if stored_crc != actual_crc {
            return Err(CodecError::CrcMismatch {
                expected: stored_crc,
                actual: actual_crc,
            });
        }

        decode_body_with(body, None, &mut LegacyDecodeAllocator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn red() -> Color {
        Color::rgb(255, 0, 0)
    }

    fn roundtrip(ops: Vec<SceneOp>) {
        let scene = Scene { ops: ops.clone() };
        let bytes = scene.encode().unwrap();
        let back = Scene::decode(&bytes).unwrap();
        assert_eq!(back.ops, ops);
    }

    #[test]
    fn fill_rect_identity_roundtrips() {
        roundtrip(vec![SceneOp::FillRect {
            area: Rect::new(
                Fixed::from_int(1),
                Fixed::from_int(2),
                Fixed::from_int(3),
                Fixed::from_int(4),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: red(),
            radius: Fixed::ZERO,
            opa: 200,
        }]);
    }

    #[test]
    fn positioned_glyph_run_roundtrips() {
        roundtrip(vec![SceneOp::GlyphRun {
            font: ResourceRef::Index(7),
            ppem: 24,
            pos: Point::new(Fixed::from_int(2), Fixed::from_int(3)),
            transform: Transform::translate(Fixed::ONE, Fixed::from_int(2)),
            color: red(),
            opa: 211,
            glyphs: vec![
                GlyphPlacement::new(53, Point::new(Fixed::from_int(1), Fixed::from_int(18))),
                GlyphPlacement::new(54, Point::new(Fixed::from_int(12), Fixed::from_int(18)))
                    .with_offset(Point::new(
                        Fixed::from_ratio(1, 2),
                        Fixed::from_ratio(-1, 2),
                    )),
            ],
        }]);
    }

    #[test]
    fn projective_group_roundtrips() {
        let projective = Transform3D {
            m20: Fixed64::from_ratio(1, 640),
            m21: Fixed64::from_ratio(-1, 960),
            ..Transform3D::IDENTITY
        };
        roundtrip(vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(projective),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupEnd,
        ]);
    }

    #[test]
    fn posed_glyph_run_roundtrips() {
        let scene = Scene::from_ops(vec![SceneOp::PosedGlyphRun {
            font: ResourceRef::Index(4),
            ppem: 18,
            pos: Point::new(Fixed::from_int(2), Fixed::from_int(3)),
            transform: Transform::translate(Fixed::ONE, Fixed::from_int(2)),
            color: red(),
            opa: 207,
            glyphs: vec![GlyphPose::new(
                71,
                Point::new(Fixed::from_int(12), Fixed::from_int(8)),
                Point::new(Fixed::ZERO, Fixed::ONE),
            )],
        }]);
        roundtrip(scene.ops.clone());

        let payload = scene.encode().unwrap();
        let mut unaligned = vec![0xa5];
        unaligned.extend_from_slice(&payload);
        assert_eq!(Scene::decode(&unaligned[1..]).unwrap(), scene);
    }

    #[test]
    fn positioned_glyph_run_rejects_zero_ppem() {
        let scene = Scene::from_ops(vec![SceneOp::GlyphRun {
            font: ResourceRef::Index(0),
            ppem: 0,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: red(),
            opa: 255,
            glyphs: Vec::new(),
        }]);

        assert_eq!(scene.encode(), Err(CodecError::InvalidPpem));
        assert!(matches!(
            scene.encode_payload(),
            Err(VectorEncodeError::InvalidPayload(VectorReadError::Codec(
                CodecError::InvalidPpem
            )))
        ));
    }

    #[test]
    fn blit_per_composite_mode_roundtrips() {
        for m in [
            CompositeMode::Add,
            CompositeMode::Screen,
            CompositeMode::Multiply,
            CompositeMode::Darken,
            CompositeMode::Lighten,
            CompositeMode::Difference,
        ] {
            roundtrip(vec![SceneOp::Blit {
                texture: ResourceRef::Index(3),
                pos: Point::ZERO,
                size: Point::new(Fixed::from_int(8), Fixed::from_int(8)),
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
                radius: Fixed::ZERO,
                composite: m,
            }]);
        }
    }

    #[test]
    fn stroke_path_roundtrips() {
        for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                roundtrip(vec![SceneOp::StrokePath {
                    path: Path::from_cmds(vec![
                        PathCmd::MoveTo(Point::new(Fixed::from_int(1), Fixed::from_int(2))),
                        PathCmd::LineTo(Point::new(Fixed::from_int(10), Fixed::from_int(20))),
                        PathCmd::Close,
                    ]),
                    transform: Transform::IDENTITY,
                    paint: Paint::Color(red()),
                    width: Fixed::from_int(3),
                    opa: 200,
                    line_cap: cap,
                    line_join: join,
                    miter_limit: Fixed::from_int(4),
                    dash: alloc::borrow::Cow::Borrowed(&[]),
                }]);
            }
        }
    }

    #[test]
    fn golden_bytes_are_stable() {
        let ops = vec![
            SceneOp::FillRect {
                area: Rect::new(
                    Fixed::from_int(1),
                    Fixed::from_int(2),
                    Fixed::from_int(3),
                    Fixed::from_int(4),
                ),
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgba(0x11, 0x22, 0x33, 0x44),
                radius: Fixed::ZERO,
                opa: 0xAB,
            },
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::new(Fixed::from_int(5), Fixed::ZERO),
                transform: Transform::IDENTITY,
                color: Color::rgba(1, 2, 3, 4),
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
        let scene = Scene { ops };
        assert_eq!(scene.encode().unwrap(), golden);
        assert_eq!(Scene::decode(golden).unwrap().ops, scene.ops);
    }

    #[test]
    fn corrupt_crc_is_rejected() {
        let scene = Scene {
            ops: vec![SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::ZERO,
                transform: Transform::IDENTITY,
                color: red(),
                width: Fixed::from_int(1),
                opa: 255,
            }],
        };
        let mut bytes = scene.encode().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            Scene::decode(&bytes),
            Err(CodecError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn unbalanced_group_end_is_rejected_at_encode() {
        let scene = Scene {
            ops: vec![SceneOp::GroupEnd],
        };
        assert!(matches!(scene.encode(), Err(CodecError::UnbalancedGroup)));
    }

    #[test]
    fn unsupported_scale_is_rejected() {
        let scene = Scene::default();
        let mut bytes = scene.encode().unwrap();
        bytes[2] = 7;
        let new_crc = crc32::compute(&bytes[VectorChunkHeader::SIZE..]);
        bytes[4..8].copy_from_slice(&new_crc.to_le_bytes());
        assert!(matches!(
            Scene::decode(&bytes),
            Err(CodecError::UnsupportedScale(7))
        ));
    }
}
