use alloc::vec::Vec;

use crate::render::scene::SceneOp;

pub use mirx::scene::CodecError;

pub fn encode_scene(ops: &[SceneOp]) -> Result<Vec<u8>, CodecError> {
    let mirx_ops: alloc::vec::Vec<mirx::scene::SceneOp> =
        ops.iter().cloned().map(Into::into).collect();
    mirx::scene::Scene::from_ops(mirx_ops).encode()
}

pub fn decode_scene(payload: &[u8]) -> Result<Vec<SceneOp>, CodecError> {
    let scene = mirx::scene::Scene::decode(payload)?;
    Ok(scene.ops.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::command::CompositeMode;
    use crate::render::path::Path;
    use crate::render::raster::FillRule;
    use crate::render::scene::{Paint, ResourceRef};
    use crate::types::{Color, Fixed, Point, Rect, Transform};
    use alloc::vec;

    fn red() -> Color {
        Color::rgb(255, 0, 0)
    }

    fn roundtrip(ops: vec::Vec<SceneOp>) {
        let bytes = encode_scene(&ops).unwrap();
        let back = decode_scene(&bytes).unwrap();
        assert_eq!(back, ops);
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
        static GLYPHS: [textflow::shaping::PositionedGlyph; 1] =
            [textflow::shaping::PositionedGlyph::new(
                crate::render::font::GlyphId::new(65),
                textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
            )];
        let ops = [SceneOp::GlyphRun {
            font: ResourceRef::Index(0),
            ppem: 18,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: red(),
            opa: 255,
            glyphs: (&GLYPHS[..]).into(),
        }];

        roundtrip(ops.into());
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
        assert_eq!(encode_scene(&ops).unwrap(), golden);
        assert_eq!(decode_scene(golden).unwrap(), ops);
    }

    #[test]
    fn fill_path_roundtrips() {
        roundtrip(vec![SceneOp::FillPath {
            path: Path::from_owned(vec![
                crate::render::path::PathCmd::MoveTo(Point::ZERO),
                crate::render::path::PathCmd::LineTo(Point::new(Fixed::from_int(4), Fixed::ZERO)),
                crate::render::path::PathCmd::QuadTo {
                    ctrl: Point::new(Fixed::from_int(4), Fixed::from_int(4)),
                    end: Point::new(Fixed::ZERO, Fixed::from_int(4)),
                },
                crate::render::path::PathCmd::Close,
            ]),
            transform: Transform::IDENTITY,
            paint: Paint::Color(red().into()),
            opa: 255,
            fill_rule: FillRule::NonZero,
        }]);
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
    fn unbalanced_group_end_is_rejected_at_encode() {
        assert!(matches!(
            encode_scene(&[SceneOp::GroupEnd]),
            Err(CodecError::UnbalancedGroup)
        ));
    }
}
