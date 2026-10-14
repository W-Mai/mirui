use super::data::{LaunchSource, Outcome, Page, Phase, Scenario};
use super::*;
use crate::surface::FramebufferAccess;

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
