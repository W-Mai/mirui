use super::scene::{CLIP_CIRCLE, LOGICAL_SIZE};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::Paint;
use crate::types::Transform;
use crate::ui::Theme;

#[crate::component]
#[derive(Default)]
pub struct ClipPath;

fn fill_rect(paint: &mut PaintCtx<'_>, area: Rect, color: Color, transform: Transform) {
    paint.draw(&DrawCommand::Fill {
        area,
        transform,
        quad: None,
        color,
        radius: Fixed::from_int(10),
        opa: 255,
    });
}

#[crate::view(component = ClipPath, name = "ClipPath", priority = 60)]
fn clip_path_render(theme: res!(optional Theme), paint: paint!()) {
    let canvas = crate::gallery::fit_logical_canvas(
        paint.rect(),
        paint.transform(),
        LOGICAL_SIZE,
        LOGICAL_SIZE,
    );
    let default_theme = Theme::default();
    let theme = theme.unwrap_or(&default_theme);
    let surface = theme.resolve(ColorToken::SurfaceVariant);
    let foreground = theme.resolve(ColorToken::OnSurface);
    let grays = [
        surface,
        surface.blend_with(foreground, Fixed::from_ratio(1, 12)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 6)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 4)),
    ];
    let colors = [
        theme.resolve(ColorToken::Primary),
        theme.resolve(ColorToken::Secondary),
        theme.resolve(ColorToken::Tertiary),
        theme.resolve(ColorToken::Success),
    ];

    for (i, color) in grays.into_iter().enumerate() {
        fill_rect(
            paint,
            Rect::new(30, 42 + i as i32 * 56, 260, 42),
            color,
            canvas,
        );
    }
    let circle_transform = canvas.compose(&Transform::translate(
        Fixed::from_int(56),
        Fixed::from_int(56),
    ));
    paint.with_clip_path(&CLIP_CIRCLE, circle_transform, FillRule::EvenOdd, |paint| {
        for (i, color) in colors.into_iter().enumerate() {
            fill_rect(
                paint,
                Rect::new(30, 42 + i as i32 * 56, 260, 42),
                color,
                canvas,
            );
        }
    });

    let outline = Paint::Color(theme.resolve(ColorToken::OnSurface).into());
    paint.draw(&DrawCommand::StrokePath {
        path: &CLIP_CIRCLE,
        transform: circle_transform,
        paint: &outline,
        width: Fixed::from_int(2),
        opa: 220,
        line_cap: crate::render::scene::LineCap::Round,
        line_join: crate::render::scene::LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: &[],
    });
}

pub use clip_path_render::view as clip_path_view;
