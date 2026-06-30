use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Opa, Point, Rect, Transform};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// Draw operation produced by `render_system` and consumed by `Renderer::draw`.
///
/// All coordinate fields (`area`, `pos`, path points, `radius`, `width`) are
/// in **logical pixels**. Each variant carries a [`Transform`] (2D widget
/// affine). Variants that can participate in a 3D warp also carry an
/// optional pre-projected `quad: Option<[Point; 4]>`; when present, the
/// renderer uses the quad directly and ignores `area + transform` for
/// geometry. Renderers may `unimplemented!()` on transform classes they
/// don't handle (see `SwRenderer::draw_transformed` for what the software
/// backend covers today).
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
    Label {
        pos: Point,
        transform: Transform,
        text: &'a str,
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
        color: Color,
        opa: Opa,
    },
}

impl DrawCommand<'_> {
    #[inline]
    pub fn transform(&self) -> Transform {
        match *self {
            Self::Fill { transform, .. }
            | Self::Border { transform, .. }
            | Self::Label { transform, .. }
            | Self::Line { transform, .. }
            | Self::Arc { transform, .. }
            | Self::Blit { transform, .. }
            | Self::FillPath { transform, .. } => transform,
        }
    }
}
