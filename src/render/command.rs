use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Opa, Point, Rect, Transform};

pub use mirx::scene::{LineCap, LineJoin, Paint};

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

/// Non-premultiplied alpha: `src` channels are multiplied by `src.a / 255`
/// before the per-variant formula and folded back onto `dst` via the
/// standard `(1 - src.a)` weight, so `src.a == 0` leaves `dst` untouched
/// for every variant.
///
/// | mode | SwRenderer | wgpu | sdl_gpu | web_canvas |
/// |---|---|---|---|---|
/// | SourceOver / Add | full | full | full (native) | full |
/// | Screen / Multiply / Darken / Lighten / Difference | full | full | per-mode `unimplemented!()` when no `SDL_ComposeCustomBlendMode` factor combination matches | full |
///
/// `radius > 0` on `Blit` is only implemented by `SwRenderer` and `wgpu`;
/// `sdl_gpu` and `web_canvas` `unimplemented!()` and the panic message
/// points at the supported backends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CompositeMode {
    /// `out = src*src.a + dst*(1 - src.a)`. Default; matches v0.36.0.
    #[default]
    SourceOver,
    /// `out = saturate(src*src.a + dst)`. LED glow / fire / additive sprites.
    Add,
    /// `out = 1 - (1 - src*src.a)*(1 - dst)`. Soft glow / DropGlow halo.
    Screen,
    /// `out = (src*src.a)*dst/255 + dst*(1 - src.a)`. Tint / shading.
    Multiply,
    /// `out = min(src*src.a, dst) + dst*(1 - src.a)`. Photoshop Darken.
    Darken,
    /// `out = max(src*src.a, dst) + dst*(1 - src.a)`. Photoshop Lighten.
    Lighten,
    /// `out = |src*src.a - dst| + dst*(1 - src.a)`. Inversion / creative.
    Difference,
}

impl CompositeMode {
    /// Per-channel formula at `src.a == 255`. The caller folds `src.a`
    /// back via the standard non-premul `out = m * src.a + dst *
    /// (255 - src.a)` weight, so `src.a == 0` always preserves `dst`
    /// and `src.a == 255` yields exactly the value returned here.
    ///
    /// Internal arithmetic is u32 to keep the 255 × 255 path from
    /// overflowing; division by 255 uses the `(x + 127) / 255`
    /// round-to-nearest approximation, exact within ±1 over u8.
    #[inline]
    pub fn blend_channel(self, src: u8, dst: u8) -> u8 {
        let s = src as u32;
        let d = dst as u32;
        match self {
            Self::SourceOver => src,
            Self::Add => (s + d).min(255) as u8,
            Self::Screen => {
                let inv = (255 - s) * (255 - d);
                (255 - ((inv + 127) / 255)) as u8
            }
            Self::Multiply => ((s * d + 127) / 255) as u8,
            Self::Darken => src.min(dst),
            Self::Lighten => src.max(dst),
            Self::Difference => src.abs_diff(dst),
        }
    }
}

#[cfg(test)]
mod composite_mode_tests {
    use super::CompositeMode::*;

    #[test]
    fn source_over_returns_src() {
        assert_eq!(SourceOver.blend_channel(0, 0), 0);
        assert_eq!(SourceOver.blend_channel(128, 200), 128);
        assert_eq!(SourceOver.blend_channel(255, 0), 255);
    }

    #[test]
    fn add_saturates_at_255() {
        assert_eq!(Add.blend_channel(128, 128), 255);
        assert_eq!(Add.blend_channel(255, 255), 255);
        assert_eq!(Add.blend_channel(0, 0), 0);
        assert_eq!(Add.blend_channel(64, 64), 128);
    }

    #[test]
    fn screen_is_inverse_multiply_of_inverses() {
        assert!((190..=192).contains(&Screen.blend_channel(128, 128)));
        assert_eq!(Screen.blend_channel(255, 0), 255);
        assert_eq!(Screen.blend_channel(0, 0), 0);
        assert_eq!(Screen.blend_channel(0, 200), 200);
    }

    #[test]
    fn multiply_halves_at_50_percent() {
        assert!((63..=65).contains(&Multiply.blend_channel(128, 128)));
        assert_eq!(Multiply.blend_channel(0, 200), 0);
        assert_eq!(Multiply.blend_channel(255, 200), 200);
    }

    #[test]
    fn darken_keeps_smaller() {
        assert_eq!(Darken.blend_channel(64, 192), 64);
        assert_eq!(Darken.blend_channel(200, 100), 100);
        assert_eq!(Darken.blend_channel(128, 128), 128);
    }

    #[test]
    fn lighten_keeps_larger() {
        assert_eq!(Lighten.blend_channel(64, 192), 192);
        assert_eq!(Lighten.blend_channel(200, 100), 200);
    }

    #[test]
    fn difference_is_absolute_diff() {
        assert_eq!(Difference.blend_channel(192, 64), 128);
        assert_eq!(Difference.blend_channel(64, 192), 128);
        assert_eq!(Difference.blend_channel(100, 100), 0);
    }
}

#[cfg(test)]
mod posed_glyph_tests {
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

/// Draw operation produced by `render_system` and consumed by `Renderer::draw`.
///
/// All coordinate fields (`area`, `pos`, path points, `radius`, `width`) are
/// in **logical pixels**. Each drawable variant carries its local [`Transform`].
/// An optional `quad` is explicit leaf geometry, not inherited projective
/// state. Widget and scene homographies are supplied through
/// [`crate::render::renderer::Renderer::draw_projective`].
pub enum DrawCommand<'a> {
    Fill {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        radius: Fixed,
        opa: Opa,
    },
    Border {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        width: Fixed,
        radius: Fixed,
        opa: Opa,
    },
    GlyphRun {
        pos: Point,
        transform: Transform,
        glyphs: &'a [textflow::shaping::PositionedGlyph],
        font: &'a Font,
        color: Color,
        opa: Opa,
    },
    PosedGlyphRun {
        pos: Point,
        transform: Transform,
        glyphs: PosedGlyphs<'a>,
        font: &'a Font,
        color: Color,
        opa: Opa,
    },
    Line {
        p1: Point,
        p2: Point,
        transform: Transform,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    /// Stroked arc on a circle (center, radius). Angles in degrees, CCW.
    Arc {
        center: Point,
        transform: Transform,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
        opa: Opa,
    },
    /// Blit `texture` at `pos`, scaling (nearest) to `size` logical pixels.
    /// `radius > 0` clips to a rounded rectangle via SDF coverage (only
    /// supported by `SwRenderer` / `wgpu`). `composite` selects the blend
    /// formula — see [`CompositeMode`] for per-mode backend support.
    Blit {
        pos: Point,
        size: Point,
        transform: Transform,
        quad: Option<[Point; 4]>,
        texture: &'a Texture<'a>,
        opa: Opa,
        radius: Fixed,
        composite: CompositeMode,
    },
    /// Fill the closed region described by `path`. Path vertices are in
    /// logical pixels; under non-translate transforms the backend may
    /// fall back to `unimplemented!` (same policy as `Arc`).
    FillPath {
        path: &'a Path,
        transform: Transform,
        paint: &'a Paint,
        opa: Opa,
        fill_rule: FillRule,
    },
    /// Stroke `path` with `width` logical pixels. Cap/join follow SVG
    /// semantics; `miter_limit` defaults to 4.0 when omitted by the
    /// caller. Backends without a path stroker `unimplemented!()`.
    StrokePath {
        path: &'a Path,
        transform: Transform,
        paint: &'a Paint,
        width: Fixed,
        opa: Opa,
        line_cap: LineCap,
        line_join: LineJoin,
        miter_limit: Fixed,
        dash: &'a [Fixed],
    },
    PushClip {
        path: &'a Path,
        transform: Transform,
        fill_rule: FillRule,
    },
    PopClip,
    ApplyBlur {
        alpha: Fixed,
        region: Rect,
    },
}

impl DrawCommand<'_> {
    #[inline]
    pub fn transform(&self) -> Transform {
        match *self {
            Self::Fill { transform, .. }
            | Self::Border { transform, .. }
            | Self::GlyphRun { transform, .. }
            | Self::PosedGlyphRun { transform, .. }
            | Self::Line { transform, .. }
            | Self::Arc { transform, .. }
            | Self::Blit { transform, .. }
            | Self::FillPath { transform, .. }
            | Self::StrokePath { transform, .. } => transform,
            Self::PushClip { .. } | Self::PopClip | Self::ApplyBlur { .. } => Transform::IDENTITY,
        }
    }
}
