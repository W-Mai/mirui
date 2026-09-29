use super::geometry::board_geometry;
use super::setup_app;
use super::state::{PictureExpeditionState, PictureSurface};
use crate::gallery::play::picture::{PictureModel, PictureModelHandle};
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, Hidden, HitTarget, InteractionFeedback};

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
