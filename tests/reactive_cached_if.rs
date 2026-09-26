mod support {
    include!("support/tracking_allocator.rs");
}

use std::cell::Cell;
use std::rc::Rc;

use mirui::core::reactive::{Signal, flush_signal_dirty};
use mirui::ecs::{Entity, World};
use mirui::render::{DrawCommand, DrawRequest, RenderError, RenderRoute, Renderer};
use mirui::types::{Color, Dimension, Fixed, Viewport};
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;
use mirui::ui::{Children, Hidden, IdMap, InteractionState};
use mirui::{input::event::hit_test::hit_test, ui};

use support::tracked_allocations;

struct BranchState(u32);

#[derive(Default)]
struct FillColors(Vec<Color>);

impl Renderer for FillColors {
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        if let DrawCommand::Fill { color, .. } = request.command {
            self.0.push(*color);
        }
        Ok(())
    }

    fn flush(&mut self) {}
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
fn reactive_if_keeps_all_roots_and_state_at_a_stable_position() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let selected = Signal::new(false);
    let condition = selected.clone();
    let built_true = Rc::new(Cell::new(0));
    let built_false = Rc::new(Cell::new(0));
    let true_counter = Rc::clone(&built_true);
    let false_counter = Rc::clone(&built_false);

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column () {
            Text ("before", id: "before")
            if $condition {
                ${ true_counter.set(true_counter.get() + 1); }
                View (id: "true-a", width: 30, height: 10)
                View (id: "true-b", width: 30, height: 10)
            } else {
                ${ false_counter.set(false_counter.get() + 1); }
                View (id: "false-a", width: 30, height: 10)
                View (id: "false-b", width: 30, height: 10)
            }
            Text ("after", id: "after")
        }
    };

    let column = world.get::<Children>(root).unwrap().0[0];
    let all_children = world.get::<Children>(column).unwrap().0.clone();
    let lookup = |id| world.find_by_id(id).unwrap();
    let before = lookup("before");
    let true_a = lookup("true-a");
    let true_b = lookup("true-b");
    let false_a = lookup("false-a");
    let false_b = lookup("false-b");
    let after = lookup("after");
    assert_eq!(
        all_children,
        [before, true_a, true_b, false_a, false_b, after]
    );
    assert_eq!(built_true.get(), 1);
    assert_eq!(built_false.get(), 1);
    assert_eq!(
        visible_children(&world, column),
        [before, false_a, false_b, after]
    );

    world.insert(true_a, BranchState(73));
    let first_switch_allocations = tracked_allocations(|| {
        selected.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(first_switch_allocations, 0);
    assert_eq!(
        visible_children(&world, column),
        [before, true_a, true_b, after]
    );
    assert_eq!(world.get::<BranchState>(true_a).unwrap().0, 73);

    let repeated_switch_allocations = tracked_allocations(|| {
        selected.set(false);
        flush_signal_dirty(&mut world);
        selected.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(repeated_switch_allocations, 0);
    assert_eq!(world.get::<Children>(column).unwrap().0, all_children);
    assert_eq!(world.get::<BranchState>(true_a).unwrap().0, 73);
    assert_eq!(built_true.get(), 1);
    assert_eq!(built_false.get(), 1);
}

#[test]
fn hidden_if_branches_leave_layout_and_hit_testing() {
    let mut app = mirui::app::App::headless(80, 80);
    app.with_default_widgets();
    let mut world = app.world;
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let selected = Signal::new(false);
    let condition = selected.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column (width: Dimension::px(80), height: Dimension::px(80)) {
            if $condition {
                View (id: "enabled", width: 40, height: 30, bg_color: Color::rgb(255, 0, 0)) on Tap {}
            } else {
                View (id: "disabled", width: 40, height: 30, bg_color: Color::rgb(0, 0, 255)) on Tap {}
            }
        }
    };

    let enabled = world.find_by_id("enabled").unwrap();
    let disabled = world.find_by_id("disabled").unwrap();
    let viewport = Viewport::new(80, 80, Fixed::ONE);
    render_system::update_layout(&mut world, root, &viewport);
    assert_eq!(
        hit_test(&world, root, 10.into(), 10.into(), 80, 80),
        Some(disabled)
    );
    let mut paints = FillColors::default();
    render_system::render(&world, root, &viewport, &mut paints).unwrap();
    assert_eq!(paints.0, [Color::rgb(0, 0, 255)]);
    world.insert(disabled, InteractionState::Hovered);

    let switch_allocations = tracked_allocations(|| {
        selected.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(switch_allocations, 0);
    assert!(world.get::<InteractionState>(disabled).is_none());
    assert_eq!(hit_test(&world, root, 10.into(), 10.into(), 80, 80), None);
    render_system::update_layout(&mut world, root, &viewport);
    assert_eq!(
        hit_test(&world, root, 10.into(), 10.into(), 80, 80),
        Some(enabled)
    );
    paints.0.clear();
    render_system::render(&world, root, &viewport, &mut paints).unwrap();
    assert_eq!(paints.0, [Color::rgb(255, 0, 0)]);
    assert!(ui::branch::is_effectively_hidden(&world, disabled));
    assert!(!ui::branch::is_effectively_hidden(&world, enabled));
}

#[test]
fn reactive_if_without_else_can_hide_every_arm() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let mode = Signal::new(0u8);
    let condition = mode.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column () {
            if ${ condition.get() == 1 } {
                View (id: "one", width: 20, height: 20)
            } elif ${ condition.get() == 2 } {
                View (id: "two", width: 20, height: 20)
            }
        }
    };

    let column = world.get::<Children>(root).unwrap().0[0];
    let one = world.find_by_id("one").unwrap();
    let two = world.find_by_id("two").unwrap();
    assert!(visible_children(&world, column).is_empty());

    let allocations = tracked_allocations(|| {
        mode.set(1);
        flush_signal_dirty(&mut world);
        mode.set(2);
        flush_signal_dirty(&mut world);
        mode.set(0);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(allocations, 0);
    assert!(visible_children(&world, column).is_empty());
    assert!(world.is_alive(one));
    assert!(world.is_alive(two));
}

#[test]
fn branch_visibility_does_not_override_a_roots_own_visible_binding() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let selected = Signal::new(false);
    let own_visible = Signal::new(false);
    let condition = selected.clone();
    let own_binding = own_visible.clone();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        Column () {
            if $condition {
                View (id: "gated", visible: $own_binding, width: 20, height: 20)
            } else {
                View (id: "fallback", width: 20, height: 20)
            }
        }
    };

    let gated = world.find_by_id("gated").unwrap();
    let fallback = world.find_by_id("fallback").unwrap();
    assert!(world.has::<Hidden>(gated));
    assert!(!world.has::<Hidden>(fallback));

    let first_select_allocations = tracked_allocations(|| {
        selected.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(first_select_allocations, 0);
    assert!(world.has::<Hidden>(gated));
    assert!(ui::branch::is_effectively_hidden(&world, fallback));

    let own_visibility_allocations = tracked_allocations(|| {
        own_visible.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(own_visibility_allocations, 0);
    assert!(!world.has::<Hidden>(gated));

    selected.set(false);
    flush_signal_dirty(&mut world);
    assert!(ui::branch::is_effectively_hidden(&world, gated));
    own_visible.set(false);
    flush_signal_dirty(&mut world);
    own_visible.set(true);
    flush_signal_dirty(&mut world);
    assert!(!world.has::<Hidden>(gated));
    assert!(ui::branch::is_effectively_hidden(&world, gated));
    assert!(!ui::branch::is_effectively_hidden(&world, fallback));

    selected.set(true);
    flush_signal_dirty(&mut world);
    assert!(!world.has::<Hidden>(gated));
}
