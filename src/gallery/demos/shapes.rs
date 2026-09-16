#![allow(clippy::needless_update)]

extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[derive(Default)]
pub struct Shapes {
    pub start_ms: u32,
}

fn shapes_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(state) = world.get::<Shapes>(entity) else {
        return;
    };
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let arc_color = theme.resolve(ColorToken::Primary);
    let hand_color = theme.resolve(ColorToken::Secondary);
    let tick_color = theme.resolve(ColorToken::OnSurfaceVariant);
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;

    let header_height = Fixed::from_int(38);
    let content_height = (rect.h - header_height).max(Fixed::ZERO);
    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + header_height + content_height / Fixed::from_int(2);
    let r = rect.w.min(content_height) / Fixed::from_int(2) - Fixed::from_int(8);
    let center = Point { x: cx, y: cy };

    ctx.draw(
        renderer,
        &DrawCommand::Arc {
            center,
            transform: ctx.transform,
            radius: r,
            start_angle: Fixed::from_int(0),
            end_angle: Fixed::from_int(360),
            color: arc_color,
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );

    let angle_deg_raw = ((elapsed_ms * 360) / 30_000) % 360;
    let angle_deg = Fixed::from_int(angle_deg_raw);
    let end = Point {
        x: cx + Fixed::cos_deg(angle_deg) * r,
        y: cy + Fixed::sin_deg(angle_deg) * r,
    };
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1: center,
            p2: end,
            transform: ctx.transform,
            color: hand_color,
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );

    for i in 0..12 {
        let a = Fixed::from_int(i * 30);
        let inner = r - Fixed::from_int(5);
        let outer = r - Fixed::from_int(1);
        let p1 = Point {
            x: cx + Fixed::cos_deg(a) * inner,
            y: cy + Fixed::sin_deg(a) * inner,
        };
        let p2 = Point {
            x: cx + Fixed::cos_deg(a) * outer,
            y: cy + Fixed::sin_deg(a) * outer,
        };
        ctx.draw(
            renderer,
            &DrawCommand::Line {
                p1,
                p2,
                transform: ctx.transform,
                color: tick_color,
                width: Fixed::ONE,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

pub fn shapes_view() -> View {
    View::new("Shapes", 60, shapes_render).with_filter::<Shapes>()
}

#[mirui_macros::system(order = ANIMATION)]
pub fn shapes_anim_system(world: &mut World) {
    world.for_each_stable::<Shapes>(|world, e| {
        world.invalidate(e);
    });
}

#[compose]
pub fn build_widgets() {
    let now_ms = cx
        .world_mut()
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(12),
            bg_color: ColorToken::Surface
        ) {
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 440,
                max_height: 296,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: 38,
                    padding: Padding {
                        left: Dimension::px(14),
                        right: Dimension::px(14),
                        ..Default::default()
                    },
                    align: AlignItems::Center
                ) {
                    Text (
                        "VECTOR CLOCK",
                        grow: 1.0,
                        font_size: 14,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "ARC · LINE",
                        width: 88,
                        height: 22,
                        font_size: 9,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Secondary,
                        border_color: ColorToken::Secondary,
                        border_width: 1,
                        border_radius: 11,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Shapes (
                    start_ms: now_ms,
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100)
                )
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
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(shapes_view());
    app.add_system(shapes_anim_system::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::view::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(shapes_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
