//! SceneOp — owned/borrowable mirror of DrawCommand for vector persistence.

pub mod bbox;
pub mod codec;
pub mod record;
pub mod replay;
pub mod resolver;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::types::{Color, Fixed, Point, Rect, Transform};

/// Reference to a font / texture / subtree resource.
///
/// `Index` points into the VECTOR chunk's resource table (embedded mode);
/// `Token` resolves through the runtime `ResourceManager` (token mode).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceRef {
    Token(Cow<'static, str>),
    Index(u32),
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
        opacity: Option<u8>,
        clip: Option<ResourceRef>,
        mask: Option<ResourceRef>,
        filter: Option<ResourceRef>,
        disjoint_hint: bool,
    },
    GroupEnd,
    FillPath {
        path: Path,
        transform: Transform,
        color: Color,
        opa: u8,
        fill_rule: FillRule,
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
    Label {
        font: ResourceRef,
        pos: Point,
        transform: Transform,
        color: Color,
        opa: u8,
        text: Cow<'static, str>,
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

    /// Strict SVG `<g opacity>`: would need offscreen compositing for
    /// overlapping children, which this build doesn't have, so replay rejects
    /// overlap rather than seaming. For layered/stacked motifs use
    /// [`group_alpha_multiply`].
    pub fn group_opacity(
        &mut self,
        transform: Transform,
        opacity: u8,
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let header_idx = self.ops.len();
        self.ops.push(SceneOp::GroupBegin {
            transform: Some(transform),
            opacity: Some(opacity),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        });
        body(self);
        let inner = &self.ops[header_idx + 1..];
        let bboxes = bbox::direct_children_bboxes(inner);
        let disjoint = bbox::pairwise_disjoint(&bboxes);
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
    /// "flatten then dim" semantics use [`group_opacity`].
    pub fn group_alpha_multiply(
        &mut self,
        transform: Transform,
        opacity: u8,
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.ops.push(SceneOp::GroupBegin {
            transform: Some(transform),
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
mod tests {
    use super::*;
    use crate::render::path::{Path, PathCmd};

    static GEOMETRY: [PathCmd; 2] = [PathCmd::MoveTo(Point::ZERO), PathCmd::Close];
    static SCENE: &[SceneOp] = &[SceneOp::FillPath {
        path: Path::from_static(&GEOMETRY),
        transform: Transform::IDENTITY,
        color: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    }];

    #[test]
    fn static_scene_is_const_constructible() {
        assert_eq!(SCENE.len(), 1);
        match &SCENE[0] {
            SceneOp::FillPath { path, opa, .. } => {
                assert_eq!(path.cmds.len(), 2);
                assert_eq!(*opa, 255);
            }
            _ => panic!("expected FillPath"),
        }
    }

    #[test]
    fn owned_and_borrowed_geometry_compare_equal() {
        let owned = SceneOp::FillPath {
            path: Path::from_owned(GEOMETRY.to_vec()),
            transform: Transform::IDENTITY,
            color: Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        };
        assert_eq!(owned, SCENE[0]);
    }

    #[test]
    fn vector_chunk_header_layout() {
        assert_eq!(VectorChunkHeader::SIZE, 8);
        assert_eq!(VectorChunkHeader::MAGIC, 0x03);
    }

    #[test]
    fn path_macro_emits_borrowed_path() {
        use alloc::borrow::Cow;
        let p: Path = mirui::path!(M 0 0 L 4 0 Q 4 4 0 4 Z);
        assert!(matches!(p.cmds, Cow::Borrowed(_)));
        assert_eq!(p.cmds.len(), 4);
        assert_eq!(p.cmds[0], PathCmd::MoveTo(Point::ZERO));
        assert_eq!(p.cmds[3], PathCmd::Close);
    }

    #[test]
    fn path_macro_svg_string_uppercase_absolute() {
        let p: Path = mirui::path!("M 0 0 L 4 0 Q 4 4 0 4 Z");
        assert_eq!(p.cmds.len(), 4);
        assert_eq!(p.cmds[0], PathCmd::MoveTo(Point::ZERO));
        assert_eq!(p.cmds[3], PathCmd::Close);
    }

    #[test]
    fn path_macro_svg_string_h_v_shorthand() {
        let p: Path = mirui::path!("M 4 4 H 20 V 20 Z");
        assert_eq!(p.cmds.len(), 4);
        assert!(
            matches!(p.cmds[1], PathCmd::LineTo(pt) if pt.x.to_int() == 20 && pt.y.to_int() == 4)
        );
        assert!(
            matches!(p.cmds[2], PathCmd::LineTo(pt) if pt.x.to_int() == 20 && pt.y.to_int() == 20)
        );
    }

    #[test]
    fn path_macro_svg_string_implicit_lineto() {
        let p: Path = mirui::path!("M 0 0 1 1 2 2 m 5 5 3 3");
        assert_eq!(p.cmds.len(), 5);
        assert!(matches!(p.cmds[0], PathCmd::MoveTo(_)));
        assert!(
            matches!(p.cmds[1], PathCmd::LineTo(pt) if pt.x.to_int() == 1 && pt.y.to_int() == 1)
        );
        assert!(
            matches!(p.cmds[2], PathCmd::LineTo(pt) if pt.x.to_int() == 2 && pt.y.to_int() == 2)
        );
        assert!(
            matches!(p.cmds[3], PathCmd::MoveTo(pt) if pt.x.to_int() == 7 && pt.y.to_int() == 7)
        );
        assert!(
            matches!(p.cmds[4], PathCmd::LineTo(pt) if pt.x.to_int() == 10 && pt.y.to_int() == 10)
        );
    }

    #[test]
    fn path_macro_svg_string_close_restores_subpath_start() {
        let p: Path = mirui::path!("M 5 5 L 10 5 L 10 10 Z l 2 2");
        let last = p.cmds.last().unwrap();
        assert!(
            matches!(last, PathCmd::LineTo(pt) if pt.x.to_int() == 7 && pt.y.to_int() == 7),
            "after Z, current point must be subpath_start (5,5), then l 2 2 → (7,7); got {last:?}",
        );
    }

    #[test]
    fn path_macro_svg_string_lowercase_relative() {
        let p: Path = mirui::path!("M 10 10 l 5 0 l 0 5 z");
        assert_eq!(p.cmds.len(), 4);
        assert!(
            matches!(p.cmds[1], PathCmd::LineTo(pt) if pt.x.to_int() == 15 && pt.y.to_int() == 10)
        );
        assert!(
            matches!(p.cmds[2], PathCmd::LineTo(pt) if pt.x.to_int() == 15 && pt.y.to_int() == 15)
        );
    }

    // Quarter circle A 8 8 0 0 1 from (12,4) to (20,12) about the
    // viewBox center (12,12). Endpoint must land within 2% of radius
    // and midpoint stay on the circle within 2% as well.
    #[test]
    fn path_macro_svg_string_arc_quarter_circle() {
        let p: Path = mirui::path!("M 12 4 A 8 8 0 0 1 20 12");
        let last_end = match p.cmds.last().unwrap() {
            PathCmd::CubicTo { end, .. } => *end,
            PathCmd::LineTo(pt) => *pt,
            other => panic!("expected CubicTo/LineTo, got {other:?}"),
        };
        let dx = last_end.x.to_f32() - 20.0;
        let dy = last_end.y.to_f32() - 12.0;
        let err = (dx * dx + dy * dy).sqrt();
        assert!(err < 0.16, "endpoint error {err} > 2% of r=8 (=0.16)");
    }

    #[test]
    fn path_macro_svg_string_arc_half_circle_emits_two_segments() {
        let p: Path = mirui::path!("M 0 0 A 5 5 0 1 1 10 0");
        let cubics = p
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::CubicTo { .. }))
            .count();
        assert_eq!(
            cubics, 2,
            "180° arc must split into 2 segments (≤90° each) for ≤2% precision",
        );
    }

    fn arc_endpoint(p: &Path) -> (f32, f32) {
        match p.cmds.last().unwrap() {
            PathCmd::CubicTo { end, .. } => (end.x.to_f32(), end.y.to_f32()),
            PathCmd::LineTo(pt) => (pt.x.to_f32(), pt.y.to_f32()),
            other => panic!("expected CubicTo/LineTo, got {other:?}"),
        }
    }

    #[test]
    fn path_macro_svg_string_arc_half_circle_endpoint() {
        let p: Path = mirui::path!("M 0 0 A 5 5 0 0 1 10 0");
        let (x, y) = arc_endpoint(&p);
        let err = ((x - 10.0).powi(2) + y.powi(2)).sqrt();
        assert!(err < 0.1, "half-circle endpoint err {err} > 2% of r=5");
    }

    #[test]
    fn path_macro_svg_string_arc_large_flag_endpoint() {
        let p: Path = mirui::path!("M 5 0 A 5 5 0 1 1 0 5");
        let (x, y) = arc_endpoint(&p);
        let err = (x.powi(2) + (y - 5.0).powi(2)).sqrt();
        assert!(err < 0.1, "270° (large_arc=1) endpoint err {err} > 2%");
    }

    #[test]
    fn path_macro_svg_string_arc_sweep_zero_endpoint() {
        let p: Path = mirui::path!("M 0 0 A 5 5 0 0 0 10 0");
        let (x, y) = arc_endpoint(&p);
        let err = ((x - 10.0).powi(2) + y.powi(2)).sqrt();
        assert!(err < 0.1, "sweep=0 endpoint err {err} > 2% of r=5");
    }

    #[test]
    fn scene_macro_emits_const_and_roundtrips() {
        const OPS: &[SceneOp] = mirui::scene! {
            group 10 20;
            rect 0 0 32 16 4 255 100 50 255 200;
            line 0 0 32 16 1.5 0 0 0 255 255;
            endgroup;
            arc 50 50 20 0 90 2 10 20 30 255 128
        };
        assert_eq!(OPS.len(), 5);
        let bytes = codec::encode_scene(OPS).unwrap();
        let back = codec::decode_scene(&bytes).unwrap();
        assert_eq!(back, OPS);
    }

    #[test]
    fn scene_macro_supports_border_fillpath_label_blit() {
        const OPS: &[SceneOp] = mirui::scene! {
            border 0 0 64 32 2 4 200 200 200 255 255;
            fill_path { M 0 0; L 8 0; L 8 8; Z } 255 0 0 255 200;
            label "noto-sans" 10 20 0 0 0 255 255 "hi";
            blit "thumb-1" 0 0 16 16
        };
        assert_eq!(OPS.len(), 4);
        assert!(matches!(OPS[0], SceneOp::Border { .. }));
        assert!(matches!(OPS[1], SceneOp::FillPath { .. }));
        assert!(matches!(OPS[2], SceneOp::Label { .. }));
        assert!(matches!(OPS[3], SceneOp::Blit { .. }));

        let bytes = codec::encode_scene(OPS).unwrap();
        let back = codec::decode_scene(&bytes).unwrap();
        assert_eq!(back, OPS);
    }

    fn group_transform(op: &SceneOp) -> Transform {
        match op {
            SceneOp::GroupBegin { transform, .. } => transform.unwrap(),
            _ => panic!("expected GroupBegin"),
        }
    }

    #[test]
    fn macro_group_bare_is_translate() {
        const OPS: &[SceneOp] = mirui::scene! { group 10 20; endgroup };
        assert_eq!(
            group_transform(&OPS[0]),
            Transform::translate(Fixed::from_int(10), Fixed::from_int(20))
        );
    }

    #[test]
    fn macro_group_rotate_90_folds_matrix() {
        const OPS: &[SceneOp] = mirui::scene! { group rotate 90; endgroup };
        let t = group_transform(&OPS[0]);
        assert_eq!(t.m00, Fixed::ZERO);
        assert_eq!(t.m01, Fixed::ZERO - Fixed::ONE);
        assert_eq!(t.m10, Fixed::ONE);
        assert_eq!(t.m11, Fixed::ZERO);
    }

    #[test]
    fn macro_group_chain_composes_left_to_right() {
        const OPS: &[SceneOp] = mirui::scene! { group translate 100 50 scale 2 2; endgroup };
        let t = group_transform(&OPS[0]);
        assert_eq!(t.m00, Fixed::from_int(2));
        assert_eq!(t.m11, Fixed::from_int(2));
        assert_eq!(t.tx, Fixed::from_int(100));
        assert_eq!(t.ty, Fixed::from_int(50));
    }

    #[test]
    fn macro_group_opacity_token_emits_slot() {
        const OPS: &[SceneOp] = mirui::scene! { group translate 5 5 opacity 128; endgroup };
        match &OPS[0] {
            SceneOp::GroupBegin {
                opacity, transform, ..
            } => {
                assert_eq!(*opacity, Some(128));
                assert!(transform.is_some());
            }
            _ => panic!("expected GroupBegin"),
        }
    }

    #[test]
    fn macro_group_opacity_alone_works() {
        const OPS: &[SceneOp] = mirui::scene! { group opacity 200; endgroup };
        match &OPS[0] {
            SceneOp::GroupBegin { opacity, .. } => assert_eq!(*opacity, Some(200)),
            _ => panic!("expected GroupBegin"),
        }
    }

    #[test]
    fn builder_groups_auto_balance_and_match_manual() {
        let t = Transform::translate(Fixed::from_int(5), Fixed::ZERO);
        let dot = SceneOp::FillRect {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(2),
                h: Fixed::from_int(2),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            radius: Fixed::ZERO,
            opa: 255,
        };

        let mut s = Scene::new();
        s.group(t, |s| {
            s.group_opacity(Transform::IDENTITY, 128, |s| {
                s.push(dot.clone());
            });
        });
        let built = s.into_ops();

        let manual = alloc::vec![
            SceneOp::GroupBegin {
                transform: Some(t),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupBegin {
                transform: Some(Transform::IDENTITY),
                opacity: Some(128),
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: true,
            },
            dot,
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        assert_eq!(built, manual);
    }

    #[test]
    fn scene_methods_match_free_fns() {
        let ops = vec![SceneOp::Line {
            p1: Point::ZERO,
            p2: Point {
                x: Fixed::from_int(4),
                y: Fixed::from_int(4),
            },
            transform: Transform::IDENTITY,
            color: Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            width: Fixed::from_int(1),
            opa: 255,
        }];

        let scene = Scene { ops: ops.clone() };
        let bytes_method = scene.encode().unwrap();
        let bytes_fn = codec::encode_scene(&ops).unwrap();
        assert_eq!(bytes_method, bytes_fn);

        let back = Scene::decode(&bytes_method).unwrap();
        assert_eq!(back.ops, ops);
    }

    #[test]
    fn record_stream_accumulates_a_frame() {
        use crate::render::command::DrawCommand;
        use crate::render::font::Font;
        use crate::render::texture::Texture;

        struct Stub;
        impl record::ResourceResolver for Stub {
            fn resolve_font(&mut self, _: &Font) -> ResourceRef {
                unreachable!()
            }
            fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
                unreachable!()
            }
        }

        let frame = [
            DrawCommand::Line {
                p1: Point::ZERO,
                p2: Point::ZERO,
                transform: Transform::IDENTITY,
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                width: Fixed::from_int(1),
                opa: 0,
            },
            DrawCommand::Line {
                p1: Point::ZERO,
                p2: Point {
                    x: Fixed::from_int(1),
                    y: Fixed::ZERO,
                },
                transform: Transform::IDENTITY,
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                width: Fixed::from_int(1),
                opa: 0,
            },
        ];

        let mut scene = Scene::new();
        scene.record_stream(&frame, &mut Stub).unwrap();
        assert_eq!(scene.ops.len(), 2);
        assert!(matches!(scene.ops[0], SceneOp::Line { .. }));
        assert!(matches!(scene.ops[1], SceneOp::Line { .. }));
    }
}
