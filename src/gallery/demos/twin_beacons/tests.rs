use super::geometry::{cell_center, target_box};
use super::shell;
use super::state::{
    StationTarget, TwinHintService, TwinSurface, TwinTrack, next, select_map_level,
};
use crate::core::i18n::{I18n, Locale};
use crate::core::model::{Model, ModelHandle};
use crate::ecs::World;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    BoardMotion, Direction4, ExpeditionHintWorkspace, ExpeditionModal, ExpeditionPanel,
    ExpeditionUiModel, ResultStars,
};
use crate::gallery::play::twin::{TwinMessage, TwinModel, pressed_for};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, ComputedRect, Hidden, HitTarget, InteractionFeedback};
use alloc::format;
use alloc::vec::Vec;

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

fn solve_level(game: &<TwinModel as Model>::Handle) {
    let len = with_game(game, |model| model.level().solution_len());
    for step in 0..len {
        let direction = with_game(game, |model| model.level().solution(step).unwrap());
        game.move_direction(direction);
    }
}

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
    crate::__macro_support::flush_entity_events(world).unwrap();
    decision == crate::input::event::BubbleControl::Prevent
}

#[test]
fn a_tap_on_a_board_moves_its_beacon_one_cell_toward_it() {
    let (mut world, (game, expedition)) = fixture();
    let targets: Vec<(Entity, u8)> = world
        .query::<StationTarget>()
        .iter()
        .map(|(entity, target)| (entity, target.station))
        .collect();
    assert_eq!(targets.len(), 2);
    for (entity, station) in targets {
        let (left, top, width, height) = target_box(usize::from(station));
        world.insert(entity, ComputedRect(Rect::new(left, top, width, height)));
        let station = usize::from(station);
        game.restart();
        let step = with_game(&game, |model| {
            let mode = model.level().mode();
            Direction4::ALL.into_iter().find_map(|direction| {
                let pressed = if station == 0 {
                    direction
                } else {
                    pressed_for(direction, mode)
                };
                let mut probe = *model;
                probe.move_direction(pressed);
                let (from, to) = (model.position(station), probe.position(station));
                (from != to).then_some(to)
            })
        });
        let to = step.expect("each beacon can step somewhere from the start");
        let point = cell_center(station, to);
        let tap = GestureEvent::Tap {
            x: point.x,
            y: point.y,
            target: entity,
        };
        assert!(trigger(&mut world, &tap));
        assert_eq!(game.board().cells[station], to, "station {station}");
        // Panels and results take the board's taps away.
        expedition.open_rules();
        let before = game.board();
        assert!(trigger(&mut world, &tap));
        assert_eq!(game.board(), before);
        expedition.close();
    }

    let surface = world.find_by_id("twin_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    game.restart();
    let mut expected = with_game(&game, |model| *model);
    expected.move_direction(Direction4::Right);
    let at = |x: i32| (Fixed::from_int(x), Fixed::from_int(150));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: at(100).0,
            y: at(100).1,
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: at(160).0,
            y: at(160).1,
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_eq!(with_game(&game, |model| *model), expected);
}

#[test]
fn registered_keyboard_preserves_commands_and_bound_model_instances() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    let observer = game.clone();
    let panel = expedition.clone();
    let independent = app.add_model(TwinModel::default()).finish();
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
            assert_eq!(with_game(&observer, |game| *game), expected);
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
        assert_eq!(observer.message(), TwinMessage::Undone);
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    for _ in 0..2 {
        let position = PhysicalKey::KeyH;
        send_key(&mut app.world, root, position);
        assert_eq!(observer.hints(), 1);
        assert!(matches!(observer.message(), TwinMessage::Hint(_, _)));
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    observer.move_direction(direction);
    send_key(&mut app.world, root, PhysicalKey::KeyR);
    assert_eq!(observer.steps(), 0);
    assert_eq!(observer.hints(), 0);

    panel.open_rules();
    let before = with_game(&observer, |game| *game);
    for position in [
        PhysicalKey::KeyW,
        PhysicalKey::KeyD,
        PhysicalKey::KeyZ,
        PhysicalKey::KeyH,
    ] {
        send_key(&mut app.world, root, position);
        assert_eq!(with_game(&observer, |game| *game), before);
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
    assert_eq!(with_game(&observer, |game| *game), before);
    assert_eq!(
        app.world.resource::<UnhandledKey>().unwrap().0,
        Some(PhysicalKey::KeyQ)
    );

    observer.restart();
    solve_level(&observer);
    assert_ne!(observer.modal(), ExpeditionModal::None);
    let completed = with_game(&observer, |game| *game);
    app.world.resource_mut::<UnhandledKey>().unwrap().0 = None;
    for position in [PhysicalKey::KeyW, PhysicalKey::KeyZ, PhysicalKey::KeyH] {
        send_key(&mut app.world, root, position);
        assert_eq!(with_game(&observer, |game| *game), completed);
        assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);
    }
    send_key(&mut app.world, root, PhysicalKey::Enter);
    assert_eq!(observer.level_index(), 1);
    assert_eq!(observer.modal(), ExpeditionModal::None);
    assert_eq!(with_game(&independent, |game| *game), TwinModel::default());
    assert_eq!(independent_panel.panel(), ExpeditionPanel::Summary);
}

#[test]
fn keyboard_refetches_the_required_hint_service_after_resource_changes() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    let observer = game.clone();
    drop(game);
    drop(expedition);

    let retained = app.world.resource::<TwinHintService>().unwrap().clone();
    app.add_resource(TwinHintService::new());
    send_key(&mut app.world, root, PhysicalKey::KeyH);
    assert_eq!(observer.hints(), 1);
    assert!(matches!(observer.message(), TwinMessage::Hint(_, _)));

    observer.restart();
    let before = with_game(&observer, |game| *game);
    app.world.remove_resource::<TwinHintService>().unwrap();
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        send_key(&mut app.world, root, PhysicalKey::KeyH);
    }));
    assert!(error.is_err());
    assert_eq!(with_game(&observer, |game| *game), before);
    retained.request(&observer);
    assert_eq!(observer.hints(), 1);

    observer.restart();
    app.add_resource(TwinHintService::new());
    for _ in 0..2 {
        let position = PhysicalKey::KeyH;
        send_key(&mut app.world, root, position);
        assert_eq!(observer.hints(), 1);
        assert!(matches!(observer.message(), TwinMessage::Hint(_, _)));
    }
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
    solve_level(&game);
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
    assert!(text.resolve(world.resource()).contains("1/6"));
    assert!(text.has_valid_content());
    assert_eq!(text.last_content_error(), None);
}

#[test]
fn messages_and_hints_update_observed_text_without_invalidating_surface() {
    let (mut world, (game, _expedition)) = fixture();
    let surface = world.find_by_id("twin_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let visual_revision = game.visual_revision();
    let hints = game.hints();

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
    assert_eq!(game.hints(), hints);
    let status = world
        .get::<Text>(world.find_by_id("twin_status").unwrap())
        .unwrap();
    assert!(status.resolve(world.resource()).contains("blocked"));

    let changes = game.request_hint(&mut ExpeditionHintWorkspace::new());
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::PERSISTENCE));
    assert!(!changes.contains(ChangeSet::VISUAL));
    flush(&mut world);

    assert_eq!(game.visual_revision(), visual_revision);
    assert_eq!(game.hints(), hints + 1);
    assert!(!world.has::<VisualDirty>(surface));
    let status = world
        .get::<Text>(world.find_by_id("twin_status").unwrap())
        .unwrap();
    assert!(status.resolve(world.resource()).starts_with("Hint"));
    let steps = world
        .get::<Text>(world.find_by_id("twin_steps").unwrap())
        .unwrap();
    assert!(steps.resolve(world.resource()).contains("HINT 1"));
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
fn the_route_rows_show_where_b_goes_and_chapter_two_is_briefed() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "twin_route_b"), "B  ↑  →  ↓  ←");
    assert_eq!(text_of(&app.world, "twin_steps"), "STEP 00 · PAR 05");

    for level in 0..6 {
        solve_level(&game);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
        assert_eq!(
            text_of(&app.world, "twin_result_title"),
            if level == 5 {
                "CHAPTER CLEAR"
            } else {
                "STATION CLEAR"
            }
        );
        let par = with_game(&game, |model| model.level().par());
        assert_eq!(
            text_of(&app.world, "twin_result_stats"),
            format!("{par} moves · par {par}")
        );
        assert_eq!(text_of(&app.world, "twin_next"), "Next →");
        next(&game, &expedition);
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world);
    }

    assert_eq!(game.level_index(), 6);
    assert_eq!(expedition.panel(), ExpeditionPanel::Briefing);
    assert_eq!(
        text_of(&app.world, "twin_briefing_title"),
        "CHAPTER 2 · MIRROR ROUTE"
    );
    assert_eq!(
        text_of(&app.world, "twin_briefing_tip_0"),
        "← sends B right, → sends B left."
    );
    assert_eq!(text_of(&app.world, "twin_route_b"), "B  ↑  ←  ↓  →");
}

#[test]
fn chinese_mode_relabels_every_screen_and_still_fits() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    assert_eq!(text_of(&app.world, "twin_title"), "TWIN BEACONS");
    assert_eq!(
        text_of(&app.world, "twin_status"),
        "Park A and B on their rings at once"
    );

    app.world
        .resource::<I18n>()
        .unwrap()
        .set_locale(Locale::ZhCn);
    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "twin_title"), "双子信标");
    assert_eq!(
        text_of(&app.world, "twin_status"),
        "让 A 和 B 同时停在各自的圆环上"
    );
    assert_eq!(text_of(&app.world, "twin_chapter"), "初次联动");
    assert_eq!(text_of(&app.world, "twin_steps"), "步数 00 · 最少 05");

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
    expedition.open(0);
    flush(&mut app.world);
    assert!(text_of(&app.world, "twin_map_summary").starts_with("已完成 00 / 36"));
    assert_eq!(
        text_of(&app.world, "twin_briefing_title"),
        "第 1 章 · 初次联动"
    );
    expedition.close();
    solve_level(&game);
    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world);
    assert_eq!(text_of(&app.world, "twin_result_title"), "本站完成");
    assert_eq!(text_of(&app.world, "twin_next"), "下一站 →");
}

#[test]
fn moves_travel_refusals_bump_and_a_clear_pops_its_stars() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let (game, expedition) = shell::setup_test_app(&mut app, root);
    app.set_root(root);
    app.tick();
    let surface = app.world.find_by_id("twin_surface").unwrap();
    assert!(expedition.banner(), "a level starts with its name");

    let start = game.board().cells;
    let first = with_game(&game, |model| model.level().solution(0).unwrap());
    game.move_direction(first);
    app.tick();
    assert!(!expedition.banner(), "a move clears the banner");
    assert_eq!(app.world.get::<TwinTrack>(surface).unwrap().from, start);
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
    let blocked = with_game(&game, |model| {
        Direction4::ALL
            .into_iter()
            .find(|direction| {
                let mut probe = *model;
                !probe.move_direction(*direction).contains(ChangeSet::VISUAL)
            })
            .expect("initial Twin state has a blocked direction")
    });
    game.move_direction(blocked);
    let mut furthest = Fixed::ZERO;
    for _ in 0..40 {
        app.tick();
        furthest = furthest.max(app.world.get::<BoardMotion>(surface).unwrap().bump);
    }
    assert!(
        furthest > Fixed::from_int(2),
        "a refused move nudges the beacons"
    );
    assert_eq!(
        app.world.get::<BoardMotion>(surface).unwrap().bump,
        Fixed::ZERO
    );

    solve_level(&game);
    for _ in 0..60 {
        app.tick();
    }
    let row = app.world.find_by_id("twin_result_stars").unwrap();
    let stars = *app.world.get::<ResultStars>(row).unwrap();
    assert_eq!(stars.earned(), 3);
    assert_eq!(stars.scales(), [Fixed::ONE; 3]);
}
