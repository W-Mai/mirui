use super::setup_app;
use super::state::{FoldSurface, select_map_level};
use crate::core::model::ModelHandle;
use crate::gallery::play::expeditions::{ExpeditionPanel, ExpeditionUiModelHandle};
use crate::gallery::play::fold::{FoldModel, FoldModelHandle};
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

fn handles(world: &World) -> (FoldModelHandle, ExpeditionUiModelHandle) {
    let surface = world.find_by_id("fold_surface").unwrap();
    let binding = world.get::<FoldSurface>(surface).unwrap();
    (binding.game.clone(), binding.expedition.clone())
}

fn with_game<R>(world: &World, inspect: impl FnOnce(&FoldModel) -> R) -> R {
    ModelHandle::read(&handles(world).0, inspect)
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
    let world = fixture();
    assert_eq!(world.query::<FoldSurface>().collect().len(), 1);
    assert!(world.find_by_id("fold_next").is_some());
    assert!(world.find_by_id("fold_map").is_some());
    assert_eq!(handles(&world).0.level_index(), 0);
    assert_eq!(handles(&world).1.panel(), ExpeditionPanel::None);
}

#[test]
fn expedition_panels_and_locked_levels_follow_bound_state() {
    let mut world = fixture();
    let (game, expedition) = handles(&world);
    let map = world.find_by_id("fold_map").unwrap();
    let locked = world.find_by_id("fold_map_level_1").unwrap();
    assert!(world.has::<Hidden>(map));
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open(game.level_index());
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    assert!(!visible_interactive_descendant(&world, locked));
    select_map_level(&game, &expedition, 1);
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    assert_eq!(game.level_index(), 0);

    select_map_level(&game, &expedition, 0);
    flush(&mut world);
    assert!(world.has::<Hidden>(map));
    expedition.open_rules();
    flush(&mut world);
    let rules = world.find_by_id("fold_rules").unwrap();
    assert!(!world.has::<Hidden>(rules));
    expedition.close();
    flush(&mut world);
    assert!(world.has::<Hidden>(rules));
}

#[test]
fn completion_updates_visual_revision_and_progress_text() {
    let mut world = fixture();
    let (game, expedition) = handles(&world);
    let surface = world.find_by_id("fold_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let revision = game.visual_revision();
    let solution_len = with_game(&world, |model| model.level().solution_len());
    for step in 0..solution_len {
        let direction = with_game(&world, |model| model.level().solution(step).unwrap());
        game.move_direction(direction);
    }
    flush(&mut world);

    assert!(game.visual_revision() > revision);
    assert!(world.has::<VisualDirty>(surface));
    assert!(game.progress().completed(0));
    assert!(!world.has::<Hidden>(world.find_by_id("fold_result").unwrap()));

    game.restart();
    expedition.open_summary();
    flush(&mut world);
    let summary = world.find_by_id("fold_summary").unwrap();
    assert!(!world.has::<Hidden>(summary));
    let text = world
        .get::<Text>(world.find_by_id("fold_summary_line_0").unwrap())
        .unwrap();
    assert!(text.resolve(&world).contains("1/6"));
    assert!(text.has_valid_content());
    assert_eq!(text.last_content_error(), None);
}
