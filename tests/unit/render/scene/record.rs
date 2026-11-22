use super::*;
use crate::render::font::GlyphId;
use crate::render::path::{Path, PathCmd};
use crate::render::scene::codec::{decode_scene, encode_scene};
use crate::types::{Color, Fixed, Point, Rect, Transform};
use alloc::vec;

struct PanicResolver;
impl ResourceResolver for PanicResolver {
    fn resolve_font(&mut self, _: &Font) -> ResourceRef {
        unreachable!("fixture has no font commands")
    }
    fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
        unreachable!("no Blit in this fixture")
    }
}

fn red() -> Color {
    Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    }
}

#[test]
fn record_encode_decode_is_lossless() {
    let mut warp = Transform::IDENTITY;
    warp.tx = Fixed::from_int(7);
    let path = {
        let mut p = Path::new();
        p.move_to(Point::ZERO).line_to(Point {
            x: Fixed::from_int(4),
            y: Fixed::from_int(4),
        });
        p.cmds.to_mut().push(PathCmd::Close);
        p
    };
    let path_paint = Paint::Color(red().into());
    let commands = [
        DrawCommand::Fill {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(8),
                h: Fixed::from_int(8),
            },
            transform: warp,
            quad: None,
            color: red(),
            radius: Fixed::from_int(2),
            opa: 200,
        },
        DrawCommand::Line {
            p1: Point::ZERO,
            p2: Point {
                x: Fixed::from_int(5),
                y: Fixed::from_int(5),
            },
            transform: Transform::IDENTITY,
            color: red(),
            width: Fixed::from_int(1),
            opa: 255,
        },
        DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &path_paint,
            opa: 255,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        },
    ];

    let mut recorded = vec::Vec::new();
    for cmd in &commands {
        recorded.push(record_command(cmd, &mut PanicResolver).unwrap());
    }
    let bytes = encode_scene(&recorded).unwrap();
    let back = decode_scene(&bytes).unwrap();
    assert_eq!(back, recorded);
}

#[test]
fn positioned_glyphs_retain_their_placements() {
    let font = Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let command = DrawCommand::GlyphRun {
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        glyphs: &glyphs,
        font: &font,
        color: red(),
        opa: 255,
    };

    struct FontResolver;
    impl ResourceResolver for FontResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            ResourceRef::Index(3)
        }

        fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
            unreachable!()
        }
    }
    let recorded = record_command(&command, &mut FontResolver).unwrap();
    let SceneOp::GlyphRun {
        font: ResourceRef::Index(3),
        ppem: 8,
        glyphs: recorded,
        ..
    } = recorded
    else {
        panic!("expected positioned glyph run")
    };
    assert_eq!(recorded.as_ref(), glyphs.as_slice());
}

#[test]
fn posed_glyphs_retain_one_paired_geometry_buffer() {
    let font = Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 0 },
    )];
    let frames = [textflow::placement::GlyphFrame {
        local_origin: textflow::shaping::FlowPoint {
            x: 4 << 8,
            y: 7 << 8,
        },
        unit_tangent: textflow::shaping::FlowPoint { x: 0, y: 1 << 8 },
    }];
    let command = DrawCommand::PosedGlyphRun {
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        glyphs: crate::render::PosedGlyphs::new(&glyphs, &frames).unwrap(),
        font: &font,
        color: red(),
        opa: 255,
    };

    struct FontResolver;
    impl ResourceResolver for FontResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            ResourceRef::Index(3)
        }

        fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
            unreachable!()
        }
    }

    let recorded = record_command(&command, &mut FontResolver).unwrap();
    let SceneOp::PosedGlyphRun {
        glyphs: recorded, ..
    } = recorded
    else {
        panic!("expected posed glyph run")
    };
    assert_eq!(recorded.glyphs(), glyphs);
    assert_eq!(recorded.frames(), frames);
}
