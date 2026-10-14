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
    app.world.remove_resource::<crate::ecs::MonoClock>();
    app.add_resource(crate::ecs::DeltaTimeMs(16));
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    (app, state)
}

#[test]
fn resizing_keeps_the_page_and_matches_direct_layout_at_the_same_size() {
    use crate::surface::framebuf::FramebufSurface;
    use crate::ui::{ComputedRect, widgets::Text};

    assert_eq!(DEMO_SIZE.fixed_size(), None);
    let (mut app, state) = fixture_at(320, 240);
    let mounted = frames(&app);
    for (width, height) in [(640, 480), (360, 640), (900, 480)] {
        let flush: fn(&[u8], PhysicalRect) = |_, _| {};
        app.backend = FramebufSurface::new(width, height, flush);
        app.render_dirty().unwrap();
        assert_eq!(frames(&app), mounted);
        assert_eq!(state.snapshot().page, Page::Mode);
        let (direct, _) = fixture_at(width, height);
        let geometry = |app: &TestApp| {
            app.world
                .query::<Text>()
                .iter()
                .map(|(entity, text)| {
                    (
                        text.resolve(app.world.resource()).to_string(),
                        app.world.get::<ComputedRect>(entity).unwrap().0,
                    )
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
        let pixels = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(pixels, app.backend.framebuffer().buf.as_slice());
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
            .any(|(_, text)| text.resolve(app.world.resource()) == "本机  192.168.4.1")
    );
    state.confirm();
    app.render_dirty().unwrap();
    assert!(transition::running(&app.world, host).is_some());
    finish_transition(&mut app);
    assert_eq!(frames(&app).len(), 1);
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
    let pixels = app.backend.framebuffer().buf.as_slice().to_vec();
    app.render().unwrap();
    assert_eq!(pixels, app.backend.framebuffer().buf.as_slice());
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
    assert_eq!(model.snapshot().page, Page::Outcome);
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
    assert_eq!(model.snapshot().page, Page::Outcome);
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
        assert_eq!(model.snapshot().page, Page::Outcome);
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
    for theme in [
        crate::gallery::showcase_theme::DARK_ID,
        crate::gallery::showcase_theme::LIGHT_ID,
    ] {
        app.set_theme(theme).unwrap();
    }
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    assert!(app.dirty_regions().unwrap().is_empty());
    for action in [1, 0, 0, 9, 9, 9, 9, 8, 8, 8, 0, 1, 0, 9, 9, 9, 8, 9] {
        match action {
            9 => state.advance(),
            8 => state.back_page(),
            index => state.activate(index),
        }
        app.prepare_text_layout().unwrap();
        app.render_dirty().unwrap();
        let pixels = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(
            pixels,
            app.backend.framebuffer().buf.as_slice(),
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
