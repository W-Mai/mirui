use super::HOUSING_ART;
use super::runtime::{housing_texture_options, setup_app};
use super::signal::{
    UART_BITS_PER_SYMBOL, UART_FALLING_OFFSET, UART_MESSAGE, UART_RISING_OFFSET, UART_SYMBOL_MS,
    bandwidth_limited_step, uart_bit,
};
use super::state::{ScopeAction, ScopeModel, ScopeState, TriggerEdge};
use crate::core::reactive::Signal;
use crate::prelude::*;
use crate::render::texture::{TexBuf, Texture};
use crate::ui::ComputedRect;

fn stable_sample(state: ScopeState, channel: u8, unit_x: Fixed) -> Fixed {
    state.sample_at_angle(channel, state.base_angle(channel, unit_x))
}

fn assert_layout(width: u16, height: u16) {
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();

    super::super::assert_text_layouts_fit(&app.world);
    let canvas = app.world.find_by_id("scope_canvas").unwrap();
    let rect = app.world.get::<ComputedRect>(canvas).unwrap().0;
    assert!(rect.w > Fixed::from_int(100), "{width}x{height}: {rect:?}");
    assert!(rect.h >= Fixed::from_int(150), "{width}x{height}: {rect:?}");
}

#[test]
fn held_scope_keeps_trace_stable() {
    let mut state = ScopeState::default();
    state.running = false;
    let trace_clock_ms = state.trace_clock_ms;
    assert!(!state.tick(16));
    assert_eq!(state.trace_clock_ms, trace_clock_ms);
}

#[test]
fn stop_and_trigger_actions_are_stable() {
    let mut state = ScopeState::default();
    state.trace_clock_ms = 80;
    let model = ScopeModel {
        state: Signal::new(state),
    };
    ScopeAction::Stop.publish(&model);
    assert_eq!(model.snapshot().trace_clock_ms, 80);
    assert_eq!(TriggerEdge::Rising.toggled().toggled(), TriggerEdge::Rising);
}

#[test]
fn acquisition_clock_does_not_move_the_triggered_phase() {
    let mut state = ScopeState::default();
    let positions = [
        Fixed::from_ratio(1, 4),
        Fixed::from_ratio(1, 2),
        Fixed::from_ratio(3, 4),
    ];
    let before = positions.map(|position| stable_sample(state, 0, position));
    assert!(state.tick(250));
    let after = positions.map(|position| stable_sample(state, 0, position));
    assert_eq!(before, after);
}

#[test]
fn phosphor_traces_only_deviate_slightly_from_triggered_signal() {
    let mut state = ScopeState::default();
    state.trace_clock_ms = 1_337;
    for position in [
        Fixed::from_ratio(1, 5),
        Fixed::from_ratio(1, 2),
        Fixed::from_ratio(4, 5),
    ] {
        let stable = stable_sample(state, 0, position);
        for history in 0..=3 {
            let trace = state.trace_sample(0, position, history);
            assert!((trace - stable).abs() <= Fixed::from_ratio(1, 4));
        }
    }
}

#[test]
fn uart_trigger_anchors_match_requested_edges() {
    for symbol in 0..UART_MESSAGE.len() as i32 {
        let frame = symbol * UART_BITS_PER_SYMBOL;
        let rising = frame + UART_RISING_OFFSET;
        let falling = frame + UART_FALLING_OFFSET;
        assert!(!uart_bit(rising - 1));
        assert!(uart_bit(rising));
        assert!(uart_bit(falling - 1));
        assert!(!uart_bit(falling));
    }

    let mut state = ScopeState::default();
    assert_eq!(state.uart_anchor(0), UART_RISING_OFFSET);
    assert!(
        state.uart_wave_sample(Fixed::from_ratio(-1, 100), 0)
            < state.uart_wave_sample(Fixed::from_ratio(1, 100), 0)
    );
    state.trigger_edge = TriggerEdge::Falling;
    assert_eq!(state.uart_anchor(0), UART_FALLING_OFFSET);
    assert!(
        state.uart_wave_sample(Fixed::from_ratio(-1, 100), 0)
            > state.uart_wave_sample(Fixed::from_ratio(1, 100), 0)
    );
}

#[test]
fn uart_edges_have_finite_slew_and_ringing() {
    let before = bandwidth_limited_step(Fixed::from_ratio(-1, 10));
    let center = bandwidth_limited_step(Fixed::ZERO);
    let after = bandwidth_limited_step(Fixed::from_ratio(1, 10));
    assert!(before > Fixed::ZERO && before < center);
    assert_eq!(center, Fixed::from_ratio(1, 2));
    assert!(after > center && after < Fixed::ONE);
    assert!(bandwidth_limited_step(Fixed::from_ratio(35, 100)) > Fixed::ONE);
    assert!(bandwidth_limited_step(Fixed::from_ratio(-35, 100)) < Fixed::ZERO);
}

#[test]
fn uart_acquisition_advances_one_character_at_a_time() {
    let mut state = ScopeState::default();
    assert_eq!(state.uart_symbol_index(0), 0);
    assert_eq!(state.uart_label(), "UART · H · 01001000");

    assert!(state.tick(UART_SYMBOL_MS as u16));
    assert_eq!(state.uart_symbol_index(0), 1);
    assert_eq!(state.uart_label(), "UART · E · 01000101");
    assert_eq!(
        state.uart_anchor(0),
        UART_BITS_PER_SYMBOL + UART_RISING_OFFSET
    );
}

#[test]
fn trigger_edge_controls_center_crossing_direction() {
    let mut state = ScopeState::default();
    let center = Fixed::from_ratio(1, 2);
    let before = center - Fixed::from_ratio(1, 100);
    let after = center + Fixed::from_ratio(1, 100);

    let rising_before = stable_sample(state, 0, before);
    let rising_center = stable_sample(state, 0, center);
    let rising_after = stable_sample(state, 0, after);
    assert!(rising_before < rising_center && rising_center < rising_after);
    assert!((rising_center - state.trigger_level).abs() <= Fixed::from_ratio(1, 50));

    state.trigger_edge = TriggerEdge::Falling;
    let falling_before = stable_sample(state, 0, before);
    let falling_center = stable_sample(state, 0, center);
    let falling_after = stable_sample(state, 0, after);
    assert!(falling_before > falling_center && falling_center > falling_after);
    assert!((falling_center - state.trigger_level).abs() <= Fixed::from_ratio(1, 50));
}

#[test]
fn scope_actions_update_acquisition_controls() {
    let model = ScopeModel::default();
    ScopeAction::ToggleRun.publish(&model);
    ScopeAction::Stop.publish(&model);
    ScopeAction::TimeScale.publish(&model);
    ScopeAction::ToggleChannelB.publish(&model);

    let state = model.snapshot();
    assert!(!state.running);
    assert_eq!(state.trace_clock_ms, 0);
    assert_eq!(state.time_scale, 3);
    assert!(!state.channel_b);
}

#[test]
fn scope_controls_reflow_without_text_overflow() {
    for (width, height) in [(800, 480), (480, 320), (320, 320)] {
        assert_layout(width, height);
    }
}

#[test]
fn housing_art_uses_lossless_lz4_storage() {
    let meta = Texture::probe_mirx(HOUSING_ART).unwrap();
    assert_eq!((meta.width, meta.height), (800, 480));

    let reader = mirx::Reader::open(HOUSING_ART).unwrap();
    let image = reader.primary().unwrap().unwrap().image().unwrap().unwrap();
    let encoded = image.encoded().unwrap();
    assert_eq!(
        encoded.codings().get(0).unwrap().id(),
        mirx::coding::CodingId::LZ4
    );
    assert!(HOUSING_ART.len() < 200 * 1024);

    let texture = Texture::from_mirx_with(HOUSING_ART, housing_texture_options()).unwrap();
    assert!(matches!(&texture.buf, TexBuf::Aligned(_)));
}
