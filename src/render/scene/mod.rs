//! SceneOp — owned/borrowable mirror of DrawCommand for vector persistence.

pub mod codec;
pub mod record;
pub mod replay;
pub mod resolver;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::render::path::PathCmd;
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
    },
    GroupEnd,
    FillPath {
        path: Cow<'static, [PathCmd]>,
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

    pub fn extend_slice(&mut self, ops: &[SceneOp]) -> &mut Self {
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
        });
        body(self);
        self.ops.push(SceneOp::GroupEnd);
        self
    }

    pub fn group_opacity(
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
        });
        body(self);
        self.ops.push(SceneOp::GroupEnd);
        self
    }

    pub fn into_ops(self) -> Vec<SceneOp> {
        self.ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static GEOMETRY: [PathCmd; 2] = [PathCmd::MoveTo(Point::ZERO), PathCmd::Close];
    static SCENE: &[SceneOp] = &[SceneOp::FillPath {
        path: Cow::Borrowed(&GEOMETRY),
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
                assert_eq!(path.len(), 2);
                assert_eq!(*opa, 255);
            }
            _ => panic!("expected FillPath"),
        }
    }

    #[test]
    fn owned_and_borrowed_geometry_compare_equal() {
        let owned = SceneOp::FillPath {
            path: Cow::Owned(GEOMETRY.to_vec()),
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
    fn path_macro_emits_const_slice() {
        const P: &[PathCmd] = mirui::path! {
            M 0 0;
            L 4 0;
            Q 4 4 0 4;
            Z
        };
        assert_eq!(P.len(), 4);
        assert_eq!(P[0], PathCmd::MoveTo(Point::ZERO));
        assert_eq!(P[3], PathCmd::Close);
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
            },
            SceneOp::GroupBegin {
                transform: Some(Transform::IDENTITY),
                opacity: Some(128),
                clip: None,
                mask: None,
                filter: None,
            },
            dot,
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        assert_eq!(built, manual);
    }
}
