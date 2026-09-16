#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::widgets::Text;

#[derive(Default)]
pub struct FillRules;

static STAR: Path = path!(
    M 100 18
    L 148.198 166.34
    L 22.013 74.661
    L 177.987 74.661
    L 51.802 166.34
    Z
);

fn fill_rules_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let fill = Paint::Color(theme.resolve(ColorToken::Secondary).into());
    let stroke = Paint::Color(theme.resolve(ColorToken::OnSurface).into());

    for (transform, rule) in [
        (
            Transform::translate(Fixed::from_int(12), Fixed::from_int(16)),
            FillRule::EvenOdd,
        ),
        (
            Transform::translate(Fixed::from_int(188), Fixed::from_int(16)),
            FillRule::NonZero,
        ),
    ] {
        ctx.draw(
            renderer,
            &DrawCommand::FillPath {
                path: &STAR,
                transform,
                paint: &fill,
                opa: 245,
                fill_rule: rule,
            },
            ctx.clip,
        );
        ctx.draw(
            renderer,
            &DrawCommand::StrokePath {
                path: &STAR,
                transform,
                paint: &stroke,
                width: Fixed::from_int(2),
                opa: 235,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            },
            ctx.clip,
        );
    }
}

pub fn fill_rules_view() -> View {
    View::new("FillRules", 60, fill_rules_render).with_filter::<FillRules>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::FlexEnd,
            padding: Padding::all(10)
        ) {
            Row (height: 24) {
                Text ("EvenOdd", width: 92, height: 22, text_color: ColorToken::OnSurface)
                Text ("NonZero", width: 92, height: 22, text_color: ColorToken::OnSurface)
            }
            FillRules (position: Position::Absolute, left: 0, top: 0, width: 400, height: 240)
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(fill_rules_view());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_geometry_stays_in_static_storage() {
        assert!(STAR.is_borrowed());
    }
}
