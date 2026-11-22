//! SceneOp — owned/borrowable mirror of DrawCommand for vector persistence.

pub mod bbox;
pub mod codec;
pub mod record;
pub mod replay;
pub mod resolver;

pub use codec::CodecError;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::render::command::CompositeMode;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::types::{Color, Fixed, Point, Rect, Transform};

pub use mirx::{
    scene::{
        GradientStop, GradientUnits, LineCap, LineJoin, LinearGradient, Paint, RadialGradient,
        SpreadMode,
    },
    types::{Color as MirxColor, Fixed as MirxFixed},
};

/// Reference to a font / texture / subtree resource.
///
/// `Index` points into the VECTOR chunk's resource table (embedded mode);
/// `Token` resolves through the runtime `ResourceManager` (token mode).
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceRef {
    Token(Cow<'static, str>),
    Index(u32),
    Inline(Path),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PosedGlyphBuffer {
    glyphs: Cow<'static, [textflow::shaping::PositionedGlyph]>,
    frames: Cow<'static, [textflow::placement::GlyphFrame]>,
}

impl PosedGlyphBuffer {
    pub fn new(
        glyphs: impl Into<Cow<'static, [textflow::shaping::PositionedGlyph]>>,
        frames: impl Into<Cow<'static, [textflow::placement::GlyphFrame]>>,
    ) -> Option<Self> {
        let glyphs = glyphs.into();
        let frames = frames.into();
        (glyphs.len() == frames.len()).then_some(Self { glyphs, frames })
    }

    pub fn glyphs(&self) -> &[textflow::shaping::PositionedGlyph] {
        &self.glyphs
    }

    pub fn frames(&self) -> &[textflow::placement::GlyphFrame] {
        &self.frames
    }

    pub fn as_draw(&self) -> crate::render::PosedGlyphs<'_> {
        crate::render::PosedGlyphs::new(self.glyphs(), self.frames())
            .expect("scene glyph pose lengths are validated at construction")
    }
}

/// One drawing operation. Owned (deser / reactive / recorded) or borrowed
/// (macro-emitted `&'static`) geometry via `Cow`.
///
/// Every draw variant carries its own `transform`; Fill/Border/Blit carry
/// `quad`; this mirrors `DrawCommand` field-for-field so a recorded live
/// frame round-trips losslessly. Group transform is layered on top via
/// `GroupBegin` for SVG `<g>` nesting.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneOp {
    GroupBegin {
        transform: Option<Transform>,
        projective: Option<crate::types::Transform3D>,
        opacity: Option<u8>,
        clip: Option<ResourceRef>,
        mask: Option<ResourceRef>,
        filter: Option<ResourceRef>,
        disjoint_hint: bool,
    },
    GroupEnd,
    PushClip {
        path: Path,
        transform: Transform,
        fill_rule: FillRule,
    },
    PopClip,
    FillPath {
        path: Path,
        transform: Transform,
        paint: Paint,
        opa: u8,
        fill_rule: FillRule,
    },
    StrokePath {
        path: Path,
        transform: Transform,
        paint: Paint,
        width: Fixed,
        opa: u8,
        line_cap: LineCap,
        line_join: LineJoin,
        miter_limit: Fixed,
        dash: Cow<'static, [Fixed]>,
    },
    FillRect {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        radius: Fixed,
        opa: u8,
    },
    Border {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
    },
    GlyphRun {
        font: ResourceRef,
        ppem: u16,
        pos: Point,
        transform: Transform,
        color: Color,
        opa: u8,
        glyphs: Cow<'static, [textflow::shaping::PositionedGlyph]>,
    },
    PosedGlyphRun {
        font: ResourceRef,
        ppem: u16,
        pos: Point,
        transform: Transform,
        color: Color,
        opa: u8,
        glyphs: PosedGlyphBuffer,
    },
    Line {
        p1: Point,
        p2: Point,
        transform: Transform,
        color: Color,
        width: Fixed,
        opa: u8,
    },
    Arc {
        center: Point,
        transform: Transform,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
        opa: u8,
    },
    Blit {
        texture: ResourceRef,
        pos: Point,
        size: Point,
        transform: Transform,
        quad: Option<[Point; 4]>,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    },
}

/// Fixed-size prefix of a `chunk_type::VECTOR` chunk payload.
///
/// mirx's container CRC only covers its fixed file header, not chunk
/// payloads, so this header carries its own `payload_crc32` over every byte
/// after it — bit rot in a length field otherwise walks the parser off a
/// cliff undetected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorChunkHeader {
    pub magic: u8,
    pub version: u8,
    pub scale: u8,
    pub flags: u8,
    pub payload_crc32: u32,
}

impl VectorChunkHeader {
    pub const MAGIC: u8 = 0x03;
    pub const SIZE: usize = 8;
    /// Reserved for a not-yet-specified in-VECTOR resource table. Decoders
    /// in this revision reject any non-zero flag byte, so writers must keep
    /// this clear until the table format lands.
    pub const FLAG_HAS_RESOURCE_TABLE: u8 = 1 << 0;
}

/// An in-memory scene: a flat op stream with virtual-tree group markers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub ops: Vec<SceneOp>,
}

impl Scene {
    pub const fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn push(&mut self, op: SceneOp) -> &mut Self {
        self.ops.push(op);
        self
    }

    pub fn extend_from_slice(&mut self, ops: &[SceneOp]) -> &mut Self {
        self.ops.extend_from_slice(ops);
        self
    }

    /// The matching `GroupEnd` is emitted on close, so callers can't leave
    /// the group stack unbalanced.
    pub fn group(&mut self, transform: Transform, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.ops.push(SceneOp::GroupBegin {
            transform: Some(transform),
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        });
        body(self);
        self.ops.push(SceneOp::GroupEnd);
        self
    }

    /// Flatten the group before applying `opacity`. Overlapping children use
    /// caller-owned RGBA storage supplied to scene replay; disjoint children
    /// can apply the same opacity without an intermediate layer.
    pub fn group_opacity(
        &mut self,
        transform: Transform,
        opacity: u8,
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let header_idx = self.ops.len();
        self.ops.push(SceneOp::GroupBegin {
            transform: Some(transform),
            projective: None,
            opacity: Some(opacity),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        });
        body(self);
        let inner = &self.ops[header_idx + 1..];
        let disjoint = bbox::children_disjoint(inner).unwrap_or(false);
        if disjoint {
            if let Some(SceneOp::GroupBegin { disjoint_hint, .. }) = self.ops.get_mut(header_idx) {
                *disjoint_hint = true;
            }
        }
        self.ops.push(SceneOp::GroupEnd);
        self
    }

    /// Multiplies `opacity` into each child's alpha at replay — children
    /// stay independently transparent, no implicit flatten. For SVG-style
    /// "flatten then dim" semantics use [`group_opacity`](Self::group_opacity).
    pub fn group_alpha_multiply(
        &mut self,
        transform: Transform,
        opacity: u8,
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.ops.push(SceneOp::GroupBegin {
            transform: Some(transform),
            projective: None,
            opacity: Some(opacity),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: true,
        });
        body(self);
        self.ops.push(SceneOp::GroupEnd);
        self
    }

    pub fn into_ops(self) -> Vec<SceneOp> {
        self.ops
    }

    pub fn encode(&self) -> Result<Vec<u8>, codec::CodecError> {
        codec::encode_scene(&self.ops)
    }

    pub fn decode(payload: &[u8]) -> Result<Self, codec::CodecError> {
        codec::decode_scene(payload).map(|ops| Self { ops })
    }

    pub fn replay(
        &self,
        renderer: &mut dyn crate::render::renderer::Renderer,
        clip: &crate::types::Rect,
        resolver: &dyn replay::SceneResolver,
    ) -> Result<(), replay::ReplayError> {
        replay::replay_scene(&self.ops, renderer, clip, resolver)
    }

    pub fn replay_with_workspace(
        &self,
        renderer: &mut dyn crate::render::renderer::Renderer,
        clip: &crate::types::Rect,
        resolver: &dyn replay::SceneResolver,
        frames: &mut [replay::ReplayFrame],
    ) -> Result<(), replay::ReplayError> {
        replay::replay_scene_with_workspace(&self.ops, renderer, clip, resolver, frames)
    }

    pub fn replay_with_scratch(
        &self,
        renderer: &mut dyn crate::render::renderer::Renderer,
        clip: &crate::types::Rect,
        resolver: &dyn replay::SceneResolver,
        scratch: replay::ReplayScratch<'_>,
    ) -> Result<(), replay::ReplayError> {
        replay::replay_scene_with_scratch(&self.ops, renderer, clip, resolver, scratch)
    }

    pub fn record(
        &mut self,
        cmd: &crate::render::command::DrawCommand,
        resolver: &mut dyn record::ResourceResolver,
    ) -> Result<&mut Self, record::RecordError> {
        let op = record::record_command(cmd, resolver)?;
        self.ops.push(op);
        Ok(self)
    }

    pub fn record_stream(
        &mut self,
        cmds: &[crate::render::command::DrawCommand],
        resolver: &mut dyn record::ResourceResolver,
    ) -> Result<&mut Self, record::RecordError> {
        for cmd in cmds {
            self.record(cmd, resolver)?;
        }
        Ok(self)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/scene/mod.rs"]
mod tests;
