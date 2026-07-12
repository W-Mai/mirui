#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::widgets::Text;

#[derive(Default)]
pub struct FillRules;

fn star_path(cx: Fixed, cy: Fixed, r: Fixed) -> Path {
    let mut pts = [Point::ZERO; 5];
    for (i, p) in pts.iter_mut().enumerate() {
        let a = Fixed::from_int(-90 + i as i32 * 72);
        *p = Point {
            x: cx + Fixed::cos_deg(a) * r,
            y: cy + Fixed::sin_deg(a) * r,
        };
    }
    let mut path = Path::new();
    path.move_to(pts[0]);
    for i in [2, 4, 1, 3] {
        path.line_to(pts[i]);
    }
    path.close();
    path
}

fn fill_rules_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let star_left = star_path(
        Fixed::from_int(112),
        Fixed::from_int(116),
        Fixed::from_int(82),
    );
    let star_right = star_path(
        Fixed::from_int(288),
        Fixed::from_int(116),
        Fixed::from_int(82),
    );
    let fill = Paint::Color(Color::rgb(255, 195, 70).into());
    let stroke = Paint::Color(Color::rgb(255, 245, 210).into());

    for (path, rule) in [
        (&star_left, FillRule::EvenOdd),
        (&star_right, FillRule::NonZero),
    ] {
        renderer.draw(
            &DrawCommand::FillPath {
                path,
                transform: Transform::IDENTITY,
                paint: &fill,
                opa: 245,
                fill_rule: rule,
            },
            ctx.clip,
        );
        renderer.draw(
            &DrawCommand::StrokePath {
                path,
                transform: Transform::IDENTITY,
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
