#[cfg(test)]
struct ConsoleNodes;

#[cfg(test)]
impl ConsoleNodes {
    const SHELL: &'static str = "orbit_console_shell";
    const WORKSPACE: &'static str = "orbit_console_workspace";
    const INSPECTOR: &'static str = "orbit_console_inspector";
    const STAGE: &'static str = "orbit_console_stage";
    const SIGNAL: &'static str = "orbit_console_signal";
    const ACTIVITY: &'static str = "orbit_console_activity";
    const CONTROLS: &'static str = "orbit_console_controls";
    const MODE_CHIPS: [&'static str; 3] = [
        "orbit_console_mode_orbit",
        "orbit_console_mode_flow",
        "orbit_console_mode_pulse",
    ];
    const SLIDER: &'static str = "orbit_console_intensity";
    const PAUSE: &'static str = "orbit_console_pause";
    const FOCUS_LABEL: &'static str = "orbit_console_focus";
    const INTENSITY_LABEL: &'static str = "orbit_console_intensity_value";
}

use super::runtime::console_animation_system;
use super::state::ConsoleState;
use super::style::VIOLET;
use super::visuals::{ActivityPlot, OrbitInstrument, SignalMeter, UNIT_CIRCLE};
use super::*;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::Children;
use crate::ui::ComputedRect;
use crate::ui::Parent;
use crate::ui::dirty::Dirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::slider::{SliderEvent, SliderHandler};
use crate::ui::widgets::{Button, Slider, Text};

type ConsoleHandle = <ConsoleState as crate::core::model::Model>::Handle;

fn fixture() -> (World, Entity) {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    setup(&mut app, parent, DemoRunMode::Capture);
    ViewRegistry::reconcile_observations(&mut app.world);
    flush_signal_dirty(&mut app.world);
    (app.world, parent)
}

fn state(world: &World) -> ConsoleState {
    crate::core::model::ModelHandle::read(&model(world), |state| *state)
}

fn model(world: &World) -> ConsoleHandle {
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    world
        .get::<OrbitInstrument>(stage)
        .expect("orbit instrument")
        .model
        .clone()
}

fn clear_canvas_dirty(world: &mut World, entities: [Entity; 3]) {
    for entity in entities {
        world.remove::<Dirty>(entity);
        world.remove::<crate::ui::dirty::VisualDirty>(entity);
    }
}

#[test]
fn shared_circle_geometry_is_static() {
    assert!(UNIT_CIRCLE.is_borrowed());
    assert_eq!(UNIT_CIRCLE.commands().len(), 6);
}

#[test]
fn capture_state_is_stable_and_explicit() {
    let state = ConsoleState::capture();
    assert_eq!(state.mode, ConsoleMode::Orbit);
    assert_eq!(state.intensity, 78);
    assert_eq!(state.focused_node, 1);
    assert_eq!(state.phase(), Fixed::from_int(32));
    assert!(!state.paused);
}

#[test]
fn animation_uses_elapsed_time_and_honors_pause() {
    let mut state = ConsoleState::live();
    state.advance(50);
    assert_eq!(state.phase(), Fixed::from_int(3));
    state.advance(0);
    assert_eq!(state.phase(), Fixed::from_int(3));
    state.toggle_paused();
    state.advance(50);
    assert_eq!(state.phase(), Fixed::from_int(3));
}

#[test]
fn animation_system_consumes_framework_delta_time() {
    let (world, _) = fixture();
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    let instrument = world
        .get::<OrbitInstrument>(stage)
        .expect("orbit instrument");
    console_animation_system(&instrument.model, crate::ecs::DeltaTimeMs(50));

    assert_eq!(state(&world).phase(), Fixed::from_int(35));
}

#[test]
fn phase_updates_only_invalidate_phase_consumers() {
    let (mut world, _) = fixture();
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
    let activity = world
        .find_by_id(ConsoleNodes::ACTIVITY)
        .expect("activity id");
    clear_canvas_dirty(&mut world, [stage, signal, activity]);

    let model = world
        .get::<OrbitInstrument>(stage)
        .expect("orbit instrument")
        .model
        .clone();
    console_animation_system(&model, crate::ecs::DeltaTimeMs(50));
    flush_signal_dirty(&mut world);

    assert!(world.has::<crate::ui::dirty::VisualDirty>(stage));
    assert!(!world.has::<crate::ui::dirty::VisualDirty>(signal));
    assert!(world.has::<crate::ui::dirty::VisualDirty>(activity));
}

#[test]
fn observed_fields_invalidate_only_their_canvas_consumers() {
    let (mut world, _) = fixture();
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
    let activity = world
        .find_by_id(ConsoleNodes::ACTIVITY)
        .expect("activity id");
    let canvases = [stage, signal, activity];
    let model = model(&world);

    clear_canvas_dirty(&mut world, canvases);
    model.set_intensity(Fixed::from_int(42));
    flush_signal_dirty(&mut world);
    assert!(world.has::<crate::ui::dirty::VisualDirty>(stage));
    assert!(world.has::<crate::ui::dirty::VisualDirty>(signal));
    assert!(!world.has::<crate::ui::dirty::VisualDirty>(activity));

    clear_canvas_dirty(&mut world, canvases);
    model.cycle_focus();
    flush_signal_dirty(&mut world);
    assert!(world.has::<crate::ui::dirty::VisualDirty>(stage));
    assert!(!world.has::<crate::ui::dirty::VisualDirty>(signal));
    assert!(!world.has::<crate::ui::dirty::VisualDirty>(activity));

    clear_canvas_dirty(&mut world, canvases);
    model.toggle_paused();
    flush_signal_dirty(&mut world);
    for entity in canvases {
        assert!(!world.has::<crate::ui::dirty::VisualDirty>(entity));
    }
    let pause = world.find_by_id(ConsoleNodes::PAUSE).expect("pause id");
    assert!(
        world
            .get::<Text>(pause)
            .is_some_and(|text| text.resolve(&world).contains("RESUME"))
    );
}

#[test]
fn build_widgets_creates_product_regions_and_controls() {
    let (world, parent) = fixture();
    let shell = world.find_by_id(ConsoleNodes::SHELL).expect("shell id");
    let workspace = world
        .find_by_id(ConsoleNodes::WORKSPACE)
        .expect("workspace id");
    let inspector = world
        .find_by_id(ConsoleNodes::INSPECTOR)
        .expect("inspector id");
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
    let activity = world
        .find_by_id(ConsoleNodes::ACTIVITY)
        .expect("activity id");
    let controls = world
        .find_by_id(ConsoleNodes::CONTROLS)
        .expect("controls id");
    let slider = world.find_by_id(ConsoleNodes::SLIDER).expect("slider id");
    assert_eq!(
        world
            .get::<Children>(parent)
            .map(|children| children.0.len()),
        Some(1)
    );
    assert_eq!(
        world.get::<Parent>(shell).map(|parent| parent.0),
        Some(parent)
    );
    assert_eq!(
        world.get::<Parent>(workspace).map(|parent| parent.0),
        Some(shell)
    );
    assert_eq!(
        world.get::<Parent>(stage).map(|parent| parent.0),
        Some(workspace)
    );
    assert_eq!(
        world.get::<Parent>(inspector).map(|parent| parent.0),
        Some(workspace)
    );
    for card in [signal, activity, controls] {
        assert_eq!(
            world.get::<Parent>(card).map(|parent| parent.0),
            Some(inspector)
        );
    }
    assert!(world.has::<OrbitInstrument>(stage));
    assert!(world.has::<SignalMeter>(signal));
    assert!(world.has::<ActivityPlot>(activity));
    assert!(world.has::<Slider>(slider));
}

#[test]
fn workspace_responds_across_supported_viewports() {
    use crate::types::Viewport;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(320, 568), (480, 320), (768, 480), (1024, 640), (1440, 900)] {
        let (mut world, parent) = fixture();
        update_layout(
            &mut world,
            parent,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let rect = |world: &World, id| {
            let entity = world.find_by_id(id).expect("responsive region id");
            world.get::<ComputedRect>(entity).expect("computed rect").0
        };
        let workspace = rect(&world, ConsoleNodes::WORKSPACE);
        let stage = rect(&world, ConsoleNodes::STAGE);
        let inspector = rect(&world, ConsoleNodes::INSPECTOR);
        assert!(stage.w > Fixed::ZERO && stage.h > Fixed::ZERO);
        assert!(inspector.w > Fixed::ZERO && inspector.h > Fixed::ZERO);
        assert!(stage.x >= workspace.x && stage.x + stage.w <= workspace.x + workspace.w);
        assert!(
            inspector.x >= workspace.x && inspector.x + inspector.w <= workspace.x + workspace.w
        );
        assert!(stage.y >= workspace.y && stage.y + stage.h <= workspace.y + workspace.h);
        assert!(
            inspector.y >= workspace.y && inspector.y + inspector.h <= workspace.y + workspace.h
        );
        if width == 320 {
            assert!(inspector.y > stage.y);
        } else {
            assert_eq!(inspector.y, stage.y);
        }
    }
}

#[test]
fn controls_update_shared_state_and_visual_style() {
    let (mut world, _) = fixture();
    let pulse = world
        .find_by_id(ConsoleNodes::MODE_CHIPS[2])
        .expect("pulse mode id");
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    let signal = world.find_by_id(ConsoleNodes::SIGNAL).expect("signal id");
    let activity = world
        .find_by_id(ConsoleNodes::ACTIVITY)
        .expect("activity id");
    clear_canvas_dirty(&mut world, [stage, signal, activity]);
    GestureHandler::trigger(
        &mut world,
        pulse,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: pulse,
        },
    );
    assert_eq!(state(&world).mode, ConsoleMode::Pulse);
    flush_signal_dirty(&mut world);
    assert_eq!(
        world
            .get::<Button>(pulse)
            .map(|button| button.normal_color.resolve(&Theme::dark())),
        Some(Theme::dark().resolve(VIOLET))
    );
    assert!(!world.has::<Dirty>(stage));
    for entity in [stage, signal, activity] {
        assert!(world.has::<crate::ui::dirty::VisualDirty>(entity));
    }
}

#[test]
fn shell_and_instruments_resolve_the_active_theme() {
    let render = |theme_id| {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup(&mut app, root, DemoRunMode::Capture);
        app.set_root(root);
        app.set_theme(theme_id).unwrap();
        app.render().unwrap();
        <[u8; 3]>::try_from(&app.backend.framebuffer().buf.as_slice()[..3]).unwrap()
    };

    let light = render(crate::gallery::showcase_theme::LIGHT_ID);
    let dark = render(crate::gallery::showcase_theme::DARK_ID);
    assert_ne!(light, dark);
    assert!(
        light.iter().map(|channel| u16::from(*channel)).sum::<u16>()
            > dark.iter().map(|channel| u16::from(*channel)).sum::<u16>()
    );
}

#[test]
fn stage_tap_cycles_focus_without_allocation_state() {
    let (mut world, _) = fixture();
    let stage = world.find_by_id(ConsoleNodes::STAGE).expect("stage id");
    GestureHandler::trigger(
        &mut world,
        stage,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: stage,
        },
    );
    assert_eq!(state(&world).focused_node, 2);
    flush_signal_dirty(&mut world);
    let focus_label = world
        .find_by_id(ConsoleNodes::FOCUS_LABEL)
        .expect("focus label id");
    assert!(
        world
            .get::<Text>(focus_label)
            .is_some_and(|text| text.resolve(&world).contains("NODE 03"))
    );
}

#[test]
fn intensity_and_pause_handlers_update_visible_state() {
    let (mut world, _) = fixture();
    let slider = world.find_by_id(ConsoleNodes::SLIDER).expect("slider id");
    let intensity_label = world
        .find_by_id(ConsoleNodes::INTENSITY_LABEL)
        .expect("intensity label id");
    let pause = world.find_by_id(ConsoleNodes::PAUSE).expect("pause id");
    let callback = world
        .get::<SliderHandler>(slider)
        .expect("slider handler")
        .on_event
        .clone_out();
    callback.call(
        &mut world,
        slider,
        &SliderEvent::ValueChanged {
            new: Fixed::from_int(42),
            old: Fixed::from_int(78),
        },
    );
    assert_eq!(state(&world).intensity, 42);
    flush_signal_dirty(&mut world);
    assert!(
        world
            .get::<Text>(intensity_label)
            .is_some_and(|text| text.resolve(&world) == "42")
    );

    GestureHandler::trigger(
        &mut world,
        pause,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: pause,
        },
    );
    assert!(state(&world).paused);
    flush_signal_dirty(&mut world);
    assert!(
        world
            .get::<Text>(pause)
            .is_some_and(|text| text.resolve(&world).contains("RESUME"))
    );
}
