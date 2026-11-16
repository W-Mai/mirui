use super::geometry::{hull_rect, panel_box};
use super::shell;
use super::state::{FoldHintService, FoldSurface, FoldTrack, MapTarget, next, select_map_level};
use crate::core::i18n::{I18n, Locale};
use crate::core::model::{Model, ModelHandle};
use crate::ecs::World;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    BoardMotion, Direction4, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel,
    ExpeditionUiModel, ResultStars,
};
use crate::gallery::play::fold::{CHAPTER_TIPS, FoldMessage, FoldModel, SAVE_LEN};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, ComputedRect, Hidden, HitTarget, InteractionFeedback};
use alloc::vec::Vec;

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

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
    crate::__macro_support::flush_entity_events(world).unwrap();
    decision == crate::input::event::BubbleControl::Prevent
}

fn game_snapshot(model: &FoldModel) -> ([u8; SAVE_LEN], FoldMessage, ExpeditionModal) {
    let mut saved = [0; SAVE_LEN];
    model.encode_into(&mut saved).unwrap();
    (saved, model.message(), model.modal())
}

fn solve_level(game: &<FoldModel as Model>::Handle) {
    let len = with_game(game, |model| model.level().solution_len());
    for step in 0..len {
        let direction = with_game(game, |model| model.level().solution(step).unwrap());
        game.move_direction(direction);
    }
}

fn blocked_direction(game: &<FoldModel as Model>::Handle) -> Direction4 {
    with_game(game, |model| {
        Direction4::ALL
            .into_iter()
            .find(|direction| {
                let mut probe = *model;
                !probe.move_direction(*direction).contains(ChangeSet::VISUAL)
            })
            .expect("initial Fold state has an unsupported direction")
    })
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

    for (positions, direction) in [
        ([PhysicalKey::KeyW, PhysicalKey::ArrowUp], Direction4::Up),
        (
            [PhysicalKey::KeyD, PhysicalKey::ArrowRight],
            Direction4::Right,
        ),
        (
            [PhysicalKey::KeyS, PhysicalKey::ArrowDown],
            Direction4::Down,
        ),
        (
            [PhysicalKey::KeyA, PhysicalKey::ArrowLeft],
            Direction4::Left,
        ),
    ] {
        for position in positions {
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
    observer.move_direction(direction);
    send_key(&mut app.world, root, PhysicalKey::KeyR);
    assert_eq!(observer.steps(), 0);

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
    app.world.resource_mut::<UnhandledKey>().unwrap().0 = None;
    send_key(&mut app.world, root, PhysicalKey::Escape);
    assert_eq!(panel.panel(), ExpeditionPanel::None);
    assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    send_key(&mut app.world, root, PhysicalKey::KeyQ);
    assert_eq!(with_game(&observer, game_snapshot), before);
    assert_eq!(
        app.world.resource::<UnhandledKey>().unwrap().0,
        Some(PhysicalKey::KeyQ)
    );

    observer.restart();
    solve_level(&observer);
    assert_ne!(observer.modal(), ExpeditionModal::None);
    let completed = with_game(&observer, game_snapshot);
    app.world.resource_mut::<UnhandledKey>().unwrap().0 = None;
    for position in [PhysicalKey::KeyW, PhysicalKey::KeyZ, PhysicalKey::KeyH] {
        send_key(&mut app.world, root, position);
        assert_eq!(with_game(&observer, game_snapshot), completed);
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    send_key(&mut app.world, root, PhysicalKey::Enter);
    assert_eq!(observer.level_index(), 1);
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
    solve_level(&game);
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

    let blocked = blocked_direction(&game);
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
    assert!(status.resolve(world.resource()).contains("sink"));

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
    assert!(status.resolve(world.resource()).starts_with("Hint"));
}

fn text_of(world: &World, id: &'static str) -> alloc::string::String {
    let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
    text.resolve(world.resource()).into_owned()
}

fn assert_texts_fit(world: &World) {
    for fit in crate::gallery::demos::text_fits(world) {
        assert!(
            !fit.overflows(4),
            "{:?} needs {:?} in {:?}",
            fit.text,
            fit.lines,
            fit.rect
        );
    }
}

#[test]
fn chapters_are_briefed_and_the_seal_counter_follows_the_hull() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "fold_steps"), "STEP 00 · PAR 05");
    let seals = app.world.find_by_id("fold_seals").unwrap();
    assert!(app.world.has::<Hidden>(seals), "level 1 has no seals");

    for level in 0..12 {
        solve_level(&game);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        assert_eq!(game.progress().stars(level), 3);
        assert_eq!(text_of(&app.world, "fold_next"), "Next →");
        next(&game, &expedition);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        if level % 6 == 5 {
            let chapter = usize::from(game.level_index() / 6);
            assert_eq!(expedition.panel(), ExpeditionPanel::Briefing);
            assert_eq!(
                text_of(&app.world, "fold_briefing_tip_0"),
                CHAPTER_TIPS[chapter][0].en
            );
            expedition.close();
        }
    }

    assert_eq!(game.level_index(), 12);
    flush(&mut app.world);
    assert!(!app.world.has::<Hidden>(seals));
    assert_eq!(text_of(&app.world, "fold_seals"), "◆ 0/1");
    solve_level(&game);
    flush(&mut app.world);
    assert_eq!(text_of(&app.world, "fold_seals"), "◆ 1/1");
}

#[test]
fn chinese_mode_relabels_every_screen_and_still_fits() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    assert_eq!(text_of(&app.world, "fold_title"), "FOLDING ARK");

    app.world
        .resource::<I18n>()
        .unwrap()
        .set_locale(Locale::ZhCn);
    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "fold_title"), "折叠方舟");
    assert_eq!(
        text_of(&app.world, "fold_status"),
        "翻滚船体，让它直立停进终点"
    );
    assert_eq!(text_of(&app.world, "fold_chapter"), "启动船体");
    assert_eq!(text_of(&app.world, "fold_steps"), "步数 00 · 最少 05");

    for panel in [
        ExpeditionPanel::Map,
        ExpeditionPanel::Rules,
        ExpeditionPanel::Briefing,
        ExpeditionPanel::Summary,
    ] {
        match panel {
            ExpeditionPanel::Map => expedition.open(0),
            ExpeditionPanel::Rules => expedition.open_rules(),
            ExpeditionPanel::Briefing => expedition.open_briefing(),
            _ => expedition.open_summary(),
        }
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
    }
    expedition.close();
    solve_level(&game);
    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "fold_result_title"), "方舟已归港");
    assert_eq!(text_of(&app.world, "fold_next"), "下一海域 →");
}

#[test]
fn a_tap_or_a_swipe_rolls_the_hull() {
    let (mut world, (game, _expedition)) = fixture();
    let target = world
        .query::<MapTarget>()
        .iter()
        .map(|(entity, _)| entity)
        .next()
        .unwrap();
    let (left, top, width, height) = panel_box();
    world.insert(target, ComputedRect(Rect::new(left, top, width, height)));
    let direction = with_game(&game, |model| {
        Direction4::ALL
            .into_iter()
            .find(|direction| {
                let mut probe = *model;
                probe.move_direction(*direction).contains(ChangeSet::VISUAL)
            })
            .expect("the hull can roll somewhere from the start")
    });
    let mut expected = with_game(&game, |model| *model);
    expected.move_direction(direction);
    let board = game.board();
    let hull = hull_rect(board.cells, board.len);
    let (dx, dy) = direction.delta();
    let pitch = Fixed::from_int(34);
    let half = Fixed::from_int(2);
    let x = hull.x + hull.w / half + pitch * Fixed::from_int(i32::from(dx));
    let y = hull.y + hull.h / half + pitch * Fixed::from_int(i32::from(dy));
    assert!(trigger(&mut world, &GestureEvent::Tap { x, y, target }));
    assert_eq!(with_game(&game, game_snapshot), game_snapshot(&expected));

    // A tap on the hull itself does nothing.
    let board = game.board();
    let hull = hull_rect(board.cells, board.len);
    let before = with_game(&game, game_snapshot);
    let (x, y) = (hull.x + Fixed::from_int(3), hull.y + Fixed::from_int(3));
    assert!(trigger(&mut world, &GestureEvent::Tap { x, y, target }));
    assert_eq!(with_game(&game, game_snapshot), before);

    let surface = world.find_by_id("fold_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    game.restart();
    let mut expected = with_game(&game, |model| *model);
    expected.move_direction(direction);
    let (dx, dy) = direction.delta();
    let start = (Fixed::from_int(240), Fixed::from_int(160));
    let end = (
        start.0 + Fixed::from_int(40 * i32::from(dx)),
        start.1 + Fixed::from_int(40 * i32::from(dy)),
    );
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: start.0,
            y: start.1,
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: end.0,
            y: end.1,
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_eq!(with_game(&game, game_snapshot), game_snapshot(&expected));
}

#[test]
fn rolls_travel_refusals_bump_and_a_clear_pops_its_stars() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.tick();
    let surface = app.world.find_by_id("fold_surface").unwrap();
    assert!(expedition.banner(), "a level starts with its name");

    let start = game.board();
    let first = with_game(&game, |model| model.level().solution(0).unwrap());
    game.move_direction(first);
    app.tick();
    assert!(!expedition.banner(), "a roll clears the banner");
    assert_eq!(
        app.world.get::<FoldTrack>(surface).unwrap().from,
        (start.cells, start.len)
    );
    let mut travel = Vec::new();
    for _ in 0..16 {
        app.tick();
        travel.push(app.world.get::<BoardMotion>(surface).unwrap().travel);
    }
    assert!(travel[0] > Fixed::ZERO && travel[0] < Fixed::ONE);
    assert!(travel.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(*travel.last().unwrap(), Fixed::ONE);

    game.restart();
    app.tick();
    game.move_direction(blocked_direction(&game));
    let mut furthest = Fixed::ZERO;
    for _ in 0..40 {
        app.tick();
        furthest = furthest.max(app.world.get::<BoardMotion>(surface).unwrap().bump);
    }
    assert!(
        furthest > Fixed::from_int(2),
        "a refused roll nudges the hull"
    );
    assert_eq!(
        app.world.get::<BoardMotion>(surface).unwrap().bump,
        Fixed::ZERO
    );

    solve_level(&game);
    for _ in 0..60 {
        app.tick();
    }
    let row = app.world.find_by_id("fold_result_stars").unwrap();
    let stars = *app.world.get::<ResultStars>(row).unwrap();
    assert_eq!(stars.earned(), 3);
    assert_eq!(stars.scales(), [Fixed::ONE; 3]);
}
