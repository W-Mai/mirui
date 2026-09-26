mod support {
    include!("support/tracking_allocator.rs");
}

use std::cell::Cell;
use std::rc::Rc;

use mirui::core::model::SharedValue;
use mirui::core::reactive::{Signal, flush_signal_dirty};
use mirui::ecs::{Entity, World};
use mirui::model;
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Text;
use mirui::ui::{Children, IdMap};

use support::tracked_allocations;

struct BranchState(u8);

#[model]
struct ModeModel {
    #[observe]
    mode: u8,
}

#[model]
impl ModeModel {
    fn set_mode(&mut self, mode: u8) {
        self.mode = mode;
    }
}

fn visible_children(world: &World, parent: Entity) -> Vec<Entity> {
    world
        .get::<Children>(parent)
        .unwrap()
        .0
        .iter()
        .copied()
        .filter(|&entity| !ui::branch::is_effectively_hidden(world, entity))
        .collect()
}

#[test]
fn no_binding_match_keeps_all_roots_state_and_static_sibling_order() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let mode = Signal::new(Some(0u8));
    let selection = mode.clone();
    let built_zero = Rc::new(Cell::new(0));
    let built_other = Rc::new(Cell::new(0));
    let count_zero = Rc::clone(&built_zero);
    let count_other = Rc::clone(&built_other);

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column () {
            Text ("before", id: "before")
            match $selection {
                Some(0) => {
                    ${ count_zero.set(count_zero.get() + 1); }
                    View (id: "zero-a", width: 20, height: 10)
                    View (id: "zero-b", width: 20, height: 10)
                }
                Some(_) => {
                    ${ count_other.set(count_other.get() + 1); }
                    View (id: "other-a", width: 20, height: 10)
                    View (id: "other-b", width: 20, height: 10)
                }
                None => {}
            }
            Text ("after", id: "after")
        }
    };

    let column = world.get::<Children>(root).unwrap().0[0];
    let lookup = |id| world.find_by_id(id).unwrap();
    let before = lookup("before");
    let zero_a = lookup("zero-a");
    let zero_b = lookup("zero-b");
    let other_a = lookup("other-a");
    let other_b = lookup("other-b");
    let after = lookup("after");
    let all_children = [before, zero_a, zero_b, other_a, other_b, after];
    assert_eq!(world.get::<Children>(column).unwrap().0, all_children);
    assert_eq!(
        visible_children(&world, column),
        [before, zero_a, zero_b, after]
    );
    assert_eq!((built_zero.get(), built_other.get()), (1, 1));

    world.insert(zero_a, BranchState(73));
    assert_eq!(
        tracked_allocations(|| {
            mode.set(Some(1));
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_eq!(
        visible_children(&world, column),
        [before, other_a, other_b, after]
    );

    assert_eq!(
        tracked_allocations(|| {
            mode.set(Some(2));
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_eq!(
        visible_children(&world, column),
        [before, other_a, other_b, after]
    );

    assert_eq!(
        tracked_allocations(|| {
            mode.set(None);
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_eq!(visible_children(&world, column), [before, after]);

    assert_eq!(
        tracked_allocations(|| {
            mode.set(Some(0));
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_eq!(world.get::<Children>(column).unwrap().0, all_children);
    assert_eq!(
        visible_children(&world, column),
        [before, zero_a, zero_b, after]
    );
    assert_eq!(world.get::<BranchState>(zero_a).unwrap().0, 73);
    assert_eq!((built_zero.get(), built_other.get()), (1, 1));
    assert!(world.is_alive(other_a));
    assert!(world.is_alive(other_b));
}

#[test]
fn payload_binding_match_still_rebuilds_the_selected_arm() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let payload = Signal::new(None::<&'static str>);
    let selection = payload.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column () {
            Text ("before")
            match $selection {
                Some(value) => {
                    Text (value)
                    Text ("suffix")
                }
                None => {}
            }
            Text ("after")
        }
    };

    let column = world.get::<Children>(root).unwrap().0[0];
    let empty_initial = world.get::<Children>(column).unwrap().0.clone();
    assert_eq!(empty_initial.len(), 2);

    payload.set(Some("alpha"));
    flush_signal_dirty(&mut world);
    let initial = world.get::<Children>(column).unwrap().0.clone();
    assert_eq!(initial.len(), 4);
    assert_eq!(initial[0], empty_initial[0]);
    assert_eq!(initial[3], empty_initial[1]);
    let mounted = initial[1];
    let suffix = initial[2];
    assert_eq!(world.get::<Text>(mounted).unwrap().resolve(&world), "alpha");

    payload.set(Some("beta"));
    flush_signal_dirty(&mut world);
    let updated = &world.get::<Children>(column).unwrap().0;
    assert_eq!(updated.len(), 4);
    assert_eq!(updated[0], initial[0]);
    assert_eq!(updated[3], initial[3]);
    assert_ne!(updated[1], mounted);
    assert!(!world.is_alive(mounted));
    assert!(!world.is_alive(suffix));
    assert_eq!(
        world.get::<Text>(updated[1]).unwrap().resolve(&world),
        "beta"
    );

    payload.set(None);
    flush_signal_dirty(&mut world);
    let empty = &world.get::<Children>(column).unwrap().0;
    assert_eq!(empty, &[initial[0], initial[3]]);

    payload.set(Some("gamma"));
    flush_signal_dirty(&mut world);
    let remounted = &world.get::<Children>(column).unwrap().0;
    assert_eq!(remounted.len(), 4);
    assert_eq!(remounted[0], initial[0]);
    assert_eq!(remounted[3], initial[3]);
    assert_eq!(
        world.get::<Text>(remounted[1]).unwrap().resolve(&world),
        "gamma"
    );
}

#[test]
fn model_observation_switches_a_cached_match_without_allocating() {
    let mut app = mirui::app::App::headless(64, 64);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    let model = app.add_model(ModeModel { mode: 0 });
    let selection = model.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column () {
            match ${ selection.mode() } {
                0 => { View (id: "zero", width: 20, height: 20) }
                _ => { View (id: "other", width: 20, height: 20) }
            }
        }
    };

    let zero = app.world.find_by_id("zero").unwrap();
    let other = app.world.find_by_id("other").unwrap();
    assert!(!ui::branch::is_effectively_hidden(&app.world, zero));
    assert!(ui::branch::is_effectively_hidden(&app.world, other));

    for (mode, visible, hidden) in [(1, other, zero), (0, zero, other)] {
        assert_eq!(
            tracked_allocations(|| {
                model.set_mode(mode);
                flush_signal_dirty(&mut app.world);
            }),
            0,
        );
        assert!(!ui::branch::is_effectively_hidden(&app.world, visible));
        assert!(ui::branch::is_effectively_hidden(&app.world, hidden));
    }
}
