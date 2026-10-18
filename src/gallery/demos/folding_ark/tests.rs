use super::shell;
use super::state::{FoldHintService, FoldSurface, select_map_level};
use crate::core::model::{Model, ModelHandle};
use crate::ecs::World;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel,
};
use crate::gallery::play::fold::{FoldMessage, FoldModel, SAVE_LEN};
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, Hidden, HitTarget, InteractionFeedback};

type Handles = (
    <FoldModel as Model>::Handle,
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

fn with_game<R>(game: &<FoldModel as Model>::Handle, inspect: impl FnOnce(&FoldModel) -> R) -> R {
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

#[derive(Default)]
struct UnhandledKey(Option<PhysicalKey>);

#[event]
fn unhandled_key(input: event!(Key), seen: res!(mut UnhandledKey)) -> bool {
    seen.0 = input.physical;
    true
}

fn send_key(world: &mut World, root: Entity, position: PhysicalKey) {
    crate::input::event::dispatch_input(
        world,
        Some(root),
        &crate::test_support::physical_key(position).into(),
        0,
        480,
        320,
    );
}

fn game_snapshot(model: &FoldModel) -> ([u8; SAVE_LEN], FoldMessage, ExpeditionModal) {
    let mut saved = [0; SAVE_LEN];
    model.encode_into(&mut saved).unwrap();
    (saved, model.message(), model.modal())
}

#[test]
fn registered_keyboard_preserves_commands_and_bound_model_instances() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    let observer = game.clone();
    let panel = expedition.clone();
    let independent = app.add_model(FoldModel::default()).finish();
    let independent_panel = app.add_model(ExpeditionUiModel::default()).finish();
    independent_panel.open_summary();
    app.add_resource(UnhandledKey::default());
    app.on_event(Key::EVENT, unhandled_key::callback());
    drop(game);
    drop(expedition);

    for (position, direction) in [
        (PhysicalKey::KeyW, Direction4::Up),
        (PhysicalKey::KeyD, Direction4::Right),
        (PhysicalKey::KeyS, Direction4::Down),
        (PhysicalKey::KeyA, Direction4::Left),
    ] {
        observer.restart();
        let mut expected = with_game(&observer, |game| *game);
        expected.move_direction(direction);
        send_key(&mut app.world, root, position);
        assert_eq!(
            with_game(&observer, game_snapshot),
            game_snapshot(&expected)
        );
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }

    observer.restart();
    let direction = with_game(&observer, |game| game.level().solution(0).unwrap());
    observer.move_direction(direction);
    assert_eq!(observer.steps(), 1);
    for _ in 0..2 {
        let position = PhysicalKey::KeyZ;
        send_key(&mut app.world, root, position);
        assert_eq!(observer.steps(), 0);
        assert_eq!(observer.message(), FoldMessage::Undone);
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    for _ in 0..2 {
        let position = PhysicalKey::KeyH;
        send_key(&mut app.world, root, position);
        assert!(matches!(observer.message(), FoldMessage::Hint(_, _)));
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }

    panel.open_rules();
    let before = with_game(&observer, game_snapshot);
    for position in [
        PhysicalKey::KeyW,
        PhysicalKey::KeyD,
        PhysicalKey::KeyZ,
        PhysicalKey::KeyH,
    ] {
        send_key(&mut app.world, root, position);
        assert_eq!(with_game(&observer, game_snapshot), before);
        assert_eq!(
            app.world.resource::<UnhandledKey>().unwrap().0,
            Some(position)
        );
    }
    panel.close();
    send_key(&mut app.world, root, PhysicalKey::KeyQ);
    assert_eq!(with_game(&observer, game_snapshot), before);
    assert_eq!(
        app.world.resource::<UnhandledKey>().unwrap().0,
        Some(PhysicalKey::KeyQ)
    );

    observer.restart();
    let solution_len = with_game(&observer, |game| game.level().solution_len());
    for step in 0..solution_len {
        let direction = with_game(&observer, |game| game.level().solution(step).unwrap());
        observer.move_direction(direction);
    }
    assert_ne!(observer.modal(), ExpeditionModal::None);
    let completed = with_game(&observer, game_snapshot);
    app.world.resource_mut::<UnhandledKey>().unwrap().0 = None;
    for position in [PhysicalKey::KeyW, PhysicalKey::KeyZ, PhysicalKey::KeyH] {
        send_key(&mut app.world, root, position);
        assert_eq!(with_game(&observer, game_snapshot), completed);
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    assert_eq!(
        with_game(&independent, game_snapshot),
        game_snapshot(&FoldModel::default())
    );
    assert_eq!(independent_panel.panel(), ExpeditionPanel::Summary);
}

#[test]
fn keyboard_refetches_hints_and_preserves_observed_revision_contracts() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    let observer = game.clone();
    drop(game);
    drop(expedition);
    ViewRegistry::reconcile_observations(&mut app.world);
    flush(&mut app.world);

    let retained = app.world.resource::<FoldHintService>().unwrap().clone();
    app.add_resource(FoldHintService::new());
    let surface = app.world.find_by_id("fold_surface").unwrap();
    app.world.remove::<VisualDirty>(surface);
    let visual_revision = observer.visual_revision();
    let saved = with_game(&observer, game_snapshot).0;
    send_key(&mut app.world, root, PhysicalKey::KeyH);
    let hint = observer.message();
    assert!(matches!(hint, FoldMessage::Hint(_, _)));
    assert_ne!(with_game(&observer, game_snapshot).0, saved);
    assert_eq!(observer.visual_revision(), visual_revision);
    flush(&mut app.world);
    assert!(!app.world.has::<VisualDirty>(surface));

    let hinted = with_game(&observer, game_snapshot).0;
    send_key(&mut app.world, root, PhysicalKey::KeyH);
    assert_eq!(observer.message(), hint);
    assert_eq!(with_game(&observer, game_snapshot).0, hinted);
    assert_eq!(observer.visual_revision(), visual_revision);
    flush(&mut app.world);
    assert!(!app.world.has::<VisualDirty>(surface));

    observer.restart();
    let before = with_game(&observer, game_snapshot);
    app.world.remove_resource::<FoldHintService>().unwrap();
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        send_key(&mut app.world, root, PhysicalKey::KeyH);
    }));
    assert!(error.is_err());
    assert_eq!(with_game(&observer, game_snapshot), before);
    retained.request(&observer);
    assert!(matches!(observer.message(), FoldMessage::Hint(_, _)));

    observer.restart();
    app.add_resource(FoldHintService::new());
    send_key(&mut app.world, root, PhysicalKey::KeyH);
    let restored = with_game(&observer, game_snapshot).0;
    assert!(matches!(observer.message(), FoldMessage::Hint(_, _)));
    send_key(&mut app.world, root, PhysicalKey::KeyH);
    assert_eq!(with_game(&observer, game_snapshot).0, restored);
}

#[test]
fn composition_binds_game_and_expedition_models() {
    let (world, (game, expedition)) = fixture();
    assert_eq!(world.query::<FoldSurface>().collect().len(), 1);
    assert!(world.find_by_id("fold_next").is_some());
    assert!(world.find_by_id("fold_map").is_some());
    assert_eq!(game.level_index(), 0);
    assert_eq!(expedition.panel(), ExpeditionPanel::None);
}

#[test]
fn expedition_panels_and_locked_levels_follow_bound_state() {
    let (mut world, (game, expedition)) = fixture();
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
    let (mut world, (game, expedition)) = fixture();
    let surface = world.find_by_id("fold_surface").unwrap();
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
    assert!(!world.has::<Hidden>(world.find_by_id("fold_result").unwrap()));

    game.restart();
    expedition.open_summary();
    flush(&mut world);
    let summary = world.find_by_id("fold_summary").unwrap();
    assert!(!world.has::<Hidden>(summary));
    let text = world
        .get::<Text>(world.find_by_id("fold_summary_line_0").unwrap())
        .unwrap();
    assert!(text.resolve(world.resource()).contains("1/6"));
    assert!(text.has_valid_content());
    assert_eq!(text.last_content_error(), None);
}

#[test]
fn messages_and_hints_update_observed_text_without_invalidating_surface() {
    let (mut world, (game, _expedition)) = fixture();
    let surface = world.find_by_id("fold_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let visual_revision = game.visual_revision();
    let saved = with_game(&game, game_snapshot).0;

    let blocked = with_game(&game, |model| {
        Direction4::ALL
            .into_iter()
            .find(|direction| {
                let mut probe = *model;
                !probe.move_direction(*direction).contains(ChangeSet::VISUAL)
            })
            .expect("initial Fold state has an unsupported direction")
    });
    let changes = game.move_direction(blocked);
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(!changes.contains(ChangeSet::VISUAL));
    assert!(!changes.contains(ChangeSet::PERSISTENCE));
    flush(&mut world);
    assert_eq!(game.visual_revision(), visual_revision);
    assert_eq!(with_game(&game, game_snapshot).0, saved);
    let status = world
        .get::<Text>(world.find_by_id("fold_status").unwrap())
        .unwrap();
    assert!(status.resolve(world.resource()).contains("没有执行"));

    let changes = game.request_hint(&mut ExpeditionHintWorkspace::new());
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::PERSISTENCE));
    assert!(!changes.contains(ChangeSet::VISUAL));
    flush(&mut world);

    assert_eq!(game.visual_revision(), visual_revision);
    assert_ne!(with_game(&game, game_snapshot).0, saved);
    assert!(!world.has::<VisualDirty>(surface));
    let status = world
        .get::<Text>(world.find_by_id("fold_status").unwrap())
        .unwrap();
    assert!(status.resolve(world.resource()).contains("提示"));
}
