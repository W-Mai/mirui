use super::runtime::UI;
use super::style::{BLUE, CYAN, GOLD, PANEL_ALT};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::font::FontManager;
use crate::render::font::scalar::ScalarField;
use crate::render::renderer::Renderer;
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};

#[crate::component]
pub(super) struct RasterContour {
    pub(super) font: FontToken,
    pub(super) character: char,
    pub(super) ppem: u16,
}

impl Default for RasterContour {
    fn default() -> Self {
        Self {
            font: UI,
            character: 'S',
            ppem: 56,
        }
    }
}

fn contour_color(theme: &Theme, value: Fixed) -> Color {
    let surface = theme.resolve(PANEL_ALT);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let success = theme.resolve(GOLD);
    match crate::types::fixed::to_textflow(value).clamp(0, 256) {
        0..=63 => surface,
        64..=111 => surface.blend_with(secondary, Fixed::from_ratio(1, 2)),
        112..=144 => success,
        145..=207 => secondary,
        _ => primary,
    }
}

fn raster_contour_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(spec), Some(manager)) = (
        world.get::<RasterContour>(entity),
        world.resource::<FontManager>(),
    ) else {
        return;
    };
    let font = manager.resolve(spec.font.cache_key());
    let Some(raster) = font
        .map_char(spec.character)
        .and_then(|glyph| font.raster(glyph, spec.ppem))
    else {
        return;
    };
    let Some(region) = raster.region else {
        return;
    };
    let bits = match raster.representation.kind() {
        mirx::font::FontRepresentationKind::Coverage { bits }
        | mirx::font::FontRepresentationKind::SignedDistance { bits, .. } => bits,
        _ => return,
    };
    let Some(field) = ScalarField::new(
        raster.surface.samples(),
        raster.surface.stride(),
        region,
        bits,
    ) else {
        return;
    };
    let width = i32::try_from(field.width()).unwrap_or(i32::MAX).max(1);
    let height = i32::try_from(field.height()).unwrap_or(i32::MAX).max(1);
    let cell = (rect.w.to_int() / width)
        .min(rect.h.to_int() / height)
        .max(1);
    let grid_width = Fixed::from_int(width * cell);
    let grid_height = Fixed::from_int(height * cell);
    let origin = Point {
        x: rect.x + (rect.w - grid_width) / Fixed::from_int(2),
        y: rect.y + (rect.h - grid_height) / Fixed::from_int(2),
    };
    let extent = Fixed::from_int((cell - 1).max(1));
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    for y in 0..height {
        for x in 0..width {
            ctx.draw(
                renderer,
                &DrawCommand::Fill {
                    area: Rect {
                        x: origin.x + Fixed::from_int(x * cell),
                        y: origin.y + Fixed::from_int(y * cell),
                        w: extent,
                        h: extent,
                    },
                    transform: ctx.transform,
                    quad: None,
                    color: contour_color(theme, field.sample(x, y)),
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                ctx.clip,
            );
        }
    }
}

pub fn raster_contour_view() -> View {
    View::new("RasterContour", 61, raster_contour_render).with_filter::<RasterContour>()
}
