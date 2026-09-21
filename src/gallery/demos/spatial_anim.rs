extern crate alloc;

use crate::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::ecs::Entity;
use crate::ecs::{DeltaTimeMs, World};
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{ParagraphStyle, Text};

mirui_macros::animate!(AnimateTweenY, |world, entity, value| {
    ui::set_position(world, entity, Fixed::from_int(48), value);
});

pub struct SpringBall {
    pub spring: Spring,
    pub x: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spring_system(world: &mut World) {
    let dt = world.resource::<DeltaTimeMs>().map_or(16, |r| r.0);
    world.for_each_stable::<SpringBall>(|world, e| {
        let (pos, settled, target, x) = {
            let Some(sb) = world.get_mut::<SpringBall>(e) else {
                return;
            };
            sb.spring.tick(dt);
            (
                sb.spring.value(),
                sb.spring.is_settled(),
                sb.spring.target,
                sb.x,
            )
        };
        ui::set_position(world, e, x, pos);
        if settled && let Some(sb) = world.get_mut::<SpringBall>(e) {
            let new_target = if target.to_int() > 150 {
                Fixed::from_int(48)
            } else {
                Fixed::from_int(220)
            };
            sb.spring.retarget(new_target, None);
        }
    });
}
//~focus-end

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(10)
        ) {
            View (
                id: "spatial_animation_stage",
                width: Dimension::percent(100),
                max_width: 400,
                height: 280,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    id: "spatial_animation_header",
                    position: Position::Absolute,
                    left: 12,
                    top: 12,
                    width: Dimension::percent(92),
                    height: 24,
                    column_gap: 10
                ) {
                    Text (
                        "TWEEN",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Error,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "SPRING",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Success,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "ELASTIC",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Primary,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                }
                walk [58, 170, 282] with x {
                    View (
                        position: Position::Absolute,
                        left: x,
                        top: 52,
                        width: 2,
                        height: 190,
                        bg_color: ColorToken::Outline,
                        border_radius: 1
                    )
                }
                View (
                    bg_color: ColorToken::Error,
                    position: Position::Absolute,
                    left: 48,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    AnimateTweenY(
                        Tween::new(
                                Fixed::from_int(48),
                                Fixed::from_int(220),
                                800,
                                ease::ease_in_out_cubic,
                                PlayMode::PingPong,
                            )
                            .into(),
                    ),
                ]
                View (
                    bg_color: ColorToken::Success,
                    position: Position::Absolute,
                    left: 160,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    SpringBall {
                        spring: Spring::preset(Fixed::from_int(48), Fixed::from_int(220), SMOOTH)
                            .repeat(),
                        x: Fixed::from_int(160),
                    },
                ]
                View (
                    bg_color: ColorToken::Primary,
                    position: Position::Absolute,
                    left: 272,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    SpringBall {
                        spring: Spring::preset(Fixed::from_int(48), Fixed::from_int(220), BOUNCY)
                            .repeat(),
                        x: Fixed::from_int(272),
                    },
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
    app.add_system(AnimateTweenY::system());
    app.add_system(spring_system::system());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(400, 300);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;
    use crate::ui::Children;
    use crate::ui::ComputedRect;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::render_system::update_layout;

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
    fn labels_stay_inside_phone_stage() {
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
        let stage = rect("spatial_animation_stage");
        let header = rect("spatial_animation_header");
        assert!(header.x >= stage.x);
        assert!(header.x + header.w <= stage.x + stage.w);
    }
}
