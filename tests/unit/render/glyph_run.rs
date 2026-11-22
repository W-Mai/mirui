use super::PosedGlyphs;
use crate::render::font::Font;
use crate::types::{Fixed, Point, Transform};
use textflow::placement::GlyphFrame;
use textflow::shaping::{FlowPoint, GlyphId, PositionedGlyph};

#[test]
fn construction_rejects_unpaired_geometry() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(1),
        FlowPoint { x: 0, y: 0 },
    )];

    assert!(PosedGlyphs::new(&glyphs, &[]).is_none());
    assert!(PosedGlyphs::new(&glyphs, &[GlyphFrame::default()]).is_some());
}

#[test]
fn ink_bounds_use_the_same_oriented_raster_quads_as_rendering() {
    let font = Font::default();
    let glyphs = [PositionedGlyph::new(
        font.map_char('A').unwrap(),
        FlowPoint { x: 0, y: 0 },
    )];
    let frames = [GlyphFrame {
        local_origin: FlowPoint {
            x: 12 << 8,
            y: 1 << 8,
        },
        unit_tangent: FlowPoint { x: 0, y: 1 << 8 },
    }];
    let posed = PosedGlyphs::new(&glyphs, &frames).unwrap();

    assert_eq!(
        posed.ink_bounds(&font, Point::ZERO, Transform::IDENTITY, Fixed::ONE,),
        Some(crate::types::Rect {
            x: Fixed::from_int(11),
            y: Fixed::from_int(1),
            w: Fixed::from_int(8),
            h: Fixed::from_int(8),
        })
    );
}
