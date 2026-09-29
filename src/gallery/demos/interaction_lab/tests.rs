use super::runtime::install;
use super::state::InteractionModel;
use super::*;
use crate::core::model::ModelHandle;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::focus::{FocusState, Focusable, focus_on_tap};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::input::event::{bubble_dispatch_at, entity_or_ancestor_disabled};
use crate::input::feedback::{InputFeedback, InputFeedbackInput};
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::UserState;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Button, Checkbox, Switch, Text, TextInput};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InteractionSnapshot {
    single: u16,
    double: u16,
    triple: u16,
    long: u16,
    switch_on: bool,
    switch_changes: u16,
    checkbox_on: bool,
    checkbox_changes: u16,
    drag_x: Fixed,
    drag_y: Fixed,
    child_taps: u16,
    parent_taps: u16,
}

type InteractionHandle = <InteractionModel as crate::core::model::Model>::Handle;

struct FixtureModel(InteractionHandle);

fn fixture_tree() -> (World, Entity) {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    let model = install(&mut app, parent);
    app.world.insert_resource(FixtureModel(model));
    ViewRegistry::reconcile_observations(&mut app.world);
    flush_signal_dirty(&mut app.world);
    (app.world, parent)
}

fn fixture() -> World {
    fixture_tree().0
}

fn model_handle(world: &World) -> InteractionHandle {
    world.resource::<FixtureModel>().unwrap().0.clone()
}

fn state(world: &World) -> InteractionSnapshot {
    ModelHandle::read(&model_handle(world), |model| InteractionSnapshot {
        single: model.single(),
        double: model.double(),
        triple: model.triple(),
        long: model.long(),
        switch_on: model.switch_on(),
        switch_changes: model.switch_changes(),
        checkbox_on: model.checkbox_on(),
        checkbox_changes: model.checkbox_changes(),
        drag_x: model.drag_x(),
        drag_y: model.drag_y(),
        child_taps: model.child_taps(),
        parent_taps: model.parent_taps(),
    })
}

fn tap(world: &mut World, id: &'static str, now_ms: u32) {
    let target = world.find_by_id(id).expect("interaction id");
    bubble_dispatch_at(
        world,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        },
        now_ms,
    );
    flush_signal_dirty(world);
}

#[test]
fn preserves_the_complete_interaction_matrix() {
    let world = fixture();
    assert!(world.resource::<InteractionModel>().is_none());
    for id in [
        "interaction_single",
        "interaction_double",
        "interaction_triple",
        "interaction_long",
        "interaction_drag_target",
        "interaction_hover_target",
        "interaction_press_target",
        "interaction_error_target",
        "interaction_disabled_target",
        "interaction_focus_target",
        "interaction_bubble_child",
        "interaction_bubble_policy",
        "interaction_switch",
        "interaction_checkbox",
        "interaction_feedback_status",
    ] {
        let entity = world
            .find_by_id(id)
            .unwrap_or_else(|| panic!("missing {id}"));
        if id != "interaction_focus_target"
            && id != "interaction_switch"
            && id != "interaction_checkbox"
            && id != "interaction_feedback_status"
        {
            assert!(world.has::<Button>(entity), "{id} is not a Button");
        }
    }
    assert!(world.has::<Switch>(world.find_by_id("interaction_switch").unwrap()));
    assert!(world.has::<Checkbox>(world.find_by_id("interaction_checkbox").unwrap()));
    assert!(world.has::<TextInput>(world.find_by_id("interaction_focus_target").unwrap()));
    assert_eq!(
        world
            .get::<TouchAction>(world.find_by_id("interaction_drag_target").unwrap())
            .copied(),
        Some(TouchAction::None)
    );
}

#[test]
fn gesture_callbacks_call_bound_model_commands() {
    let mut world = fixture();
    tap(&mut world, "interaction_single", 100);
    tap(&mut world, "interaction_double", 1_000);
    tap(&mut world, "interaction_double", 1_100);
    tap(&mut world, "interaction_triple", 2_000);
    tap(&mut world, "interaction_triple", 2_100);
    tap(&mut world, "interaction_triple", 2_200);
    let target = world.find_by_id("interaction_long").unwrap();
    bubble_dispatch_at(
        &mut world,
        &GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        },
        3_000,
    );
    flush_signal_dirty(&mut world);

    let state = state(&world);
    assert_eq!(
        (state.single, state.double, state.triple, state.long),
        (1, 1, 1, 1)
    );
    for (id, expected) in [
        ("interaction_single_status", "single 1"),
        ("interaction_double_status", "double 1"),
        ("interaction_triple_status", "triple 1"),
        ("interaction_long_status", "long 1"),
    ] {
        let entity = world.find_by_id(id).unwrap();
        let text = world.get::<Text>(entity).unwrap();
        assert_eq!(text.resolve(&world), expected);
        assert!(text.text_capacity().is_some(), "{id}");
    }
}

#[test]
fn unchanged_commands_do_not_dirty_observers() {
    let mut world = fixture();
    let model = model_handle(&world);
    let switch_status = world.find_by_id("interaction_switch_status").unwrap();
    let checkbox_status = world.find_by_id("interaction_checkbox_status").unwrap();
    for entity in [switch_status, checkbox_status] {
        world.remove::<crate::ui::dirty::Dirty>(entity);
        world.remove::<crate::ui::dirty::VisualDirty>(entity);
    }

    model.set_switch(false);
    model.set_checkbox(false);
    flush_signal_dirty(&mut world);
    for entity in [switch_status, checkbox_status] {
        assert!(!world.has::<crate::ui::dirty::Dirty>(entity));
        assert!(!world.has::<crate::ui::dirty::VisualDirty>(entity));
    }
}

#[test]
fn drag_and_bubble_policy_update_only_the_model() {
    let mut world = fixture();
    let drag = world.find_by_id("interaction_drag_target").unwrap();
    bubble_dispatch_at(
        &mut world,
        &GestureEvent::DragMove {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            dx: Fixed::from_int(34),
            dy: Fixed::from_int(18),
            target: drag,
        },
        100,
    );
    flush_signal_dirty(&mut world);
    assert_eq!(
        (state(&world).drag_x, state(&world).drag_y),
        (Fixed::from_int(34), Fixed::from_int(18))
    );
    let drag = world.find_by_id("interaction_drag_target").unwrap();
    let style = world.get::<Style>(drag).unwrap();
    assert_eq!(style.layout.left, Dimension::px(58));
    assert_eq!(style.layout.top, Dimension::px(45));

    tap(&mut world, "interaction_bubble_child", 1_000);
    assert_eq!(
        (state(&world).child_taps, state(&world).parent_taps),
        (1, 0)
    );
    tap(&mut world, "interaction_bubble_policy", 1_500);
    let policy = world.find_by_id("interaction_bubble_policy").unwrap();
    assert_eq!(world.get::<Text>(policy).unwrap().resolve(&world), "ALLOW");
    assert!(
        world
            .get::<Children>(policy)
            .is_none_or(|children| children.0.iter().all(|child| !world.has::<Text>(*child)))
    );
    tap(&mut world, "interaction_bubble_child", 2_000);
    assert_eq!(
        (state(&world).child_taps, state(&world).parent_taps),
        (2, 1)
    );
}

#[test]
fn observed_model_state_projects_to_user_state_and_focus() {
    let mut world = fixture();
    let error = world.find_by_id("interaction_error_target").unwrap();
    let disabled = world.find_by_id("interaction_disabled_target").unwrap();
    assert!(matches!(
        world.get::<UserState>(error),
        Some(UserState::Errored)
    ));
    assert!(entity_or_ancestor_disabled(&world, disabled));

    tap(&mut world, "interaction_error_target", 100);
    tap(&mut world, "interaction_toggle_disabled", 500);
    let error = world.find_by_id("interaction_error_clear_target").unwrap();
    let disabled = world.find_by_id("interaction_enabled_target").unwrap();
    assert!(world.get::<UserState>(error).is_none());
    assert!(!entity_or_ancestor_disabled(&world, disabled));

    let focus = world.find_by_id("interaction_focus_target").unwrap();
    assert!(world.has::<Focusable>(focus));
    world.insert_resource(FocusState::default());
    focus_on_tap(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: focus,
        },
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(focus));
}

#[test]
fn switch_and_checkbox_publish_business_values() {
    let mut world = fixture();
    tap(&mut world, "interaction_switch", 100);
    tap(&mut world, "interaction_checkbox", 500);
    let state = state(&world);
    assert!(state.switch_on);
    assert!(state.checkbox_on);
    assert_eq!((state.switch_changes, state.checkbox_changes), (1, 1));
    for (id, expected) in [
        ("interaction_switch_status", "switch ON / 1 changes"),
        ("interaction_checkbox_status", "check ON / 1 changes"),
    ] {
        let entity = world.find_by_id(id).unwrap();
        let text = world.get::<Text>(entity).unwrap();
        assert_eq!(text.resolve(&world), expected);
        assert!(text.text_capacity().is_some(), "{id}");
    }
}

#[test]
fn model_updates_project_back_into_switch_and_checkbox() {
    let mut world = fixture();
    let switch = world.find_by_id("interaction_switch").unwrap();
    let checkbox = world.find_by_id("interaction_checkbox").unwrap();
    let model = model_handle(&world);

    model.set_switch(true);
    model.set_checkbox(true);
    flush_signal_dirty(&mut world);

    assert!(world.get::<Switch>(switch).unwrap().on);
    assert!(world.get::<Checkbox>(checkbox).unwrap().checked);
}

#[test]
fn setup_installs_input_feedback() {
    use crate::input::event::sim::SimTimeline;

    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);

    assert!(app.world.resource::<InputFeedback>().is_some());
    assert!(app.world.resource::<InputFeedbackInput>().is_some());
    assert!(app.world.resource::<SimTimeline>().is_none());
}

#[test]
fn default_viewport_keeps_the_two_by_two_grid_inside_the_shell() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let (mut world, parent) = fixture_tree();
    update_layout(
        &mut world,
        parent,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );
    let rect = |world: &World, id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let shell = rect(&world, "interaction_lab_shell");
    let gestures = rect(&world, "interaction_gestures");
    let states = rect(&world, "interaction_states");
    let motion = rect(&world, "interaction_motion");
    let controls = rect(&world, "interaction_controls");

    assert_eq!(gestures.y, states.y);
    assert_eq!(motion.y, controls.y);
    assert!(motion.y > gestures.y);
    for card in [gestures, states, motion, controls] {
        assert!(card.x >= shell.x && card.x + card.w <= shell.x + shell.w);
        assert!(card.y >= shell.y && card.y + card.h <= shell.y + shell.h);
    }
}

#[test]
fn dirty_first_frame_reconciles_the_responsive_tree() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::dirty::DirtyRegions;
    use crate::ui::render_system::collect_dirty_regions_into;

    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    setup_app(&mut app, parent);
    app.set_root(parent);
    let viewport = Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE);
    let mut plan = DirtyRegions::default();
    collect_dirty_regions_into(&mut app.world, parent, &viewport, &mut plan);

    let rect = |world: &World, id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let gestures = rect(&app.world, "interaction_gestures");
    let states = rect(&app.world, "interaction_states");
    let motion = rect(&app.world, "interaction_motion");
    let controls = rect(&app.world, "interaction_controls");
    assert_eq!(gestures.y, states.y);
    assert_eq!(motion.y, controls.y);
    assert!(motion.y > gestures.y);
    super::super::assert_text_layouts_fit(&app.world);
    assert!(!plan.is_empty());
}

#[test]
fn phone_viewport_stacks_cards_and_wraps_gesture_targets() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let (mut world, parent) = fixture_tree();
    update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
    let rect = |world: &World, id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let shell = rect(&world, "interaction_lab_shell");
    let gestures = rect(&world, "interaction_gestures");
    for id in [
        "interaction_gestures",
        "interaction_states",
        "interaction_motion",
        "interaction_controls",
    ] {
        let card = rect(&world, id);
        assert!(card.x >= shell.x);
        assert!(card.x + card.w <= shell.x + shell.w);
    }
    for id in [
        "interaction_single",
        "interaction_double",
        "interaction_triple",
        "interaction_long",
    ] {
        let target = rect(&world, id);
        assert!(target.x >= gestures.x);
        assert!(target.x + target.w <= gestures.x + gestures.w);
    }
}

#[test]
fn medium_portrait_cards_fill_the_single_column() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(672, 666), (666, 674), (502, 900), (320, 568)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let parent = app.spawn_root().id();
        setup_app(&mut app, parent);
        app.set_root(parent);
        update_layout(
            &mut app.world,
            parent,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let grid = rect(&app.world, "interaction_lab_grid");
        for id in [
            "interaction_gestures",
            "interaction_states",
            "interaction_motion",
            "interaction_controls",
        ] {
            let card = rect(&app.world, id);
            assert_eq!(card.x, grid.x, "{width}x{height}: {id}");
            assert_eq!(card.w, grid.w, "{width}x{height}: {id}");
        }
        let gestures = rect(&app.world, "interaction_gestures");
        let status = rect(&app.world, "interaction_gesture_status");
        let targets = rect(&app.world, "interaction_gesture_targets");
        let caption = rect(&app.world, "interaction_gesture_caption");
        assert!(
            status.y + status.h <= targets.y,
            "{width}x{height}: gesture status overlaps targets"
        );
        assert!(
            targets.y + targets.h <= caption.y,
            "{width}x{height}: gesture targets overlap caption"
        );
        assert!(
            caption.y + caption.h <= gestures.y + gestures.h,
            "{width}x{height}: gesture caption escapes card"
        );
        super::super::assert_text_layouts_fit(&app.world);
    }
}

#[test]
fn phone_scroll_extent_keeps_the_last_card_reachable() {
    use crate::input::event::scroll::scroll_bounds;
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(320, 568), (422, 600), (480, 320)] {
        let (mut world, parent) = fixture_tree();
        update_layout(
            &mut world,
            parent,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let shell = world.find_by_id("interaction_lab_shell").unwrap();
        let controls = world.find_by_id("interaction_controls").unwrap();
        let shell_rect = world.get::<ComputedRect>(shell).unwrap().0;
        let controls_rect = world.get::<ComputedRect>(controls).unwrap().0;
        let bounds = scroll_bounds(&world, shell).unwrap();
        let extent = bounds.content_height;
        let max_offset = bounds.max_y;

        assert!(max_offset > Fixed::ZERO, "{width}x{height}");
        assert!(
            controls_rect.y + controls_rect.h - max_offset <= shell_rect.y + shell_rect.h,
            "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
        );
        assert!(
            controls_rect.y + controls_rect.h - max_offset > shell_rect.y,
            "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
        );
    }
}
