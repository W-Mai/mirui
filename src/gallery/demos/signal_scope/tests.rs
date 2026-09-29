use super::HOUSING_ART;
use super::VIEWPORT;
use super::render::ScopeCanvas;
use super::runtime::{housing_texture_options, scope_animation_system, setup_app};
use super::signal::{
    UART_BITS_PER_SYMBOL, UART_FALLING_OFFSET, UART_MESSAGE, UART_RISING_OFFSET, UART_SYMBOL_MS,
    bandwidth_limited_step, uart_bit, uart_label,
};
use super::state::{ScopeControl, ScopeModel, TriggerEdge};
use crate::ecs::SystemScheduler;
use crate::gallery::play::change::ChangeSet;
use crate::prelude::*;
use crate::render::texture::{TexBuf, Texture};
use crate::ui::ComputedRect;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;

fn stable_sample(state: ScopeModel, channel: u8, unit_x: Fixed) -> Fixed {
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
    let mut state = ScopeModel::default();
    state.running = false;
    let trace_clock_ms = state.trace_clock_ms;
    assert_eq!(state.advance_ms(16), ChangeSet::NONE);
    assert_eq!(state.trace_clock_ms, trace_clock_ms);
}

#[test]
fn stop_and_trigger_actions_are_stable() {
    let mut state = ScopeModel::default();
    state.trace_clock_ms = 80;
    state.activate(ScopeControl::Stop);
    assert_eq!(state.trace_clock_ms, 80);
    assert_eq!(TriggerEdge::Rising.toggled().toggled(), TriggerEdge::Rising);
}

#[test]
fn acquisition_clock_does_not_move_the_triggered_phase() {
    let mut state = ScopeModel::default();
    let positions = [
        Fixed::from_ratio(1, 4),
        Fixed::from_ratio(1, 2),
        Fixed::from_ratio(3, 4),
    ];
    let before = positions.map(|position| stable_sample(state, 0, position));
    assert_eq!(state.advance_ms(250), ChangeSet::VISUAL);
    let after = positions.map(|position| stable_sample(state, 0, position));
    assert_eq!(before, after);
}

#[test]
fn phosphor_traces_only_deviate_slightly_from_triggered_signal() {
    let mut state = ScopeModel::default();
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

    let mut state = ScopeModel::default();
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
    let mut state = ScopeModel::default();
    assert_eq!(state.uart_symbol_index(0), 0);
    assert_eq!(
        uart_label(state.uart_symbol_index(0)),
        "UART · H · 01001000"
    );

    assert_eq!(state.advance_ms(UART_SYMBOL_MS as u16), ChangeSet::VISUAL);
    assert_eq!(state.uart_symbol_index(0), 1);
    assert_eq!(
        uart_label(state.uart_symbol_index(0)),
        "UART · E · 01000101"
    );
    assert_eq!(
        state.uart_anchor(0),
        UART_BITS_PER_SYMBOL + UART_RISING_OFFSET
    );
}

#[test]
fn registered_scope_models_publish_only_real_instance_changes() {
    let mut app = App::headless(1, 1);
    let first = app.add_model(ScopeModel::default());
    let second = app.add_model(ScopeModel::default());

    let first_revision = first.visual_revision();
    let second_revision = second.visual_revision();
    assert_eq!(first.advance_ms(0), ChangeSet::NONE);
    assert_eq!(first.visual_revision(), first_revision);

    assert_eq!(first.advance_ms(UART_SYMBOL_MS as u16), ChangeSet::VISUAL);
    assert_eq!(first.uart_symbol(), 1);
    assert!(first.visual_revision() > first_revision);
    assert_eq!(second.uart_symbol(), 0);
    assert_eq!(second.visual_revision(), second_revision);

    first.activate(ScopeControl::Stop);
    let stopped_revision = first.visual_revision();
    assert_eq!(first.advance_ms(16), ChangeSet::NONE);
    assert_eq!(first.visual_revision(), stopped_revision);
}

#[test]
fn bound_tick_updates_uart_and_only_publishes_real_visual_changes() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();

    let canvas = app.world.find_by_id("scope_canvas").unwrap();
    let uart = app.world.find_by_id("scope_uart_readout").unwrap();
    let model = app.world.get::<ScopeCanvas>(canvas).unwrap().model.clone();
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world.remove::<VisualDirty>(canvas);

    let mut scheduler = SystemScheduler::new();
    scheduler.add(scope_animation_system::system(model.clone()));
    app.world
        .insert_resource(crate::ecs::DeltaTimeMs(UART_SYMBOL_MS as u16));
    scheduler.run_all(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);

    assert_eq!(model.uart_symbol(), 1);
    assert!(app.world.has::<VisualDirty>(canvas));
    assert_eq!(
        app.world
            .get::<Text>(uart)
            .unwrap()
            .resolve(&app.world)
            .as_ref(),
        "UART · E · 01000101"
    );

    app.world.remove::<VisualDirty>(canvas);
    let revision = model.visual_revision();
    app.world.insert_resource(crate::ecs::DeltaTimeMs(0));
    scheduler.run_all(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(model.visual_revision(), revision);
    assert!(!app.world.has::<VisualDirty>(canvas));

    model.activate(ScopeControl::Stop);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world.remove::<VisualDirty>(canvas);
    let revision = model.visual_revision();
    app.world.insert_resource(crate::ecs::DeltaTimeMs(16));
    scheduler.run_all(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(model.visual_revision(), revision);
    assert!(!app.world.has::<VisualDirty>(canvas));
}

#[test]
fn trigger_edge_controls_center_crossing_direction() {
    let mut state = ScopeModel::default();
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
    let mut model = ScopeModel::default();
    model.activate(ScopeControl::Acquire);
    model.activate(ScopeControl::Stop);
    model.activate(ScopeControl::TimeScale);
    model.activate(ScopeControl::ChannelB);

    assert!(!model.running);
    assert_eq!(model.trace_clock_ms, 0);
    assert_eq!(model.time_scale, 3);
    assert!(!model.channel_b);
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
