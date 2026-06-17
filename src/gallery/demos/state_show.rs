extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
use crate::state::Signal;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::widgets::ProgressBar;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let shown = Signal::new(false);
    let toggle = shown.clone();
    let cond = shown.clone();

    let mode = Signal::new(0u8);
    let cycle = mode.clone();
    let sel = mode.clone();

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(16)
        ) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                width: 160,
                height: 40,
                border_radius: 8,
                text: "toggle panel"
            ) on Tap {
                toggle.update(|v| *v = !*v);
            }
            if $cond {
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    width: 200,
                    height: 60,
                    border_radius: 8,
                    text: "now you see me"
                )
            } else {
                View (
                    bg_color: Color::rgb(80, 80, 96),
                    width: 200,
                    height: 60,
                    border_radius: 8,
                    text: "hidden — tap to show"
                )
            }
            View (
                bg_color: Color::rgb(210, 168, 80),
                width: 160,
                height: 40,
                border_radius: 8,
                text: "cycle mode"
            ) on Tap {
                cycle.update(|m| *m = (*m + 1) % 3);
            }
            match $sel {
                0 => {
                    View (
                        width: 220,
                        height: 60,
                        border_radius: 8,
                        bg_color: Color::rgb(88, 166, 255),
                        text: "single card"
                    )
                }
                1 => {
                    Row (
                        width: 220,
                        height: 60,
                        justify: JustifyContent::SpaceBetween,
                        align: AlignItems::Center
                    ) {
                        View (
                            width: 60,
                            height: 60,
                            border_radius: 8,
                            bg_color: Color::rgb(220, 80, 80)
                        )
                        View (
                            width: 60,
                            height: 60,
                            border_radius: 8,
                            bg_color: Color::rgb(63, 185, 80)
                        )
                        View (
                            width: 60,
                            height: 60,
                            border_radius: 8,
                            bg_color: Color::rgb(88, 166, 255)
                        )
                    }
                }
                _ => {
                    Column (
                        width: 220,
                        align: AlignItems::Stretch,
                        padding: Padding::all(8)
                    ) {
                        View (height: 24, text: "stacked bars")
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
    use crate::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;
    use crate::state::flush_signal_dirty;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn reactive_if_else_swaps_branch() {
        use crate::ui::widgets::text::Text;
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        // reactive branches mount after static siblings: [toggle, cycle, if/else, match]
        let branch_text = |w: &World| {
            let branch = w.get::<Children>(col).unwrap().0[2];
            alloc::string::String::from_utf8(w.get::<Text>(branch).unwrap().0.clone()).unwrap()
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
                        View (text: "zero", height: 30)
                    }
                    _ => {
                        View (text: "other", height: 30)
                    }
                }
            }
        };
        mode
    }

    #[test]
    fn reactive_match_swaps_arm() {
        use crate::ui::widgets::text::Text;
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mode = build_match_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let arm_text = |w: &World| {
            let arm = w.get::<Children>(col).unwrap().0[0];
            alloc::string::String::from_utf8(w.get::<Text>(arm).unwrap().0.clone()).unwrap()
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
                View (text: "top", height: 20)
                if $f {
                    View (text: "on", height: 20)
                } else {
                    View (text: "off", height: 20)
                }
                View (text: "bottom", height: 20)
            }
        };
        flag
    }

    #[test]
    fn reactive_branch_keeps_index_between_static_siblings() {
        use crate::ui::widgets::text::Text;
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let flag = build_sandwich(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let text_at = |w: &World, i: usize| {
            let e = w.get::<Children>(col).unwrap().0[i];
            alloc::string::String::from_utf8(w.get::<Text>(e).unwrap().0.clone()).unwrap()
        };
        // static-ordered: top(0), bottom(1), then the reactive branch appends(2)
        assert_eq!(text_at(&world, 0), "top");
        assert_eq!(text_at(&world, 1), "bottom");
        assert_eq!(text_at(&world, 2), "off");

        flag.set(true);
        flush_signal_dirty(&mut world);
        // swap keeps the branch at its index; statics stay put
        assert_eq!(text_at(&world, 0), "top");
        assert_eq!(text_at(&world, 1), "bottom");
        assert_eq!(
            text_at(&world, 2),
            "on",
            "branch swapped in place, no reorder"
        );
    }
}
