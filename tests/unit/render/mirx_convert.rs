use super::*;

#[test]
fn fixed_conversion_preserves_every_endpoint_bit() {
    for bits in [i32::MIN, -1, 0, 1, i32::MAX] {
        let wire = mirx::types::Fixed::from_ratio(bits, 256);
        let runtime = Fixed::from(wire);
        assert_eq!(storage::to_i32(runtime), bits);
        assert_eq!(mirx::types::Fixed::from(runtime).to_f64(), wire.to_f64());
    }
}

#[test]
fn fixed64_conversion_preserves_every_endpoint_bit() {
    for bits in [i64::MIN, -1, 0, 1, i64::MAX] {
        let wire = mirx::types::Fixed64::from_ratio(bits, 65_536);
        let runtime = Fixed64::from(wire);
        let back = mirx::types::Fixed64::from(runtime);
        assert_eq!(back.to_parts(), wire.to_parts());
    }
}

#[test]
fn posed_scene_conversion_preserves_compact_geometry() {
    let glyphs = vec![textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(42),
        textflow::shaping::FlowPoint { x: 0, y: 0 },
    )];
    let frames = vec![textflow::placement::GlyphFrame {
        local_origin: textflow::shaping::FlowPoint {
            x: 12 << 8,
            y: 7 << 8,
        },
        unit_tangent: textflow::shaping::FlowPoint { x: 0, y: 1 << 8 },
    }];
    let scene = Scene {
        ops: vec![SceneOp::PosedGlyphRun {
            font: ResourceRef::Index(3),
            ppem: 20,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: Color::rgb(1, 2, 3),
            opa: 255,
            glyphs: PosedGlyphBuffer::new(glyphs, frames).unwrap(),
        }],
    };

    let wire: mirx::scene::Scene = scene.clone().into();
    let back: Scene = wire.into();
    assert_eq!(back, scene);
}
