use super::render::KineticOrbit;
use super::runtime::kinetic_animation_system;
use super::state::{ConsoleMode, ConsoleModel, ConsoleMotion};
use super::style::BACKGROUND;
use super::*;
use crate::core::reactive::flush_signal_dirty;
use crate::gallery::play::change::ChangeSet;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;

fn fixture() -> World {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root, false);
    app.set_root(root);
    app.render().unwrap();
    app.world
}

#[test]
fn model_commands_change_only_business_state() {
    let mut model = ConsoleModel::default();
    assert_eq!(model.select_mode(ConsoleMode::Flow), changed());
    assert_eq!(model.select_mode(ConsoleMode::Flow), ChangeSet::NONE);
    assert_eq!(model.set_intensity(Fixed::from_int(120)), changed());
    assert_eq!(model.cycle_focus(), changed());
    assert_eq!(model.toggle_paused(), changed());
    assert_eq!(model.mode, ConsoleMode::Flow);
    assert_eq!(model.intensity, Fixed::from_int(100));
    assert_eq!(model.focused, 1);
    assert!(model.paused);
}

#[test]
fn motion_uses_delta_throttles_wave_and_settles_when_paused() {
    let mut motion = ConsoleMotion::default();
    let orbit_key = motion.orbit_render_key();
    let wave_key = motion.wave_render_key();

    assert!(motion.advance(ConsoleMode::Orbit, Fixed::from_int(68), false, 100,));
    assert!(motion.orbit_render_key() > orbit_key);
    assert_eq!(motion.wave_render_key(), wave_key);

    assert!(motion.advance(ConsoleMode::Orbit, Fixed::from_int(68), false, 100,));
    assert!(motion.wave_render_key() > wave_key);

    for _ in 0..10 {
        assert!(motion.advance(ConsoleMode::Orbit, Fixed::from_int(68), true, 50,));
    }
    let stopped = motion.phase.phase();
    assert_eq!(motion.phase.rate(), Fixed::ZERO);
    assert!(!motion.advance(ConsoleMode::Orbit, Fixed::from_int(68), true, 5_000,));
    assert_eq!(motion.phase.phase(), stopped);

    assert!(motion.advance(ConsoleMode::Orbit, Fixed::from_int(68), false, 50,));
    assert!(motion.phase.rate() > Fixed::ZERO && motion.phase.rate() < Fixed::ONE);
    assert!(motion.phase.phase() > stopped);
}

#[test]
fn animation_adapter_captures_technical_motion_without_a_world_model() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    let model = app.add_model(ConsoleModel::default());
    let motion = crate::core::reactive::Signal::new(ConsoleMotion::default());
    let system = kinetic_animation_system(model, motion.clone());
    app.world.insert_resource(crate::ecs::DeltaTimeMs(50));

    (system.run)(&mut app.world);

    assert!(motion.get_untracked().orbit_render_key() > 0);
    assert!(app.world.resource::<ConsoleModel>().is_none());
}

#[test]
fn model_and_motion_dirty_bound_views_without_node_sync() {
    let mut world = fixture();
    let orbit = world.find_by_id("kinetic_console_orbit_layer").unwrap();
    let wave = world.find_by_id("kinetic_console_wave_layer").unwrap();
    ViewRegistry::reconcile_observations(&mut world);
    world.remove::<VisualDirty>(orbit);
    world.remove::<VisualDirty>(wave);

    let model = world.get::<KineticOrbit>(orbit).unwrap().model.clone();
    let motion = world.get::<KineticOrbit>(orbit).unwrap().motion.clone();
    assert!(world.resource::<ConsoleModel>().is_none());
    model.cycle_focus();
    flush_signal_dirty(&mut world);
    assert!(world.has::<VisualDirty>(orbit));
    assert!(world.has::<VisualDirty>(wave));
    world.remove::<VisualDirty>(orbit);
    world.remove::<VisualDirty>(wave);

    let mut next = motion.get_untracked();
    assert!(next.advance(model.mode(), model.intensity(), model.paused(), 50));
    motion.set(next);
    flush_signal_dirty(&mut world);
    assert!(world.has::<VisualDirty>(orbit));
    assert!(!world.has::<VisualDirty>(wave));
    world.remove::<VisualDirty>(orbit);

    let mut next = motion.get_untracked();
    assert!(next.advance(model.mode(), model.intensity(), model.paused(), 50));
    motion.set(next);
    flush_signal_dirty(&mut world);
    assert!(world.has::<VisualDirty>(orbit));
    assert!(world.has::<VisualDirty>(wave));
}

#[test]
fn composition_exposes_every_timeline_target_by_id() {
    let world = fixture();
    for id in [
        "kinetic_console_shell",
        "kinetic_console_status",
        "kinetic_console_instrument",
        "kinetic_console_orbit_layer",
        "kinetic_console_wave_layer",
        "kinetic_console_orbit",
        "kinetic_console_flow",
        "kinetic_console_pulse",
        "kinetic_console_intensity",
    ] {
        assert!(world.find_by_id(id).is_some(), "missing {id}");
    }
    assert!(build_sim_timeline(&world).is_some_and(|timeline| timeline.total_ms >= 10_000));
}

#[test]
fn compact_console_resolves_the_active_theme() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    install(&mut app, root, false);
    app.set_root(root);
    app.set_theme(crate::gallery::showcase_theme::LIGHT_ID)
        .unwrap();
    app.render().unwrap();

    let surface = crate::gallery::showcase_theme::light().resolve(BACKGROUND);
    assert_eq!(
        &app.backend.framebuffer().buf.as_slice()[..3],
        &[surface.r, surface.g, surface.b]
    );
}

const fn changed() -> ChangeSet {
    ChangeSet::MODEL.union(ChangeSet::VISUAL)
}
