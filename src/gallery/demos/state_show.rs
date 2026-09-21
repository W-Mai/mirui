extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::{Button, ProgressBar, Text};

#[compose]
pub fn build_widgets() {
    let shown = Signal::new(false);
    let toggle = shown.clone();
    let cond = shown.clone();

    let mode = Signal::new(0u8);
    let cycle = mode.clone();
    let sel = mode.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Button (
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 11,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary,
                text_color: ColorToken::OnPrimary
            ) [
                Text::label("Toggle branch"),
            ] on Tap { toggle.update(|v| *v = !*v); }
            if $cond {
                View (
                    bg_color: ColorToken::Success,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 12,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("now you see me"),
                ]
            } else {
                View (
                    bg_color: ColorToken::SurfaceVariant,
                    width: Dimension::percent(100),
                    max_width: 260,
                    height: 58,
                    border_radius: 12,
                    text_color: ColorToken::OnSurfaceVariant
                ) [
                    Text::label("hidden — tap to show"),
                ]
            }
            Button (
                width: Dimension::percent(100),
                max_width: 220,
                height: 42,
                border_radius: 11,
                normal_color: ColorToken::Secondary,
                pressed_color: ColorToken::Primary,
                text_color: ColorToken::OnSecondary
            ) [
                Text::label("Cycle keyed layout"),
            ] on Tap { cycle.update(|m| *m = (*m + 1) % 3); }
            match $sel {
                0 => {
                    View (
                        width: Dimension::percent(100),
                        max_width: 260,
                        height: 60,
                        border_radius: 12,
                        bg_color: ColorToken::Primary,
                        text_color: ColorToken::OnPrimary
                    ) [
                        Text::label("single card"),
                    ]
                }
                1 => {
                    Row (
                        width: Dimension::percent(100),
                        max_width: 260,
                        height: 60,
                        justify: JustifyContent::SpaceBetween,
                        align: AlignItems::Center,
                        column_gap: 8
                    ) {
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 12,
                            bg_color: ColorToken::Error
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 12,
                            bg_color: ColorToken::Success
                        )
                        View (
                            grow: 1.0,
                            height: 60,
                            border_radius: 12,
                            bg_color: ColorToken::Primary
                        )
                    }
                }
                _ => {
                    Column (
                        width: Dimension::percent(100),
                        max_width: 260,
                        align: AlignItems::Stretch,
                        padding: Padding::all(8),
                        row_gap: 6,
                        bg_color: ColorToken::SurfaceVariant,
                        border_radius: 12
                    ) {
                        Text (
                            "stacked bars",
                            height: 24,
                            text_color: ColorToken::OnSurfaceVariant
                        )
                        ProgressBar (height: 12, border_radius: 6, value: 0.3)
                        ProgressBar (height: 12, border_radius: 6, value: 0.7)
                    }
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
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 240);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn reactive_if_else_swaps_branch() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let branch_text = |w: &World| {
            let branch = w.get::<Children>(col).unwrap().0[1];
            w.get::<Text>(branch).unwrap().resolve(w).into_owned()
        };
        assert_eq!(branch_text(&world), "hidden — tap to show");

        let btn = world.get::<Children>(col).unwrap().0[0];
        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: btn,
        };
        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(branch_text(&world), "now you see me", "if branch mounted");

        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(
            branch_text(&world),
            "hidden — tap to show",
            "else branch mounted"
        );
    }

    fn build_match_widgets(world: &mut World, parent: Entity) -> Signal<u8> {
        let mode = Signal::new(0u8);
        let m = mode.clone();
        ui! {
            :(
                parent: parent
                world: world
            :)

            Column (grow: 1.0) {
                match $m {
                    0 => {
                        Text ("zero", height: 30)
                    }
                    _ => {
                        Text ("other", height: 30)
                    }
                }
            }
        };
        mode
    }

    #[test]
    fn reactive_match_swaps_arm() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mode = build_match_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let arm_text = |w: &World| {
            let arm = w.get::<Children>(col).unwrap().0[0];
            w.get::<Text>(arm).unwrap().resolve(w).into_owned()
        };
        assert_eq!(arm_text(&world), "zero");

        mode.set(5);
        flush_signal_dirty(&mut world);
        assert_eq!(
            arm_text(&world),
            "other",
            "arm switched on scrutinee change"
        );
    }

    fn build_sandwich(world: &mut World, parent: Entity) -> Signal<bool> {
        let flag = Signal::new(false);
        let f = flag.clone();
        ui! {
            :(
                parent: parent
                world: world
            :)

            Column (grow: 1.0) {
                Text ("top", height: 20)
                if $f {
                    Text ("on", height: 20)
                } else {
                    Text ("off", height: 20)
                }
                Text ("bottom", height: 20)
            }
        };
        flag
    }

    #[test]
    fn reactive_branch_keeps_index_between_static_siblings() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let flag = build_sandwich(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let text_at = |w: &World, i: usize| {
            let e = w.get::<Children>(col).unwrap().0[i];
            w.get::<Text>(e).unwrap().resolve(w).into_owned()
        };
        assert_eq!(text_at(&world, 0), "top");
        assert_eq!(text_at(&world, 1), "off");
        assert_eq!(text_at(&world, 2), "bottom");

        flag.set(true);
        flush_signal_dirty(&mut world);
        assert_eq!(text_at(&world, 0), "top");
        assert_eq!(text_at(&world, 2), "bottom");
        assert_eq!(
            text_at(&world, 1),
            "on",
            "branch swapped in place, no reorder"
        );
    }
}
