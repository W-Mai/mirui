use super::data::{LaunchSource, Outcome, Page, Phase, Scenario};
use super::*;
use crate::surface::FramebufferAccess;
use crate::ui::widgets::navigation::transition;

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, model::DisplayStateHandle) {
    fixture_at(640, 480)
}

fn fixture_at(width: u16, height: u16) -> (TestApp, model::DisplayStateHandle) {
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let state = setup_app(&mut app, root);
    state.launcher().close();
    app.world.remove_resource::<crate::ecs::MonoClock>();
    app.add_resource(crate::ecs::DeltaTimeMs(16));
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    (app, state)
}

#[test]
fn all_palettes_define_the_same_paint_roles_and_cycle_without_replacing_models() {
    use super::style::*;
    for index in 0..3 {
        let theme = palette::theme(index);
        for token in [
            BACKGROUND,
            FOREGROUND,
            ACCENT,
            ON_ACCENT,
            PANEL,
            SECONDARY,
            DIVIDER,
            WARNING,
            SKY,
            HILL,
            DEEP,
            CLAY,
            HEADER_TEXT,
            HEADER_DETAIL,
        ] {
            assert_ne!(theme.resolve(token), Color::rgb(255, 0, 255));
        }
    }
    let (mut app, state) = fixture();
    for expected in ["sunset", "night", "forest"] {
        state.launcher().adjust(launcher::MenuAction::CycleTheme);
        app.tick();
        assert_eq!(app.theme().id().as_str(), expected);
    }
}

#[test]
fn character_follows_the_active_choice_and_sessions_own_their_character() {
    use crate::ui::{Hidden, widgets::FollowBounds};
    let (mut app, state) = fixture();
    for _ in 0..3 {
        app.tick();
    }
    let actor = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .find(|(_, marker)| !marker.launcher)
        .unwrap()
        .0;
    assert!(app.world.get::<FollowBounds>(actor).is_some());
    assert!(app.world.get::<Hidden>(actor).is_none());
    state.activate(0);
    finish_transition(&mut app);
    assert!(app.world.is_alive(actor));
    assert!(app.world.get::<Hidden>(actor).is_none());
    state.activate(0);
    for _ in 0..3 {
        app.tick();
    }
    assert!(app.world.get::<Hidden>(actor).is_some());
    assert!(
        app.world
            .query::<pixel::SessionSprite>()
            .iter()
            .next()
            .is_some()
    );
}

#[test]
fn initial_launcher_binds_focus_and_character_before_the_first_tick() {
    use crate::ui::{ComputedRect, Hidden, widgets::FollowBounds};
    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let state = setup_app(&mut app, root);
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    let actor = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .find(|(_, marker)| marker.launcher)
        .unwrap()
        .0;
    let target = app.world.get::<FollowBounds>(actor).unwrap().target;
    assert!(app.world.get::<ComputedRect>(target).is_some());
    assert!(app.world.get::<Hidden>(actor).is_none());
    let frame = app
        .world
        .query::<library::LibraryFocus>()
        .iter()
        .find(|(_, marker)| marker.screen == state.launcher().screen())
        .unwrap()
        .0;
    assert_eq!(app.world.get::<FollowBounds>(frame).unwrap().target, target);
}

#[test]
fn first_tick_keeps_entity_bindings_before_layout_geometry_exists() {
    use crate::ui::{Hidden, widgets::FollowBounds};
    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.tick();
    for (entity, marker) in app.world.query::<actor::ActorFrame>().iter() {
        if marker.launcher {
            assert!(app.world.get::<FollowBounds>(entity).is_some());
            assert!(app.world.get::<Hidden>(entity).is_none());
        }
    }
}

#[test]
fn discovery_loading_motion_uses_the_visible_mock_state() {
    let (mut app, state) = fixture();
    while state.scenario() != Scenario::EmptyRooms {
        state.next_scenario();
    }
    state.open_network();
    app.tick();
    assert!(state.presentation().rooms_empty);
    assert!(state.search_motion().wave.is_some());
    state.expand_discovery(true);
    app.tick();
    assert!(state.search_motion().wave.is_none());
}

#[test]
fn action_overlay_keeps_preview_and_all_menu_items_inside_the_panel() {
    use crate::ui::ComputedRect;
    for (width, height) in [(360, 480), (640, 480), (900, 600)] {
        let (mut app, state) = fixture_at(width, height);
        let library = state.launcher();
        library.resume();
        library.choose_game(0, 0);
        app.render_dirty().unwrap();
        let panel = app.world.find_by_id("nearby_actions_panel").unwrap();
        let panel = app.world.get::<ComputedRect>(panel).unwrap().0;
        assert!(panel.w > Fixed::from_int(i32::from(width) * 4 / 5));
        assert!(panel.h > Fixed::from_int(i32::from(height) * 3 / 5));
        let preview = app.world.find_by_id("nearby_actions_preview").unwrap();
        let preview = app.world.get::<ComputedRect>(preview).unwrap().0;
        let tiles: alloc::vec::Vec<_> = app
            .world
            .query::<actions::ActionAnchor>()
            .iter()
            .filter_map(|(entity, anchor)| {
                Some((anchor.0, app.world.get::<ComputedRect>(entity)?.0))
            })
            .collect();
        assert_eq!(tiles.len(), 4);
        for (index, rect) in &tiles {
            assert!(rect.x >= preview.x + preview.w, "{index}: {rect:?}");
            assert!(rect.w > Fixed::ZERO && rect.h > Fixed::ZERO);
            assert!(rect.x + rect.w <= panel.x + panel.w);
            assert!(rect.y >= panel.y && rect.y + rect.h <= panel.y + panel.h);
        }
        let partial = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            partial,
            app.backend.framebuffer().read_pixels().unwrap().bytes()
        );
        library.dismiss_menu();
        app.render_dirty().unwrap();
        let closed = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        assert_ne!(closed, partial);
        app.render().unwrap();
        assert_eq!(
            closed,
            app.backend.framebuffer().read_pixels().unwrap().bytes()
        );
    }
}

#[test]
fn launcher_hold_launches_once_and_cancels_across_navigation() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    launcher_view::begin_play(&state);
    for _ in 0..10 {
        app.tick();
    }
    assert!(library.hold_progress() > Fixed::ZERO);
    launcher_view::finish_play(&state);
    assert_eq!(library.notice(), "单机游玩 · LOCAL DEMO");
    assert!(!library.holding());
    launcher_view::finish_play(&state);
    assert_eq!(library.notice(), "单机游玩 · LOCAL DEMO");
    library.back();
    launcher_view::begin_play(&state);
    for _ in 0..50 {
        app.tick();
    }
    assert!(!library.active());
    let key = state.page_key();
    launcher_view::finish_play(&state);
    assert_eq!(state.page_key(), key);
    assert_eq!(library.hold_progress(), Fixed::ZERO);
    assert_eq!(state.snapshot().source, LaunchSource::Game);
    library.resume();
    launcher_view::begin_play(&state);
    library.show_overlay(launcher::Overlay::System);
    assert!(!library.holding());
    launcher_view::finish_play(&state);
    assert_eq!(library.overlay(), Some(launcher::Overlay::System));
}

#[test]
fn hold_feedback_and_stale_frames_cannot_retrigger_a_consumed_press() {
    let mut library = launcher::Launcher::default();
    library.begin_hold(0);
    library.advance_hold(129);
    assert_eq!(library.pending_play(), None);
    assert_eq!(library.hold_progress, Fixed::ZERO);
    library.advance_hold(1);
    assert!(library.hold_progress > Fixed::ZERO);
    library.advance_hold(251);
    library.begin_hold(0);
    assert!(!library.holding());
    library.release_hold();
    assert_eq!(library.pending_play(), None);
    library.begin_hold(0);
    for _ in 0..6 {
        library.advance_hold(100);
        assert_eq!(library.pending_play(), None);
    }
    library.advance_hold(100);
    assert_eq!(library.pending_play(), Some((0, true)));
    library.acknowledge_play();
    library.begin_hold(0);
    library.advance_hold(200);
    library.release_hold();
    assert_eq!(library.pending_play(), None);
    library.begin_hold(1);
    library.advance_hold(100);
    library.release_hold();
    assert_eq!(library.pending_play(), Some((1, false)));
}

#[test]
fn native_menu_hierarchy_reflows_without_replacing_the_browser() {
    use crate::ui::ComputedRect;
    use launcher::{MenuAction, Overlay};
    for (width, height) in [(320, 240), (640, 480)] {
        let (mut app, state) = fixture_at(width, height);
        let library = state.launcher();
        library.resume();
        library.select(4);
        for overlay in [
            Overlay::System,
            Overlay::Controls,
            Overlay::Tools,
            Overlay::Exit,
            Overlay::Power,
            Overlay::Network,
        ] {
            library.show_overlay(overlay);
            app.render_dirty().unwrap();
            let host = app.world.find_by_id("nearby_actions_panel").unwrap();
            let panel = app.world.get::<ComputedRect>(host).unwrap().0;
            let tiles: alloc::vec::Vec<_> = app
                .world
                .query::<actions::ActionAnchor>()
                .iter()
                .filter_map(|(entity, _)| {
                    let mut ancestor = entity;
                    loop {
                        if crate::ui::branch::is_effectively_hidden(&app.world, ancestor) {
                            return None;
                        }
                        let Some(parent) = app.world.get::<crate::ui::Parent>(ancestor) else {
                            break;
                        };
                        ancestor = parent.0;
                    }
                    app.world.get::<ComputedRect>(entity).map(|r| r.0)
                })
                .collect();
            assert_eq!(tiles.len(), library.item_count());
            for rect in tiles {
                assert!(rect.w > Fixed::ZERO && rect.h > Fixed::ZERO);
                assert!(rect.x >= panel.x && rect.x + rect.w <= panel.x + panel.w);
                assert!(rect.y >= panel.y && rect.y + rect.h <= panel.y + panel.h);
            }
            assert_eq!(library.selected(), 4);
        }
        library.show_overlay(Overlay::Controls);
        library.adjust(MenuAction::VolumeUp);
        app.render_dirty().unwrap();
        assert_eq!(library.item(0).unwrap().value, "70%");
        library.show_overlay(Overlay::Game);
        library.choose_game(2, 2);
        library.select_action(1);
        launcher_view::confirm(&state);
        assert!(library.active());
        assert_eq!(library.notice(), "此游戏暂未接入联机");
    }
}

#[test]
fn launcher_filtered_selection_and_all_tools_are_reachable() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    library.choose_platform(1);
    launcher_view::move_selection(&state, 1);
    assert_eq!(library.selected(), 7);
    launcher_view::confirm(&state);
    assert_eq!(library.game_key(), Some(7));
    library.dismiss_menu();
    for overlay in [
        launcher::Overlay::System,
        launcher::Overlay::Controls,
        launcher::Overlay::Tools,
        launcher::Overlay::Exit,
        launcher::Overlay::Power,
        launcher::Overlay::Network,
    ] {
        library.show_overlay(overlay);
        let count = library.item_count();
        for index in 0..count {
            library.show_overlay(overlay);
            library.select_action(index);
            launcher_view::confirm(&state);
            assert!(library.volume() <= 100 && library.brightness() <= 100);
            if !library.active() {
                library.resume();
            }
            app.tick();
        }
    }
    library.dismiss_menu();
    assert_eq!(library.screen(), launcher::Screen::Games);
}

#[test]
fn launcher_deck_retains_shelf_members_and_independent_selection_on_reversal() {
    use super::launcher::Screen;
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    library.select(4);
    for _ in 0..3 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let shelf = app.world.find_by_id("nearby_shelf").unwrap();
    let children = app
        .world
        .get::<crate::ui::Children>(shelf)
        .unwrap()
        .0
        .clone();
    library.open(Screen::Platforms);
    for _ in 0..12 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let middle = library.deck().expansion();
    assert!(middle > Fixed::ZERO && middle < Fixed::ONE);
    library.select(2);
    library.open(Screen::Favorites);
    assert_eq!(library.selected(), 4);
    for _ in 0..50 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert_eq!(library.deck().expansion(), Fixed::ZERO);
    assert_eq!(app.world.find_by_id("nearby_shelf"), Some(shelf));
    assert_eq!(
        app.world.get::<crate::ui::Children>(shelf).unwrap().0,
        children
    );
    library.open(Screen::Platforms);
    assert_eq!(library.selected(), 2);
}

#[test]
fn room_panel_retains_browser_entities_and_returns_to_the_same_selection() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    library.select(4);
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let shelf = app.world.find_by_id("nearby_shelf").unwrap();
    state.launch_game("双人冒险".into(), true);
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert!(state.return_to_launcher());
    assert_eq!(library.deck().panel(), Fixed::ONE);
    assert_eq!(app.world.find_by_id("nearby_shelf"), Some(shelf));
    let panel = app.world.find_by_id("nearby_room_panel").unwrap();
    let rect = app.world.get::<crate::ui::ComputedRect>(panel).unwrap().0;
    assert!(rect.w > Fixed::from_int(500) && rect.h > Fixed::from_int(350));
    let actor = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .find(|(_, frame)| frame.launcher)
        .unwrap()
        .0;
    let target = app
        .world
        .get::<crate::ui::widgets::FollowBounds>(actor)
        .unwrap()
        .target;
    let body = app
        .world
        .query::<input::SelectionFrame>()
        .iter()
        .find(|(_, frame)| frame.room && frame.page == state.page_key())
        .unwrap()
        .1
        .body;
    assert!(input::contains(body, target, |e| app
        .world
        .get::<crate::ui::Parent>(e)
        .map(|p| p.0)));
    state.back_page();
    assert!(library.active());
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert_eq!(library.deck().panel(), Fixed::ZERO);
    assert_eq!(library.selected(), 4);
    assert_eq!(app.world.find_by_id("nearby_shelf"), Some(shelf));
}

#[test]
fn modal_background_uses_output_history_until_the_overlay_closes() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    app.render().unwrap();
    library.show_overlay(launcher::Overlay::System);
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let pixel =
        |app: &mut TestApp| app.backend.framebuffer().read_pixels().unwrap().bytes()[..4].to_vec();
    let frozen = pixel(&mut app);
    library.adjust(launcher::MenuAction::CycleTheme);
    for _ in 0..5 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert_eq!(app.theme().id().as_str(), "sunset");
    assert_eq!(pixel(&mut app), frozen);
    library.dismiss_menu();
    for _ in 0..5 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert_ne!(pixel(&mut app), frozen);
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

#[test]
fn modal_panels_remain_drawable_without_offscreen_capacity() {
    let (mut app, state) = fixture();
    state.launcher().resume();
    app.with_offscreen_pool_budget(0);
    state.launcher().show_overlay(launcher::Overlay::System);
    app.render().unwrap();
    assert!(app.world.find_by_id("nearby_actions_panel").is_some());
    let tile = app
        .world
        .query::<actions::ActionAnchor>()
        .iter()
        .find(|(_, marker)| marker.0 == 0)
        .unwrap()
        .0;
    let rect = app.world.get::<crate::ui::ComputedRect>(tile).unwrap().0;
    let expected = app.theme().resolve(style::ACCENT);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(rect.x.to_int() + 4, rect.y.to_int() + 4),
        expected
    );
}

#[test]
fn launcher_pages_and_actions_reuse_the_application_and_keep_game_identity() {
    use super::launcher::Screen;
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    app.render_dirty().unwrap();
    let identity = library.games().entries().next().unwrap().0;
    for screen in [Screen::Platforms, Screen::Search, Screen::Favorites] {
        library.open(screen);
        for _ in 0..90 {
            app.tick();
        }
        app.render_dirty().unwrap();
        assert_eq!(library.screen(), screen);
        assert_eq!(library.games().entries().next().unwrap().0, identity);
    }
    library.choose_game(0, 0);
    app.render_dirty().unwrap();
    library.toggle_favorite();
    assert!(!library.games().get_by_id(identity).unwrap().favorite());
    app.render_dirty().unwrap();
    library.toggle_favorite();
    library.dismiss_menu();
    library.open(Screen::Platforms);
    library.choose_platform(2);
    library.choose_game(2, 2);
    library.dismiss_menu();
    assert_eq!(library.screen(), Screen::Games);
    assert_eq!(library.platform(), 2);
    state.launch_game("方块世界".into(), false);
    app.render_dirty().unwrap();
    assert_eq!(state.snapshot().page, Page::Unavailable);
    library.resume();
    app.render_dirty().unwrap();
    assert_eq!(library.games().entries().next().unwrap().0, identity);
}

#[test]
fn launcher_search_and_tools_are_local_and_reversible() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    library.open(launcher::Screen::Search);
    library.type_key("X");
    assert_eq!(library.query(), "X");
    library.type_key("退格");
    assert!(library.query().is_empty());
    library.set_query("冒险".into());
    assert_eq!(
        library
            .games()
            .iter()
            .filter(|game| game.matches("adventure"))
            .count(),
        1
    );
    app.render_dirty().unwrap();
    library.show_overlay(launcher::Overlay::System);
    library.show_overlay(launcher::Overlay::Controls);
    library.show_overlay(launcher::Overlay::Tools);
    library.network();
    library.back();
    assert_eq!(library.overlay(), Some(launcher::Overlay::Controls));
    library.back();
    assert_eq!(library.overlay(), Some(launcher::Overlay::System));
    assert!(library.connected());
    state.open_network();
    assert!(!library.active());
    assert_eq!(state.snapshot().source, LaunchSource::Menu);
    assert_eq!(state.snapshot().page, Page::Lobby);
}

#[test]
fn discovery_panes_share_the_mounted_page_and_join_through_candidates() {
    let (mut app, state) = fixture();
    state.launcher().resume();
    state.open_network();
    finish_transition(&mut app);
    let deck = app.world.find_by_id("discovery_deck").unwrap();
    let key = state.page_key();
    state.expand_discovery(true);
    assert_eq!(state.page_key(), key);
    finish_transition(&mut app);
    for _ in 0..24 {
        app.tick();
    }
    assert_eq!(app.world.find_by_id("discovery_deck"), Some(deck));
    assert_eq!(state.motion().deck, Fixed::ONE);
    state.back_page();
    finish_transition(&mut app);
    for _ in 0..24 {
        app.tick();
    }
    assert_eq!(state.motion().deck, Fixed::ZERO);
    assert_eq!(app.world.find_by_id("discovery_deck"), Some(deck));
    state.choose_room(0);
    assert_eq!(state.snapshot().phase, Phase::ResolvingGame);
    state.advance();
    assert_eq!(state.snapshot().page, Page::Candidates);
    assert_eq!(state.snapshot().phase, Phase::SelectingGame);
    state.activate(0);
    assert_eq!(state.snapshot().page, Page::Joined);
    assert_eq!(state.snapshot().phase, Phase::Connecting);
    state.back_page();
    state.advance();
    assert_eq!(state.snapshot().page, Page::Lobby);
    finish_transition(&mut app);
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

#[test]
fn discovery_focus_and_characters_stay_in_the_active_pane_at_native_sizes() {
    use crate::ui::{Hidden, widgets::FollowBounds};
    for (width, height) in [(320, 240), (640, 480), (900, 600)] {
        let (mut app, state) = fixture_at(width, height);
        state.open_network();
        for expanded in [false, true, false] {
            state.expand_discovery(expanded);
            finish_transition(&mut app);
            let body = app.world.find_by_id("discovery_deck").unwrap();
            let frames: alloc::vec::Vec<_> = app
                .world
                .query::<input::SelectionFrame>()
                .iter()
                .filter(|(e, frame)| {
                    frame.page == state.page_key()
                        && frame.room
                        && app.world.get::<Hidden>(*e).is_none()
                })
                .map(|(e, _)| e)
                .collect();
            assert_eq!(frames.len(), 1);
            let target = app.world.get::<FollowBounds>(frames[0]).unwrap().target;
            let marker = app.world.get::<input::SelectionTarget>(target).unwrap();
            assert!(marker.active(&state));
            assert!(input::contains(body, target, |e| app
                .world
                .get::<crate::ui::Parent>(e)
                .map(|p| p.0)));
            assert!(state.deck_height() > Fixed::ZERO);
        }
    }
}

#[test]
fn catalog_windows_keyboard_and_game_selection_share_the_native_routes() {
    use launcher::Screen;
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    library.open(Screen::Search);
    assert!(library.keyboard());
    assert_eq!(library.selected(), 100);
    library.select(100);
    launcher_view::confirm(&state);
    assert_eq!(library.query(), "Q");
    library.type_key("退格");
    library.type_key("空格");
    library.type_key("清空");
    assert!(library.query().is_empty());
    library.set_query("racing".into());
    assert!(!library.keyboard());
    assert_eq!(library.selected(), 1);
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    launcher_view::confirm(&state);
    assert_eq!(library.game_key(), Some(1));
    library.dismiss_menu();
    library.open(Screen::Platforms);
    for selection in 0..6 {
        library.select(selection);
        app.tick();
        app.render_dirty().unwrap();
        let target =
            app.world
                .query::<library::LibraryTarget>()
                .iter()
                .find_map(|(entity, row)| {
                    (row.screen == Screen::Platforms && row.key == selection).then_some(entity)
                });
        assert!(target.is_some());
    }
    library.open(Screen::Search);
    library.set_query("not-a-game".into());
    for _ in 0..40 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

#[test]
fn physical_keyboard_controls_the_active_launcher_instead_of_the_hidden_connection_page() {
    use crate::input::event::input::PhysicalKey;
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    app.render_dirty().unwrap();
    let root = app.root;
    crate::input::event::dispatch_input(
        &mut app.world,
        root,
        &crate::test_support::physical_key(PhysicalKey::KeyA).into(),
        1,
        640,
        480,
    );
    assert!(library.menu());
    assert_eq!(state.snapshot().page, Page::Mode);
    crate::input::event::dispatch_input(
        &mut app.world,
        root,
        &crate::test_support::physical_key(PhysicalKey::KeyB).into(),
        2,
        640,
        480,
    );
    assert!(!library.menu());
    assert_eq!(state.snapshot().page, Page::Mode);
}

#[test]
fn launcher_rapid_route_changes_and_partial_repaint_match_full_repaint() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    library.resume();
    for screen in [
        launcher::Screen::Platforms,
        launcher::Screen::Search,
        launcher::Screen::Favorites,
    ] {
        library.open(screen);
        for _ in 0..3 {
            app.tick();
        }
    }
    for _ in 0..120 {
        app.tick();
    }
    library.choose_game(0, 0);
    app.render_dirty().unwrap();
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
    library.dismiss_menu();
    library.open(launcher::Screen::Platforms);
    library.choose_platform(3);
    for _ in 0..120 {
        app.tick();
    }
    library.back();
    for _ in 0..120 {
        app.tick();
    }
    assert_eq!(library.screen(), launcher::Screen::Platforms);
}

#[test]
fn resizing_keeps_the_page_and_matches_direct_layout_at_the_same_size() {
    use crate::surface::framebuf::FramebufSurface;
    use crate::ui::{ComputedRect, widgets::Text};

    assert_eq!(DEMO_SIZE.fixed_size(), None);
    let (mut app, state) = fixture_at(640, 480);
    for (width, height) in [(640, 480), (360, 640), (900, 480)] {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        app.backend = FramebufSurface::new(width, height, flush);
        app.render_dirty().unwrap();
        app.render_dirty().unwrap();
        assert_eq!(state.snapshot().page, Page::Mode);
        let (direct, _) = fixture_at(width, height);
        let geometry = |app: &TestApp| {
            app.world
                .query::<Text>()
                .iter()
                .filter_map(|(entity, text)| {
                    Some((
                        text.resolve(app.world.resource()).to_string(),
                        app.world.get::<ComputedRect>(entity)?.0,
                    ))
                })
                .collect::<alloc::vec::Vec<_>>()
        };
        assert_eq!(geometry(&app), geometry(&direct), "{width} x {height}");
        for (_, text) in &geometry(&app) {
            assert!(
                text.w >= Fixed::from_int(12),
                "{width} x {height}: {text:?}"
            );
            assert!(
                text.h >= Fixed::from_int(12),
                "{width} x {height}: {text:?}"
            );
        }
    }
}

fn finish_transition(app: &mut TestApp) {
    let host = app.world.find_by_id("nearby_pages").unwrap();
    for _ in 0..240 {
        app.tick();
        if transition::running(&app.world, host).is_none() {
            return;
        }
    }
    panic!("Nearby page transition did not settle");
}

fn frames(app: &TestApp) -> alloc::vec::Vec<Entity> {
    app.world
        .query::<input::SelectionFrame>()
        .iter()
        .map(|(entity, _)| entity)
        .collect()
}

#[test]
fn page_intent_and_direction_are_published_together_and_reverse_without_jumping() {
    use crate::ui::widgets::{
        WidgetTransform,
        navigation::{PageDirection, PageTransition},
    };
    let (mut app, state) = fixture();
    let host = app.world.find_by_id("nearby_pages").unwrap();
    let x = |app: &TestApp, entity| {
        app.world
            .get::<WidgetTransform>(entity)
            .unwrap()
            .0
            .apply_point(crate::types::Point::ZERO)
            .x
    };
    state.activate(0);
    app.render_dirty().unwrap();
    let pair = transition::running(&app.world, host).unwrap();
    assert!(x(&app, pair.to) > Fixed::ZERO);
    app.tick();
    app.tick();
    let position = x(&app, pair.to);
    state.back_page();
    app.render_dirty().unwrap();
    assert_eq!(x(&app, pair.to), position);
    assert_eq!(
        transition::running(&app.world, host).unwrap().destination(),
        pair.from
    );
    state.activate(1);
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    assert_eq!(state.snapshot().page, Page::Games);
    state.back_page();
    app.render_dirty().unwrap();
    let returning = transition::running(&app.world, host).unwrap();
    assert!(x(&app, returning.to) < Fixed::ZERO);
    finish_transition(&mut app);
    assert_eq!(state.snapshot().page, Page::Mode);
    state.activate(0);
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    state.activate(0);
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    state.back_page();
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    state.advance();
    app.render_dirty().unwrap();
    let returning = transition::running(&app.world, host).unwrap();
    assert!(x(&app, returning.to) < Fixed::ZERO);
    assert!(
        state.transition()
            == PageTransition::slide(crate::anim::SNAPPY).with_direction(PageDirection::Backward)
    );
    finish_transition(&mut app);
    assert_eq!(state.snapshot().page, Page::Choice);
}

#[test]
fn page_transitions_animate_but_session_progress_retains_the_mounted_page() {
    let (mut app, state) = fixture();
    let initial_frame = frames(&app)[0];
    assert!(
        app.world
            .get::<crate::ui::widgets::FollowBounds>(initial_frame)
            .is_some()
    );
    state.activate(0);
    app.render_dirty().unwrap();
    let host = app.world.find_by_id("nearby_pages").unwrap();
    assert!(transition::running(&app.world, host).is_some());
    assert_eq!(frames(&app).len(), 2);
    for _ in 0..8 {
        app.tick();
        let pixels = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            pixels,
            app.backend.framebuffer().read_pixels().unwrap().bytes()
        );
    }
    finish_transition(&mut app);
    assert!(!app.world.is_alive(initial_frame));
    state.activate(0);
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    let mounted = frames(&app);
    let route = state.page_key();
    for _ in 0..3 {
        state.advance();
        app.render_dirty().unwrap();
    }
    state.confirm();
    state.advance();
    app.render_dirty().unwrap();
    assert_eq!(state.snapshot().phase, Phase::NetworkConnected);
    assert_eq!(state.page_key(), route);
    assert_eq!(frames(&app), mounted);
    assert!(transition::running(&app.world, host).is_none());
    assert!(
        app.world
            .query::<crate::ui::widgets::Text>()
            .iter()
            .any(|(_, text)| text.resolve(app.world.resource()) == "192.168.4.1")
    );
    state.confirm();
    app.render_dirty().unwrap();
    assert!(state.launcher().active());
    assert!(state.launcher().connected());
    assert_eq!(state.snapshot().outcome, Outcome::KeepNetwork);
    assert!(!state.return_to_launcher());
}

#[test]
fn direct_game_entry_and_current_game_shortcut_use_the_same_session_flow() {
    let (mut app, state) = fixture();
    state.launcher().resume();
    state.switch_entry();
    assert!(!state.launcher().active());
    assert!(!state.return_to_launcher());
    assert_eq!(state.snapshot().page, Page::Choice);
    state.back_page();
    assert!(state.launcher().active());
    assert_eq!(state.snapshot().outcome, Outcome::SinglePlayer);
    state.open_network();
    state.expand_discovery(true);
    assert!(state.current_game());
    state.choose_connection(2);
    assert_eq!(state.snapshot().page, Page::Host);
    assert_eq!(state.snapshot().phase, Phase::Creating);
    assert_eq!(state.snapshot().source, LaunchSource::Game);
    for _ in 0..3 {
        state.advance();
    }
    state.confirm();
    state.confirm();
    state.advance();
    app.render_dirty().unwrap();
    assert!(state.launcher().active());
    assert_eq!(state.snapshot().outcome, Outcome::Multiplayer);
}

#[test]
fn connection_catalog_edits_the_same_favorites_without_replacing_entries() {
    let (mut app, state) = fixture();
    let library = state.launcher();
    let identities: alloc::vec::Vec<_> = library.games().entries().map(|item| item.0).collect();
    state.activate(1);
    finish_transition(&mut app);
    state.select(1);
    assert!(state.selected_platform() == consoles::Platform::from_id(launcher::PLATFORMS[1].1));
    assert!(state.entry_favorite(1));
    state.toggle_game_favorite();
    assert!(!state.entry_favorite(1));
    assert_eq!(
        library
            .games()
            .entries()
            .map(|item| item.0)
            .collect::<alloc::vec::Vec<_>>(),
        identities
    );
    app.render_dirty().unwrap();
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

#[test]
fn discovery_refresh_and_rapid_return_do_not_replace_the_wrong_page() {
    let (mut app, state) = fixture();
    state.switch_entry();
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    state.activate(1);
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    let mounted = frames(&app);
    state.advance();
    app.render_dirty().unwrap();
    assert_eq!(frames(&app), mounted);
    state.refresh_rooms();
    app.render_dirty().unwrap();
    state.advance();
    app.render_dirty().unwrap();
    assert_eq!(frames(&app), mounted);
    state.activate(0);
    app.render_dirty().unwrap();
    app.tick();
    state.back_page();
    app.render_dirty().unwrap();
    state.advance();
    app.render_dirty().unwrap();
    finish_transition(&mut app);
    assert_eq!(state.snapshot().page, Page::Rooms);
    assert_eq!(frames(&app).len(), 1);
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        pixels,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

#[test]
fn pixel_fonts_cover_the_demo_charset_at_native_sizes() {
    for (size, bytes) in [
        (12, include_bytes!("../assets/nearby-menu.mirx").as_slice()),
        (24, include_bytes!("../assets/nearby-title.mirx").as_slice()),
    ] {
        let font =
            Font::from_mirx("Nearby", size, bytes, &mirx::reader::PayloadLimits::HOST).unwrap();
        for character in include_str!("../assets/nearby-charset.txt")
            .chars()
            .filter(|c| !c.is_control())
        {
            assert!(
                font.map_char(character).is_some(),
                "missing {character} at {size}px"
            );
        }
    }
}

#[test]
fn local_session_transitions_do_not_retain_closed_network_state() {
    let mut model = model::DisplayState::default();
    model.activate(0);
    assert_eq!(model.snapshot().page, Page::Choice);
    model.activate(0);
    for _ in 0..3 {
        model.advance();
    }
    assert_eq!(model.snapshot().phase, Phase::ApprovalPending);
    model.advance();
    assert_eq!(model.snapshot().phase, Phase::ApprovalPending);
    model.confirm();
    assert_eq!(model.snapshot().phase, Phase::NetworkConfirming);
    model.advance();
    assert_eq!(model.snapshot().phase, Phase::NetworkConnected);
    assert!(!model.snapshot().peers.is_empty());
    model.back_page();
    assert_eq!(model.snapshot().page, Page::Leaving);
    model.advance();
    assert_eq!(model.snapshot().page, Page::Choice);
    assert!(model.snapshot().peers.is_empty());
    assert!(model.snapshot().local_address.is_empty());
}

fn set_scenario(model: &mut model::DisplayState, scenario: Scenario) {
    for _ in 0..10 {
        if model.scenario() == scenario {
            return;
        }
        model.next_scenario();
    }
    panic!("scenario is not reachable");
}

fn enter_host(model: &mut model::DisplayState, network: bool) {
    if network {
        model.activate(0);
    } else {
        model.switch_entry();
    }
    model.activate(0);
}

#[test]
fn game_entry_returns_to_game_instead_of_the_game_picker() {
    let mut model = model::DisplayState::default();
    model.switch_entry();
    assert_eq!(model.snapshot().source, LaunchSource::Game);
    assert_eq!(model.snapshot().page, Page::Choice);
    model.back_page();
    assert!(model.launcher.active());
    assert_eq!(model.snapshot().outcome, Outcome::SinglePlayer);
    model.restart();
    model.activate(1);
    assert_eq!(model.snapshot().page, Page::Rooms);
    model.advance();
    model.activate(0);
    assert_eq!(model.snapshot().page, Page::Joined);
    model.back_page();
    model.advance();
    assert_eq!(model.snapshot().page, Page::Rooms);
    model.back_page();
    model.back_page();
    assert_eq!(model.snapshot().outcome, Outcome::SinglePlayer);
    model.switch_entry();
    model.expand_discovery(true);
    model.activate(1);
    model.activate(0);
    model.back_page();
    assert_eq!(model.snapshot().page, Page::Games);
}

#[test]
fn mock_progress_cannot_replace_local_confirmation_and_has_a_launch_outcome() {
    let mut model = model::DisplayState::default();
    enter_host(&mut model, false);
    for phase in [Phase::Creating, Phase::WaitingPeer, Phase::PeerPreparing] {
        assert_eq!(model.snapshot().phase, phase);
        model.confirm();
        assert_eq!(model.snapshot().phase, phase);
        model.advance();
    }
    assert_eq!(model.snapshot().phase, Phase::ApprovalPending);
    model.confirm();
    assert_eq!(model.snapshot().phase, Phase::HostReady);
    model.advance();
    assert_eq!(model.snapshot().phase, Phase::HostReady);
    model.confirm();
    assert_eq!(model.snapshot().phase, Phase::Starting);
    model.advance();
    assert_eq!(model.snapshot().outcome, Outcome::Multiplayer);
    assert!(model.launcher.active());
}

#[test]
fn mock_faults_have_recovery_paths_and_do_not_leak_into_new_sessions() {
    for scenario in [
        Scenario::ConnectionFailure,
        Scenario::RestoreFailure,
        Scenario::ExistingRoom,
    ] {
        let mut model = model::DisplayState::default();
        set_scenario(&mut model, scenario);
        enter_host(&mut model, false);
        match scenario {
            Scenario::ConnectionFailure => {
                model.advance();
                assert_eq!(model.snapshot().phase, Phase::Failed);
                model.back_page();
            }
            Scenario::RestoreFailure => {
                model.back_page();
                model.advance();
                assert_eq!(model.snapshot().phase, Phase::RestoreFailed);
                model.confirm();
            }
            Scenario::ExistingRoom => {
                assert_eq!(model.snapshot().phase, Phase::RoomActive);
                model.confirm();
            }
            _ => unreachable!(),
        }
        model.advance();
        assert_eq!(model.snapshot().page, Page::Choice);
        assert_eq!(model.snapshot().phase, Phase::Idle);
        model.activate(0);
        model.advance();
        assert_eq!(model.snapshot().phase, Phase::WaitingPeer);
    }
}

#[test]
fn client_discovery_connection_and_remote_events_are_distinct() {
    for network in [false, true] {
        let mut model = model::DisplayState::default();
        if network {
            model.activate(0);
        } else {
            model.switch_entry();
        }
        model.activate(1);
        model.activate(0);
        assert_eq!(model.snapshot().page, Page::Rooms);
        model.advance();
        model.activate(2);
        assert_eq!(model.snapshot().page, Page::Rooms);
        model.activate(0);
        let phases: &[Phase] = if network {
            &[
                Phase::Connecting,
                Phase::ApprovalPendingClient,
                Phase::NetworkConfirming,
            ]
        } else {
            &[
                Phase::Connecting,
                Phase::Checking,
                Phase::Notifying,
                Phase::WaitingStart,
                Phase::Starting,
            ]
        };
        for phase in phases {
            assert_eq!(model.snapshot().phase, *phase);
            model.confirm();
            assert_eq!(model.snapshot().phase, *phase);
            model.advance();
        }
        if network {
            model.confirm();
            assert_eq!(model.snapshot().outcome, Outcome::KeepNetwork);
            assert!(model.snapshot().connected);
        } else {
            assert_eq!(model.snapshot().outcome, Outcome::Multiplayer);
        }
        assert!(model.launcher.active());
    }
    for (scenario, phase) in [
        (Scenario::Reconnect, Phase::NetworkReconnecting),
        (Scenario::PeerClosed, Phase::Closed),
    ] {
        let mut model = model::DisplayState::default();
        set_scenario(&mut model, scenario);
        enter_host(&mut model, true);
        for _ in 0..3 {
            model.advance();
        }
        model.confirm();
        model.advance();
        model.advance();
        assert_eq!(model.snapshot().phase, phase);
        if scenario == Scenario::Reconnect {
            model.advance();
            assert!(model.snapshot().connected);
        } else {
            assert!(!model.snapshot().connected);
            model.confirm();
            model.advance();
            assert_eq!(model.snapshot().page, Page::Choice);
        }
    }
    let mut model = model::DisplayState::default();
    set_scenario(&mut model, Scenario::EmptyRooms);
    model.activate(0);
    model.activate(1);
    model.advance();
    model.activate(0);
    assert_eq!(model.snapshot().page, Page::Rooms);
    assert!(!model.rooms_visible());
    set_scenario(&mut model, Scenario::EmptyGames);
    model.activate(1);
    model.activate(0);
    assert_eq!(model.snapshot().page, Page::Games);
    assert!(model.snapshot().entries.is_empty());
    set_scenario(&mut model, Scenario::UnsupportedGame);
    model.switch_entry();
    assert_eq!(model.snapshot().page, Page::Unavailable);
    model.back_page();
    assert_eq!(model.snapshot().outcome, Outcome::SinglePlayer);
}

#[test]
fn all_pages_render_through_one_registered_model() {
    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let state = setup_app(&mut app, root);
    state.launcher().close();
    for index in 0..3 {
        app.set_theme(palette::theme(index)).unwrap();
    }
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    assert!(app.dirty_regions().unwrap().is_empty());
    for action in [1, 0, 0, 9, 9, 9, 9, 8, 8, 8, 0, 1, 0, 9, 9, 9, 8, 9] {
        if state.launcher().active() {
            state.launcher().close();
            state.restart();
        }
        match action {
            9 => state.advance(),
            8 => state.back_page(),
            index => state.activate(index),
        }
        app.prepare_text_layout().unwrap();
        app.render_dirty().unwrap();
        let pixels = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            pixels,
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            "{:?}",
            state.snapshot().page
        );
    }
    state.restart();
    state.activate(1);
    state.activate(2);
    assert_eq!(state.snapshot().page, Page::Unavailable);
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
}
