use super::geometry::board_geometry;
use super::setup_app;
use super::state::{PictureExpeditionState, PictureSurface};
use crate::core::model::ModelHandle;
use crate::gallery::play::picture::{PictureModel, PictureModelHandle};
use crate::input::event::GestureHandler;
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

fn handles(
    world: &World,
) -> (
    PictureModelHandle,
    crate::gallery::play::expeditions::ExpeditionUiModelHandle,
) {
    let surface = world.find_by_id("picture_surface").unwrap();
    (
        world.get::<PictureSurface>(surface).unwrap().model.clone(),
        world
            .get::<PictureExpeditionState>(surface)
            .unwrap()
            .expedition
            .clone(),
    )
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

fn trigger(world: &mut World, surface: Entity, event: &GestureEvent) -> bool {
    GestureHandler::trigger(world, surface, event).unwrap_or(false)
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
fn surface_gesture_uses_the_bound_picture_model() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        },
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
}

#[test]
fn stroke_completion_does_not_depend_on_layout_components() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        },
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        },
    ));
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 0);

    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        },
    ));
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        surface,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        },
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
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
