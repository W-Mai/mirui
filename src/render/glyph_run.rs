use crate::render::font::Font;
use crate::types::{Fixed, Point, Rect, Transform};

/// Paired shaped glyphs and path-placement frames for one posed run.
#[derive(Clone, Copy, Debug)]
pub struct PosedGlyphs<'a> {
    glyphs: &'a [textflow::shaping::PositionedGlyph],
    frames: &'a [textflow::placement::GlyphFrame],
}

impl<'a> PosedGlyphs<'a> {
    pub const fn new(
        glyphs: &'a [textflow::shaping::PositionedGlyph],
        frames: &'a [textflow::placement::GlyphFrame],
    ) -> Option<Self> {
        if glyphs.len() != frames.len() {
            return None;
        }
        Some(Self { glyphs, frames })
    }

    pub const fn glyphs(self) -> &'a [textflow::shaping::PositionedGlyph] {
        self.glyphs
    }

    pub const fn frames(self) -> &'a [textflow::placement::GlyphFrame] {
        self.frames
    }

    pub fn iter(
        self,
    ) -> impl ExactSizeIterator<
        Item = (
            &'a textflow::shaping::PositionedGlyph,
            &'a textflow::placement::GlyphFrame,
        ),
    > {
        self.glyphs.iter().zip(self.frames)
    }

    pub(crate) fn ink_bounds(
        self,
        font: &Font,
        pos: Point,
        transform: Transform,
        output_scale: Fixed,
    ) -> Option<Rect> {
        let requested_size = font.size.max(1);
        let output_ppem = crate::render::font::output_ppem(
            requested_size,
            output_scale * transform.raster_scale(),
        );
        self.ink_bounds_for_output(font, pos, transform, output_ppem)
    }

    pub(crate) fn ink_bounds_for_output(
        self,
        font: &Font,
        pos: Point,
        transform: Transform,
        output_ppem: u16,
    ) -> Option<Rect> {
        let requested_size = font.size.max(1);
        let mut bounds: Option<Rect> = None;
        for (glyph, frame) in self.iter() {
            let Some(raster) =
                font.raster_for_output(glyph.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(quad) = raster.posed_quad(pos, *frame, requested_size, transform) else {
                continue;
            };
            let glyph_bounds = quad.transform.apply_rect_bbox(quad.rect);
            bounds = Some(match bounds {
                Some(bounds) => bounds.union(&glyph_bounds),
                None => glyph_bounds,
            });
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
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
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
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
            posed.ink_bounds(
                &Font::bitmap_8x8(),
                Point::ZERO,
                Transform::IDENTITY,
                Fixed::ONE,
            ),
            Some(crate::types::Rect {
                x: Fixed::from_int(11),
                y: Fixed::from_int(1),
                w: Fixed::from_int(8),
                h: Fixed::from_int(8),
            })
        );
    }
}
