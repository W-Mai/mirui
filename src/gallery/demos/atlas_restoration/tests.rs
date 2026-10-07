use super::geometry::board_geometry;
use super::setup_app;
use super::state::PictureSurface;
use crate::core::model::ModelHandle;
use crate::ecs::World;
use crate::gallery::play::expeditions::{ExpeditionModal, ExpeditionUiModelHandle};
use crate::gallery::play::picture::{PictureModel, PictureModelHandle};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, ComputedRect, Hidden, HitTarget, InteractionFeedback};

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn handles(world: &World) -> (PictureModelHandle, ExpeditionUiModelHandle) {
    let surface = world.find_by_id("picture_surface").unwrap();
    let surface = world.get::<PictureSurface>(surface).unwrap();
    (surface.model.clone(), surface.expedition.clone())
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn editable_cell(model: &PictureModelHandle) -> (u8, i32, i32) {
    ModelHandle::read(model, |model| {
        let size = model.level().size();
        let cell = (0..size * size)
            .find(|cell| !model.level().is_given(*cell))
            .expect("the first picture has an editable cell");
        let geometry = board_geometry(model);
        let x = geometry.x + i32::from(cell % size) * geometry.cell + 1;
        let y = geometry.y + i32::from(cell / size) * geometry.cell + 1;
        (cell, x, y)
    })
}

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    }
}

fn visible_interactive_descendant(world: &World, entity: Entity) -> bool {
    if crate::ui::branch::is_effectively_hidden(world, entity) {
        return false;
    }
    if world.has::<HitTarget>(entity) || world.has::<InteractionFeedback>(entity) {
        return true;
    }
    world.get::<Children>(entity).is_some_and(|children| {
        children
            .0
            .iter()
            .copied()
            .any(|child| visible_interactive_descendant(world, child))
    })
}

#[test]
fn composition_binds_game_and_expedition_models() {
    let mut world = fixture();
    let (model, expedition) = handles(&world);
    assert!(world.find_by_id("picture_fill").is_some());
    assert!(world.find_by_id("pic_col_9").is_some());
    assert_eq!(world.query::<PictureSurface>().iter().count(), 1);

    let map = world.find_by_id("picture_map").unwrap();
    assert!(world.has::<Hidden>(map));
    let locked = world.find_by_id("picture_map_level_1").unwrap();
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open(model.level_index());
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open_rules();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_rules").unwrap()));
    expedition.close();
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("picture_rules").unwrap()));
}

#[test]
fn surface_event_uses_the_bound_picture_model() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
}

#[test]
fn surface_preserves_consumption_when_layout_is_unavailable() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.remove::<ComputedRect>(surface);
    let (model, expedition) = handles(&world);
    let (_, x, y) = editable_cell(&model);
    let tap = GestureEvent::Tap {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y),
        target: surface,
    };
    let drag_start = GestureEvent::DragStart {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y),
        target: surface,
    };
    let drag_move = GestureEvent::DragMove {
        x: Fixed::from_int(x + 10),
        y: Fixed::from_int(y),
        dx: Fixed::from_int(10),
        dy: Fixed::ZERO,
        target: surface,
    };

    assert!(!trigger(&mut world, &tap));
    assert!(!trigger(&mut world, &drag_start));
    assert!(trigger(&mut world, &drag_move));

    expedition.open_rules();
    assert!(trigger(&mut world, &tap));
    assert!(trigger(&mut world, &drag_start));
    assert!(trigger(&mut world, &drag_move));
}

#[test]
fn overlays_block_surface_taps_drags_and_keyboard_commands() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, expedition) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));
    let cursor = ModelHandle::read(&model, PictureModel::cursor);

    expedition.open_rules();
    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragMove {
            x: Fixed::from_int(x + 10),
            y: Fixed::from_int(y),
            dx: Fixed::from_int(10),
            dy: Fixed::ZERO,
            target: surface,
        }
    ));
    crate::input::event::dispatch_input(
        &mut world,
        None,
        &crate::surface::InputEvent::CharInput { ch: 'd' },
        0,
        480,
        320,
    );
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(ModelHandle::read(&model, PictureModel::cursor), cursor);
    assert_eq!(model.history_len(), 0);

    expedition.close();
    while model.modal() == ExpeditionModal::None {
        model.reveal_hint();
    }
    assert_eq!(model.modal(), ExpeditionModal::Result);
    let cursor = ModelHandle::read(&model, PictureModel::cursor);
    let tool = model.tool();
    let history_len = model.history_len();
    for ch in ['d', 'x'] {
        crate::input::event::dispatch_input(
            &mut world,
            None,
            &crate::surface::InputEvent::CharInput { ch },
            1,
            480,
            320,
        );
    }
    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_eq!(ModelHandle::read(&model, PictureModel::cursor), cursor);
    assert_eq!(model.tool(), tool);
    assert_eq!(model.history_len(), history_len);
}

#[test]
fn stroke_completion_does_not_depend_on_layout_components() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    let (model, expedition) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    expedition.open_rules();
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 0);

    expedition.close();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    expedition.open_rules();
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
}

#[test]
fn keyboard_commands_wait_for_the_active_stroke_transaction() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));
    let hints = model.hints();
    let history_len = model.history_len();

    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    crate::input::event::dispatch_input(
        &mut world,
        None,
        &crate::surface::InputEvent::CharInput { ch: 'h' },
        0,
        480,
        320,
    );
    assert_eq!(model.hints(), hints);
    assert_eq!(model.history_len(), history_len);

    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_eq!(model.history_len(), history_len + 1);
    model.undo();
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
}

#[derive(Default)]
struct UnhandledCharacter(Option<char>);

#[event]
fn unhandled_character(input: event!(CharInput), seen: res!(mut UnhandledCharacter)) -> bool {
    seen.0 = Some(input.ch);
    true
}

#[test]
fn registered_keyboard_uses_both_model_instances_and_leaves_unknown_characters_unhandled() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    let (model, expedition) = handles(&app.world);
    let observer = model.clone();
    let panel = expedition.clone();
    app.world.insert_resource(UnhandledCharacter::default());
    app.on_event(CharInput::EVENT, unhandled_character::callback());
    drop(model);
    drop(expedition);

    let before = observer.read(PictureModel::cursor);
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::surface::InputEvent::CharInput { ch: 'D' },
        0,
        480,
        320,
    );
    let moved = observer.read(PictureModel::cursor);
    assert_ne!(moved, before);
    assert_eq!(app.world.resource::<UnhandledCharacter>().unwrap().0, None);

    panel.open_rules();
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::surface::InputEvent::CharInput { ch: 'a' },
        1,
        480,
        320,
    );
    assert_eq!(observer.read(PictureModel::cursor), moved);
    assert_eq!(
        app.world.resource::<UnhandledCharacter>().unwrap().0,
        Some('a')
    );

    panel.close();
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::surface::InputEvent::CharInput { ch: 'q' },
        2,
        480,
        320,
    );
    assert_eq!(observer.read(PictureModel::cursor), moved);
    assert_eq!(
        app.world.resource::<UnhandledCharacter>().unwrap().0,
        Some('q')
    );
}

#[test]
fn model_changes_drive_board_result_and_bounded_text() {
    let mut world = fixture();
    let (model, expedition) = handles(&world);
    let surface = world.find_by_id("picture_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let revision = model.visual_revision();

    model.reveal_hint();
    flush(&mut world);
    assert!(model.visual_revision() > revision);
    assert!(world.has::<VisualDirty>(surface));

    for _ in 1..25 {
        model.reveal_hint();
    }
    flush(&mut world);
    assert!(model.progress().completed(0));
    assert!(!world.has::<Hidden>(world.find_by_id("picture_result").unwrap()));
    expedition.open(model.level_index());
    flush(&mut world);
    assert!(visible_interactive_descendant(
        &world,
        world.find_by_id("picture_map_level_1").unwrap()
    ));

    model.restart();
    expedition.open_briefing();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_briefing").unwrap()));
    expedition.open_summary();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_summary").unwrap()));
    assert!(
        world
            .get::<Text>(world.find_by_id("picture_summary_line_0").unwrap())
            .unwrap()
            .resolve(&world)
            .contains("1/6")
    );

    for id in [
        "picture_level",
        "picture_progress",
        "picture_status",
        "picture_map_summary",
        "picture_result_stats",
        "picture_briefing_title",
        "picture_summary_line_0",
        "pic_row_0",
        "pic_col_0",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.text_capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn first_picture_matches_reference_geometry() {
    let model = PictureModel::default();
    let geometry = board_geometry(&model);
    assert_eq!(
        (geometry.x, geometry.y, geometry.cell, geometry.size),
        (112, 81, 26, 130)
    );
}
