//! The mirui logo for Gallery demos.
//!
//! [`MiruiMark`] draws the eye mark and [`MiruiLogo`] the mark followed by the
//! wordmark. Both keep the colours of the published artwork rather than the
//! demo palette, and pick the light or dark artwork from the theme surface.

use crate::app::{App, RendererFactory};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::scene::Paint;
use crate::surface::Surface;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

/// The eye mark: two blades around a red iris with a white catchlight.
#[crate::component]
#[derive(Default)]
pub(crate) struct MiruiMark;

/// The eye mark followed by the `mirui` wordmark.
#[crate::component]
#[derive(Default)]
pub(crate) struct MiruiLogo;

// Logo geometry at a tenth of its source size, centred on the mark. The mark
// is about 24 by 30 units; the full logo runs from x = -12.164 to 75.675.
static LEAD: Path =
    path!("M -4.919 -14.758 L 8.855 -2.952 L -2.952 -8.855 L -7.245 -0.268 L -12.164 -0.268 Z");
static TRAIL: Path =
    path!("M 4.919 14.758 L -8.855 2.952 L 2.952 8.855 L 7.245 0.268 L 12.164 0.268 Z");
static IRIS: Path = path!(
    "M 4.626 -1.163 C 4.626 0.107 3.596 1.137 2.326 1.137 C 1.056 1.137 0.026 0.107 0.026 -1.163 C 0.026 -2.433 1.056 -3.463 2.326 -3.463 C 3.596 -3.463 4.626 -2.433 4.626 -1.163 Z"
);
static CATCHLIGHT: Path = path!(
    "M 3.588 -2.089 C 3.588 -1.647 3.23 -1.289 2.788 -1.289 C 2.346 -1.289 1.988 -1.647 1.988 -2.089 C 1.988 -2.531 2.346 -2.889 2.788 -2.889 C 3.23 -2.889 3.588 -2.531 3.588 -2.089 Z"
);
static WORD_M: Path = path!(
    "M 17.7 9.3 L 17.7 0.6 C 17.7 -0.637 18.067 -1.846 18.754 -2.874 C 19.442 -3.902 20.419 -4.703 21.562 -5.176 C 22.705 -5.648 23.962 -5.771 25.175 -5.529 C 26.388 -5.287 27.502 -4.69 28.375 -3.814 C 29.248 -4.69 30.362 -5.287 31.575 -5.529 C 32.788 -5.771 34.045 -5.648 35.188 -5.176 C 36.331 -4.703 37.308 -3.902 37.996 -2.874 C 38.683 -1.846 39.05 -0.637 39.05 0.6 L 39.05 9.3 L 35.4 9.3 L 35.4 0.6 C 35.4 0.051 35.226 -0.484 34.903 -0.928 C 34.581 -1.372 34.126 -1.703 33.603 -1.873 C 33.081 -2.042 32.519 -2.042 31.997 -1.873 C 31.474 -1.703 31.019 -1.372 30.697 -0.928 C 30.374 -0.484 30.2 0.051 30.2 0.6 L 30.2 7.475 L 26.55 9.3 L 26.55 0.6 C 26.55 0.051 26.376 -0.484 26.053 -0.928 C 25.731 -1.372 25.276 -1.703 24.753 -1.873 C 24.231 -2.042 23.669 -2.042 23.147 -1.873 C 22.624 -1.703 22.169 -1.372 21.847 -0.928 C 21.524 -0.484 21.35 0.051 21.35 0.6 L 21.35 7.475 Z"
);
static WORD_I1: Path = path!("M 41.85 -5.4 L 45.5 -5.4 L 45.5 9.3 L 41.85 9.3 Z");
static WORD_R: Path = path!(
    "M 48.3 9.3 L 48.3 0.6 C 48.3 -0.497 48.589 -1.575 49.137 -2.525 C 49.686 -3.475 50.475 -4.264 51.425 -4.813 C 52.375 -5.361 53.453 -5.65 54.55 -5.65 L 54.55 -2 C 54.094 -2 53.645 -1.88 53.25 -1.652 C 52.855 -1.423 52.527 -1.095 52.298 -0.7 C 52.07 -0.305 51.95 0.144 51.95 0.6 L 51.95 7.475 L 48.3 9.3 Z"
);
static WORD_U: Path = path!(
    "M 56.35 -5.4 L 60 -5.4 L 60 3.3 C 60 3.849 60.174 4.384 60.497 4.828 C 60.819 5.272 61.274 5.603 61.797 5.773 C 62.319 5.942 62.881 5.942 63.403 5.773 C 63.926 5.603 64.381 5.272 64.703 4.828 C 65.026 4.384 65.2 3.849 65.2 3.3 L 65.2 -5.4 L 68.85 -7.225 L 68.85 3.3 C 68.85 4.62 68.432 5.906 67.656 6.974 C 66.881 8.041 65.786 8.836 64.531 9.244 C 63.276 9.652 61.924 9.652 60.669 9.244 C 59.414 8.836 58.319 8.041 57.544 6.974 C 56.768 5.906 56.35 4.62 56.35 3.3 Z"
);
static WORD_I2: Path = path!("M 71.55 -5.4 L 75.2 -5.4 L 75.2 9.3 L 71.55 9.3 Z");
static DOT_SEE: Path = path!(
    "M 45.975 -8.8 C 45.975 -7.53 44.945 -6.5 43.675 -6.5 C 42.405 -6.5 41.375 -7.53 41.375 -8.8 C 41.375 -10.07 42.405 -11.1 43.675 -11.1 C 44.945 -11.1 45.975 -10.07 45.975 -8.8 Z"
);
static DOT_UI: Path = path!(
    "M 75.675 -8.8 C 75.675 -7.53 74.645 -6.5 73.375 -6.5 C 72.105 -6.5 71.075 -7.53 71.075 -8.8 C 71.075 -10.07 72.105 -11.1 73.375 -11.1 C 74.645 -11.1 75.675 -10.07 75.675 -8.8 Z"
);

/// Height of the mark, which also bounds the full logo, in logo units.
const HEIGHT: i32 = 30;
/// Width of the full logo in logo units.
pub(crate) const LOGO_WIDTH: Fixed = Fixed::from_ratio(8784, 100);
/// Horizontal centre of the full logo, measured from the mark centre.
const LOGO_CENTRE_X: Fixed = Fixed::from_ratio(3176, 100);

/// Colours of the published light and dark artwork.
struct Artwork {
    ink: Color,
    see: Color,
    ui: Color,
}

fn artwork(theme: &Theme) -> Artwork {
    let surface = theme.resolve(ColorToken::Surface);
    if u16::from(surface.r) + u16::from(surface.g) + u16::from(surface.b) < 384 {
        Artwork {
            ink: Color::rgb(238, 240, 244),
            see: Color::rgb(229, 83, 59),
            ui: Color::rgb(111, 130, 255),
        }
    } else {
        Artwork {
            ink: Color::rgb(57, 66, 76),
            see: Color::rgb(207, 67, 43),
            ui: Color::rgb(52, 82, 255),
        }
    }
}

fn fill(paint: &mut PaintCtx<'_>, path: &Path, transform: Transform, color: Color) {
    paint.draw(&DrawCommand::FillPath {
        path,
        transform,
        paint: &Paint::Color(color.into()),
        opa: 255,
        fill_rule: FillRule::NonZero,
    });
}

fn draw_mark(paint: &mut PaintCtx<'_>, transform: Transform, colors: &Artwork) {
    fill(paint, &LEAD, transform, colors.ink);
    fill(paint, &TRAIL, transform, colors.ink);
    fill(paint, &IRIS, transform, colors.see);
    fill(paint, &CATCHLIGHT, transform, Color::rgb(255, 255, 255));
}

/// Scale `units` into the widget rect, centred on its middle.
fn centred(paint: &PaintCtx<'_>, scale: Fixed) -> Transform {
    let rect = paint.rect();
    paint
        .transform()
        .compose(&Transform::translate(
            rect.x + rect.w / 2,
            rect.y + rect.h / 2,
        ))
        .compose(&Transform::scale(scale, scale))
}

#[crate::view(component = MiruiMark, name = "MiruiMark", priority = 60)]
pub(crate) fn mark_render(paint: paint!(), theme: res!(Theme)) {
    let rect = paint.rect();
    let transform = centred(paint, rect.w.min(rect.h) / HEIGHT);
    draw_mark(paint, transform, &artwork(&theme));
}

#[crate::view(component = MiruiLogo, name = "MiruiLogo", priority = 60)]
pub(crate) fn logo_render(paint: paint!(), theme: res!(Theme)) {
    let rect = paint.rect();
    let scale = (rect.w / LOGO_WIDTH).min(rect.h / HEIGHT);
    let transform =
        centred(paint, scale).compose(&Transform::translate(-LOGO_CENTRE_X, Fixed::ZERO));
    let colors = artwork(&theme);
    draw_mark(paint, transform, &colors);
    for letter in [&WORD_M, &WORD_I1, &WORD_R, &WORD_U, &WORD_I2] {
        fill(paint, letter, transform, colors.ink);
    }
    fill(paint, &DOT_SEE, transform, colors.see);
    fill(paint, &DOT_UI, transform, colors.ui);
}

/// Register the mark and logo views.
pub(crate) fn install<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(mark_render::view())
        .with_widget(logo_render::view());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::ColorFormat;
    use crate::surface::FramebufferAccess;
    use crate::surface::framebuf::FramebufSurface;

    #[compose]
    fn fixture(logo: bool) -> Entity {
        if logo {
            ui! {
                View (
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: ColorToken::Surface
                ) [
                    MiruiLogo,
                ]
            }
        } else {
            ui! {
                View (
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: ColorToken::Surface
                ) [
                    MiruiMark,
                ]
            }
        }
    }

    fn render(width: u16, height: u16, logo: bool, dark: bool) -> alloc::vec::Vec<u8> {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush);
        let mut app = App::new(backend);
        app.with_default_widgets().with_default_systems();
        install(&mut app);
        crate::gallery::showcase_theme::install(&mut app);
        if !dark {
            app.set_theme(crate::gallery::showcase_theme::LIGHT_ID)
                .unwrap();
        }
        let root = app.spawn_root().id();
        app.compose(root, fixture(logo));
        app.set_root(root);
        app.render().unwrap();
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec()
    }

    fn count(pixels: &[u8], color: Color) -> usize {
        pixels
            .chunks_exact(4)
            .filter(|p| {
                p[0].abs_diff(color.r) < 6
                    && p[1].abs_diff(color.g) < 6
                    && p[2].abs_diff(color.b) < 6
            })
            .count()
    }

    #[test]
    fn mark_uses_the_published_ink_and_iris_on_both_surfaces() {
        let dark = render(60, 60, false, true);
        assert!(count(&dark, Color::rgb(238, 240, 244)) > 200);
        assert!(count(&dark, Color::rgb(229, 83, 59)) > 30);
        let light = render(60, 60, false, false);
        assert!(count(&light, Color::rgb(57, 66, 76)) > 200);
        assert!(count(&light, Color::rgb(207, 67, 43)) > 30);
    }

    #[test]
    fn logo_draws_both_letter_dots_inside_its_box() {
        let pixels = render(176, 60, true, true);
        assert!(count(&pixels, Color::rgb(111, 130, 255)) > 20, "ui dot");
        assert!(
            count(&pixels, Color::rgb(229, 83, 59)) > 60,
            "iris and see dot"
        );
        assert!(
            count(&pixels, Color::rgb(238, 240, 244)) > 1_000,
            "mark and wordmark"
        );
    }
}
