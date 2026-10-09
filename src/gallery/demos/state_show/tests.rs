use super::*;
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::World;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;

#[test]
fn reactive_if_else_swaps_branch() {
    let mut app = App::headless(320, 240);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let world = &mut app.world;

    let col = world.get::<Children>(parent).unwrap().0[0];
    let branch_text = |w: &World| {
        let branch = w
            .get::<Children>(col)
            .unwrap()
            .0
            .iter()
            .copied()
            .filter(|&entity| !crate::ui::branch::is_effectively_hidden(w, entity))
            .nth(1)
            .unwrap();
        w.get::<Text>(branch).unwrap().resolve(w).into_owned()
    };
    assert_eq!(branch_text(world), "hidden — tap to show");

    let btn = world.get::<Children>(col).unwrap().0[0];
    let tap = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: btn,
    };
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, &tap, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    flush_signal_dirty(world);
    assert_eq!(branch_text(world), "now you see me", "if branch mounted");

    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, &tap, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    flush_signal_dirty(world);
    assert_eq!(
        branch_text(world),
        "hidden — tap to show",
        "else branch mounted"
    );
}

#[compose]
fn build_match_widgets() -> Signal<u8> {
    let mode = Signal::new(0u8);
    let m = mode.clone();
    ui! {
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
    let mut app = App::headless(320, 240);
    let parent = app.spawn_root().id();
    let mode = app.compose(parent, build_match_widgets());
    let world = &mut app.world;

    let col = world.get::<Children>(parent).unwrap().0[0];
    let arm_text = |w: &World| {
        let arm = w
            .get::<Children>(col)
            .unwrap()
            .0
            .iter()
            .copied()
            .find(|&entity| !crate::ui::branch::is_effectively_hidden(w, entity))
            .unwrap();
        w.get::<Text>(arm).unwrap().resolve(w).into_owned()
    };
    assert_eq!(arm_text(world), "zero");

    mode.set(5);
    flush_signal_dirty(world);
    assert_eq!(arm_text(world), "other", "arm switched on scrutinee change");
}

#[compose]
fn build_sandwich() -> Signal<bool> {
    let flag = Signal::new(false);
    let f = flag.clone();
    ui! {
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
    let mut app = App::headless(320, 240);
    let parent = app.spawn_root().id();
    let flag = app.compose(parent, build_sandwich());
    let world = &mut app.world;

    let col = world.get::<Children>(parent).unwrap().0[0];
    let text_at = |w: &World, i: usize| {
        let e = w
            .get::<Children>(col)
            .unwrap()
            .0
            .iter()
            .copied()
            .filter(|&entity| !crate::ui::branch::is_effectively_hidden(w, entity))
            .nth(i)
            .unwrap();
        w.get::<Text>(e).unwrap().resolve(w).into_owned()
    };
    assert_eq!(text_at(world, 0), "top");
    assert_eq!(text_at(world, 1), "off");
    assert_eq!(text_at(world, 2), "bottom");

    flag.set(true);
    flush_signal_dirty(world);
    assert_eq!(text_at(world, 0), "top");
    assert_eq!(text_at(world, 2), "bottom");
    assert_eq!(
        text_at(world, 1),
        "on",
        "branch swapped in place, no reorder"
    );
}
