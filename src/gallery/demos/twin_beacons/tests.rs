use super::shell;
use super::state::{TwinSurface, select_map_level};
use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionHintWorkspace, ExpeditionPanel, ExpeditionUiModel,
};
use crate::gallery::play::twin::TwinModel;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, Hidden, HitTarget, InteractionFeedback};

type Handles = (
    <TwinModel as Model>::Handle,
    <ExpeditionUiModel as Model>::Handle,
);

fn fixture() -> (World, Handles) {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let handles = shell::setup_test_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    (app.world, handles)
}

fn with_game<R>(game: &<TwinModel as Model>::Handle, inspect: impl FnOnce(&TwinModel) -> R) -> R {
    ModelHandle::read(game, inspect)
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
    let (world, (game, expedition)) = fixture();
    assert_eq!(world.query::<TwinSurface>().collect().len(), 1);
    assert!(world.find_by_id("twin_next").is_some());
    assert!(world.find_by_id("twin_map").is_some());
    assert_eq!(game.level_index(), 0);
    assert_eq!(expedition.panel(), ExpeditionPanel::None);
}

#[test]
fn expedition_panels_and_locked_levels_follow_bound_state() {
    let (mut world, (game, expedition)) = fixture();
    let map = world.find_by_id("twin_map").unwrap();
    let locked = world.find_by_id("twin_map_level_1").unwrap();
    assert!(world.has::<Hidden>(map));
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open(game.level_index());
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    select_map_level(&game, &expedition, 1);
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    assert_eq!(game.level_index(), 0);

    select_map_level(&game, &expedition, 0);
    flush(&mut world);
    assert!(world.has::<Hidden>(map));
    expedition.open_rules();
    flush(&mut world);
    let rules = world.find_by_id("twin_rules").unwrap();
    assert!(!world.has::<Hidden>(rules));
    expedition.close();
    flush(&mut world);
    assert!(world.has::<Hidden>(rules));
}

#[test]
fn completion_updates_visual_revision_and_progress_text() {
    let (mut world, (game, expedition)) = fixture();
    let surface = world.find_by_id("twin_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let revision = game.visual_revision();
    let solution_len = with_game(&game, |model| model.level().solution_len());
    for step in 0..solution_len {
        let direction = with_game(&game, |model| model.level().solution(step).unwrap());
        game.move_direction(direction);
    }
    flush(&mut world);

    assert!(game.visual_revision() > revision);
    assert!(world.has::<VisualDirty>(surface));
    assert!(game.progress().completed(0));
    assert!(!world.has::<Hidden>(world.find_by_id("twin_result").unwrap()));

    game.restart();
    expedition.open_summary();
    flush(&mut world);
    let summary = world.find_by_id("twin_summary").unwrap();
    assert!(!world.has::<Hidden>(summary));
    let text = world
        .get::<Text>(world.find_by_id("twin_summary_line_0").unwrap())
        .unwrap();
    assert!(text.resolve(&world).contains("1/6"));
    assert!(text.has_valid_content());
    assert_eq!(text.last_content_error(), None);
}

#[test]
fn messages_and_hints_update_observed_text_without_invalidating_surface() {
    let (mut world, (game, _expedition)) = fixture();
    let surface = world.find_by_id("twin_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let visual_revision = game.visual_revision();
    let persistence_revision = game.persistence_revision();

    let blocked = with_game(&game, |model| {
        Direction4::ALL
            .into_iter()
            .find(|direction| {
                let mut probe = *model;
                !probe.move_direction(*direction).contains(ChangeSet::VISUAL)
            })
            .expect("initial Twin state has a blocked direction")
    });
    let changes = game.move_direction(blocked);
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(!changes.contains(ChangeSet::VISUAL));
    assert!(!changes.contains(ChangeSet::PERSISTENCE));
    flush(&mut world);
    assert_eq!(game.visual_revision(), visual_revision);
    assert_eq!(game.persistence_revision(), persistence_revision);
    let status = world
        .get::<Text>(world.find_by_id("twin_status").unwrap())
        .unwrap();
    assert!(status.resolve(&world).contains("路径受阻"));

    let changes = game.request_hint(&mut ExpeditionHintWorkspace::new());
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::PERSISTENCE));
    assert!(!changes.contains(ChangeSet::VISUAL));
    flush(&mut world);

    assert_eq!(game.visual_revision(), visual_revision);
    assert!(game.persistence_revision() > persistence_revision);
    assert!(!world.has::<VisualDirty>(surface));
    let status = world
        .get::<Text>(world.find_by_id("twin_status").unwrap())
        .unwrap();
    assert!(status.resolve(&world).contains("提示"));
    let steps = world
        .get::<Text>(world.find_by_id("twin_steps").unwrap())
        .unwrap();
    assert!(steps.resolve(&world).contains("HINT 1"));
}
