use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::types::Fixed;

use super::Style;
use super::view::{PaintCtx, View, ViewInvalidation};

fn style_render(world: &World, _entity: Entity, ctx: &mut PaintCtx<'_>) {
    let style = ctx.style();
    let bg_color = style.bg_color;
    let border_color = style.border_color;
    let border_width = style.border_width;
    let border_radius = style.border_radius;
    let theme = world
        .resource::<crate::ui::Theme>()
        .expect("App::new must insert Theme; missing means a test fixture skipped App");
    if !ctx.background_handled() {
        if let Some(color) = bg_color {
            ctx.draw(&DrawCommand::Fill {
                area: ctx.rect(),
                transform: ctx.transform(),
                quad: ctx.quad(),
                color: color.resolve_in(theme, ctx.state()),
                radius: border_radius,
                opa: 255,
            });
            ctx.set_background_handled(true);
        }
    }

    if let Some(border_color) = border_color {
        if border_width > Fixed::ZERO {
            ctx.draw(&DrawCommand::Border {
                area: ctx.rect(),
                transform: ctx.transform(),
                quad: ctx.quad(),
                color: border_color.resolve_in(theme, ctx.state()),
                width: border_width,
                radius: border_radius,
                opa: 255,
            });
        }
    }
}

pub fn view() -> View {
    View::new("Style", 50, style_render)
        .with_filter::<Style>()
        .with_invalidation(ViewInvalidation::Layout)
}
