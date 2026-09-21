extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

mirui_macros::animate!(AnimateX, |world, entity, value| {
    ui::set_position(world, entity, value, Fixed::from_int(70));
});

mirui_macros::animate!(AnimateColor, |world, entity, value| {
    let r = (value * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
    if let Some(style) = world.get_mut::<ui::Style>(entity) {
        style.set_bg_color(Color::rgb(r, 50, 255 - r));
    }
    world.invalidate(entity);
});

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(12)
        ) {
            View (
                id: "animation_stage",
                width: Dimension::percent(100),
                max_width: 320,
                height: 144,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    id: "animation_header",
                    height: 52,
                    align: AlignItems::Center,
                    padding: Padding {
                        top: Dimension::px(12),
                        right: Dimension::px(16),
                        bottom: Dimension::px(12),
                        left: Dimension::px(16),
                    }
                ) {
                    Text (
                        "TWEEN MOTION",
                        grow: 1.0,
                        font_size: 16,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "PING · PONG",
                        width: 100,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Primary,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                }
                View (
                    position: Position::Absolute,
                    left: 18,
                    top: 85,
                    width: 264,
                    height: 4,
                    bg_color: ColorToken::Outline,
                    border_radius: 2
                )
                View (
                    bg_color: Color::rgb(255, 86, 139),
                    border_color: Color::rgb(255, 174, 206),
                    border_width: 2,
                    border_radius: 18,
                    position: Position::Absolute,
                    left: 18,
                    top: 70,
                    width: 36,
                    height: 36
                ) [
                    AnimateX(
                        Tween::new(
                                Fixed::from_int(18),
                                Fixed::from_int(246),
                                1200,
                                ease::ease_in_out_cubic,
                                PlayMode::PingPong,
                            )
                            .into(),
                    ),
                    AnimateColor(
                        Tween::new(Fixed::ZERO, Fixed::ONE, 2400, ease::ease_in_out_quad, PlayMode::Loop)
                            .into(),
                    ),
                ]
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
    app.add_system(AnimateX::system());
    app.add_system(AnimateColor::system());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 180);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;
    use crate::ui::Children;
    use crate::ui::render_system::update_layout;
    use crate::ui::{ComputedRect, IdMap, UiScope};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
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

    #[test]
    fn phone_header_stays_inside_the_animation_stage() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);

        update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
        let rect = |id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let stage = rect("animation_stage");
        let header = rect("animation_header");
        assert!(header.x >= stage.x);
        assert!(header.x + header.w <= stage.x + stage.w);
    }
}
