#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

static DIAMOND_PATH: Path = path!(M 50 0 L 100 50 L 50 100 L 0 50 Z);

pub struct Diamond {
    pub color: Color,
    pub line_width: Fixed,
}

impl Default for Diamond {
    fn default() -> Self {
        Self {
            color: Color::rgb(255, 255, 255),
            line_width: Fixed::from_int(1),
        }
    }
}

fn diamond_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(d) = world.get::<Diamond>(entity) else {
        return;
    };
    let transform = ctx
        .transform
        .compose(&Transform::translate(rect.x, rect.y))
        .compose(&Transform::scale(
            rect.w / Fixed::from_int(100),
            rect.h / Fixed::from_int(100),
        ));
    let paint = Paint::Color(d.color.into());
    let dash: [Fixed; 0] = [];
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &DIAMOND_PATH,
            transform,
            paint: &paint,
            width: d.line_width,
            opa: 255,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &dash,
        },
        ctx.clip,
    );
}

pub fn diamond_view() -> View {
    View::new("Diamond", 60, diamond_render)
}

pub const PALETTE: [Color; 3] = [
    Color::rgb(244, 167, 89),
    Color::rgb(140, 211, 255),
    Color::rgb(190, 240, 140),
];

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 16,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "CUSTOM VECTOR VIEW",
                width: Dimension::percent(100),
                max_width: 480,
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "STATIC PATH · TAP ANY TILE TO RECOLOR",
                width: Dimension::percent(100),
                max_width: 480,
                height: 18,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "custom_view_tiles",
                justify: JustifyContent::SpaceEvenly,
                align: AlignItems::Center,
                width: Dimension::percent(100),
                max_width: 480,
                height: 120
            ) {
                Diamond (
                    color: PALETTE[0],
                    line_width: Fixed::from_int(2),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
                Diamond (
                    color: PALETTE[1],
                    line_width: Fixed::from_int(3),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
                Diamond (
                    color: PALETTE[2],
                    line_width: Fixed::from_int(4),
                    width: 88,
                    height: 88,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 18
                ) on Tap {
                    if let Some(d) = ctx.world.get_mut::<Diamond>(ctx.entity) {
                        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
                        d.color = PALETTE[(i + 1) % PALETTE.len()];
                    }
                    ctx.world.invalidate(ctx.entity);
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(diamond_view());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::view::ViewRegistry;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(diamond_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn diamond_geometry_stays_in_static_storage() {
        assert!(DIAMOND_PATH.is_borrowed());
    }

    #[test]
    fn tap_cycles_diamond_color() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(diamond_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        let row = world.find_by_id("custom_view_tiles").unwrap();
        let d0 = world.get::<Children>(row).unwrap().0[0];

        assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[0]));
        GestureHandler::trigger(
            &mut world,
            d0,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: d0,
            },
        );
        assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[1]));
    }
}
