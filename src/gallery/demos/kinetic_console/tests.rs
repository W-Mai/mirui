use super::render::{orbit_view, wave_view};
use super::state::{
    ConsoleAction, ConsoleMode, ConsoleModel, ConsoleMotion, ConsoleNodes, ConsoleState,
};
use super::style::BACKGROUND;
use super::*;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::IdMap;
#[cfg(test)]
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;

fn fixture() -> World {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ConsoleModel::default());
    world.insert_resource(ConsoleMotion::default());
    let mut views = ViewRegistry::with_builtins();
    views.insert(orbit_view());
    views.insert(wave_view());
    world.insert_resource(views);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = crate::ui::UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    world
}

#[test]
fn actions_publish_only_changed_control_state() {
    let model = ConsoleModel::default();
    ConsoleAction::Select(ConsoleMode::Flow).publish(&model);
    ConsoleAction::SetIntensity(Fixed::from_int(120)).publish(&model);
    ConsoleAction::CycleFocus.publish(&model);
    ConsoleAction::TogglePaused.publish(&model);
    assert_eq!(
        model.snapshot(),
        ConsoleState {
            mode: ConsoleMode::Flow,
            intensity: Fixed::from_int(100),
            focused: 1,
            paused: true,
        }
    );
}

#[test]
fn animation_uses_delta_and_honors_pause() {
    let mut world = World::new();
    world.insert_resource(ConsoleModel::default());
    world.insert_resource(ConsoleMotion::default());
    world.insert_resource(DeltaTimeMs(100));
    let orbit = world.spawn_empty();
    let wave = world.spawn_empty();
    world.insert_resource(ConsoleNodes { orbit, wave });

    kinetic_animation_system(&mut world);
    let phase = world.resource::<ConsoleMotion>().unwrap().phase.phase();
    assert!(phase > Fixed::ZERO);
    assert!(world.get::<VisualDirty>(orbit).is_some());
    assert!(world.get::<VisualDirty>(wave).is_none());
    world.remove::<VisualDirty>(orbit);

    kinetic_animation_system(&mut world);
    assert!(world.get::<VisualDirty>(orbit).is_some());
    assert!(world.get::<VisualDirty>(wave).is_some());
    world.remove::<VisualDirty>(orbit);
    world.remove::<VisualDirty>(wave);

    let model = world.resource::<ConsoleModel>().unwrap().clone();
    ConsoleAction::TogglePaused.publish(&model);
    kinetic_animation_system(&mut world);
    let braking = *world.resource::<ConsoleMotion>().unwrap();
    assert!(braking.phase.phase() > phase);
    assert!(braking.phase.rate() > Fixed::ZERO && braking.phase.rate() < Fixed::ONE);
    assert!(world.get::<VisualDirty>(orbit).is_some());
    world.remove::<VisualDirty>(orbit);

    for _ in 0..9 {
        kinetic_animation_system(&mut world);
        world.remove::<VisualDirty>(orbit);
        world.remove::<VisualDirty>(wave);
    }
    let stopped = *world.resource::<ConsoleMotion>().unwrap();
    assert_eq!(stopped.phase.rate(), Fixed::ZERO);

    world.insert_resource(DeltaTimeMs(5_000));
    kinetic_animation_system(&mut world);
    assert_eq!(
        world.resource::<ConsoleMotion>().unwrap().phase.phase(),
        stopped.phase.phase()
    );
    assert!(world.get::<VisualDirty>(wave).is_none());
    assert!(world.get::<VisualDirty>(orbit).is_none());

    ConsoleAction::TogglePaused.publish(&model);
    kinetic_animation_system(&mut world);
    let resuming = *world.resource::<ConsoleMotion>().unwrap();
    assert!(resuming.phase.rate() > Fixed::ZERO && resuming.phase.rate() < Fixed::ONE);
    assert!(resuming.phase.phase() > stopped.phase.phase());
    assert!(world.get::<VisualDirty>(orbit).is_some());
}

#[test]
fn intensity_changes_do_not_starve_orbit_animation() {
    let mut world = World::new();
    world.insert_resource(ConsoleModel::default());
    world.insert_resource(ConsoleMotion::default());
    world.insert_resource(DeltaTimeMs(16));
    let orbit = world.spawn_empty();
    let wave = world.spawn_empty();
    world.insert_resource(ConsoleNodes { orbit, wave });
    let model = world.resource::<ConsoleModel>().unwrap().clone();

    ConsoleAction::SetIntensity(Fixed::from_int(82)).publish(&model);
    kinetic_animation_system(&mut world);

    assert!(world.get::<VisualDirty>(orbit).is_some());
    assert!(world.get::<VisualDirty>(wave).is_some());
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
