use super::geometry::{CurvePaths, make_lane, path_window_end, update_lane_for_stage};
use super::runtime::{CurveMotion, curve_text_animation_system};
use super::stage::CurveStage;
use super::state::{BASE_STAGE_HEIGHT, BASE_STAGE_WIDTH, CurveModel, CurveStageSize};
use super::style::BACKGROUND;
use super::{VIEWPORT, install};
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::DeltaTimeMs;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::render::path::PathStore;
use crate::surface::FramebufferAccess;
use crate::types::{Transform, Viewport};
use crate::ui::Theme;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::{Slider, Text};

type CurveHandle = <CurveModel as crate::core::model::Model>::Handle;

type TestApp = App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
    crate::app::SwRendererFactory,
>;

fn fixture() -> TestApp {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    install(&mut app, parent);
    app.set_root(parent);
    ViewRegistry::reconcile_observations(&mut app.world);
    flush_signal_dirty(&mut app.world);
    app
}

fn model_handle(world: &World) -> CurveHandle {
    let stage = world.query::<CurveStage>().iter().next().unwrap().0;
    world.get::<CurveStage>(stage).unwrap().model.clone()
}

fn stage_entity(world: &World) -> Entity {
    world.query::<CurveStage>().iter().next().unwrap().0
}

fn motion_rate(world: &World) -> Fixed {
    world
        .get::<CurveMotion>(stage_entity(world))
        .expect("CurveStage motion")
        .rate()
}

fn clear_visual_dirty(world: &mut World, entities: &[Entity]) {
    for &entity in entities {
        world.remove::<VisualDirty>(entity);
    }
}

#[test]
fn animated_lanes_keep_a_bounded_flattened_shape() {
    for lane in 0..3 {
        let path = make_lane(lane);
        let requirements = crate::text::baseline::PathMeasure::new(
            &path,
            0,
            crate::text::baseline::DEFAULT_TOLERANCE,
        )
        .unwrap()
        .requirements()
        .unwrap();
        assert!(
            requirements.segments <= 128,
            "lane {lane}: {requirements:?}"
        );
    }
}

#[test]
fn stage_size_is_published_as_one_clamped_value() {
    assert_eq!(
        CurveStageSize::from_dimensions(Fixed::from_int(80), Fixed::from_int(160)),
        CurveStageSize {
            width: Fixed::from_int(120),
            height: Fixed::from_int(220),
        }
    );
    assert_eq!(
        CurveStageSize::from_dimensions(Fixed::from_int(640), Fixed::from_int(360)),
        CurveStageSize {
            width: Fixed::from_int(640),
            height: Fixed::from_int(360),
        }
    );
}

#[test]
fn layout_binding_updates_responsive_geometry_without_animation() {
    let mut app = fixture();
    let root = app.root.expect("root");
    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &Viewport::new(400, 540, Fixed::ONE),
    );

    let stage = app.world.find_by_id("curve_text_stage").unwrap();
    let primary = app.world.find_by_id("curve_text_primary").unwrap();
    let controls = app
        .world
        .find_by_id("curve_text_controls")
        .expect("Curve Text controls");
    let stage_rect = app.world.get::<crate::ui::ComputedRect>(stage).unwrap().0;
    let stage_size = model_handle(&app.world).stage_size();
    assert_eq!(
        stage_size,
        CurveStageSize::from_dimensions(stage_rect.w, stage_rect.h)
    );
    assert_eq!(
        app.world.get::<Style>(controls).unwrap().layout.height,
        Dimension::px(146)
    );
    assert_eq!(app.world.get::<Style>(primary).unwrap().font_size, Some(18));
    super::super::assert_text_layouts_fit(&app.world);
    assert_eq!(
        app.world
            .get::<crate::text::TextPath>(primary)
            .unwrap()
            .end(),
        Some(path_window_end(stage_size.width))
    );

    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );
    assert_eq!(
        app.world.get::<Style>(controls).unwrap().layout.height,
        Dimension::px(62)
    );
    assert_eq!(app.world.get::<Style>(primary).unwrap().font_size, Some(30));
}

#[test]
fn controls_and_stage_share_the_available_height_at_medium_sizes() {
    for (width, height, expected_controls, expected_stage) in
        [(672, 666, 104, 300), (960, 540, 62, 250)]
    {
        let mut app = fixture();
        let root = app.root.expect("root");
        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        let rect = |world: &World, id| {
            world
                .get::<crate::ui::ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let shell = rect(&app.world, "curve_text_shell");
        let stage = rect(&app.world, "curve_text_stage_shell");
        let controls = rect(&app.world, "curve_text_controls");

        assert_eq!(stage.h, Fixed::from_int(expected_stage), "{width}x{height}");
        assert_eq!(
            controls.h,
            Fixed::from_int(expected_controls),
            "{width}x{height}"
        );
        assert!(
            controls.y + controls.h <= shell.y + shell.h,
            "{width}x{height}: {controls:?} {shell:?}"
        );
        super::super::assert_text_layouts_fit(&app.world);
    }
}

#[test]
fn responsive_text_window_stays_inside_every_scaled_lane() {
    for width in [Fixed::from_int(120), Fixed::from_int(378), BASE_STAGE_WIDTH] {
        for lane in 0..3 {
            let mut path = make_lane(lane);
            update_lane_for_stage(
                &mut path,
                lane,
                Fixed::ZERO,
                Fixed::from_int(68),
                width,
                BASE_STAGE_HEIGHT,
            );
            let mut segments = [crate::text::baseline::MeasuredSegment {
                end: Point::ZERO,
                end_distance: Fixed::ZERO,
            }; 128];
            let baseline = crate::text::baseline::PathMeasure::new(
                &path,
                0,
                crate::text::baseline::DEFAULT_TOLERANCE,
            )
            .unwrap()
            .measure_into(&mut segments)
            .unwrap();
            assert!(path_window_end(width) <= baseline.length());
        }
    }
}

#[test]
fn animated_lane_baseline_is_temporally_continuous() {
    let mut path = make_lane(0);
    let mut previous: Option<(Point, Point)> = None;
    let mut segment_count = None;
    let mut maximum_position_step = Fixed::ZERO;
    let mut maximum_tangent_step = Fixed::ZERO;
    let mut previous_quad: Option<[Point; 4]> = None;
    let mut maximum_quad_step = Fixed::ZERO;
    for frame in 0..360 {
        let phase = Fixed::from_ratio(frame * 1_024, 1_000);
        let envelope =
            Fixed::from_ratio(17, 20) + Fixed::sin_deg(phase / 3) * Fixed::from_ratio(3, 20);
        update_lane_for_stage(
            &mut path,
            0,
            phase,
            Fixed::from_int(68) * envelope,
            BASE_STAGE_WIDTH,
            BASE_STAGE_HEIGHT,
        );
        let measure = crate::text::baseline::PathMeasure::new(
            &path,
            0,
            crate::text::baseline::DEFAULT_TOLERANCE,
        )
        .unwrap();
        let requirements = measure.requirements().unwrap();
        let mut segments = alloc::vec![
            crate::text::baseline::MeasuredSegment {
                end: Point::ZERO,
                end_distance: Fixed::ZERO,
            };
            requirements.segments
        ];
        let baseline = measure.measure_into(&mut segments).unwrap();
        let mut cursor = baseline.cursor();
        let sample = cursor.sample_forward(Fixed::from_int(360)).unwrap();
        let pose = Transform {
            m00: sample.unit_tangent.x,
            m01: -sample.unit_tangent.y,
            tx: sample.position.x,
            m10: sample.unit_tangent.y,
            m11: sample.unit_tangent.x,
            ty: sample.position.y,
        };
        let quad = pose.apply_rect(Rect::new(0, -24, 32, 32));
        assert_eq!(
            *segment_count.get_or_insert(requirements.segments),
            requirements.segments
        );
        if let Some((last_position, last_tangent)) = previous {
            let position_step = (sample.position.x - last_position.x)
                .abs()
                .max((sample.position.y - last_position.y).abs());
            let tangent_step = (sample.unit_tangent.x - last_tangent.x)
                .abs()
                .max((sample.unit_tangent.y - last_tangent.y).abs());
            maximum_position_step = maximum_position_step.max(position_step);
            maximum_tangent_step = maximum_tangent_step.max(tangent_step);
        }
        if let Some(previous_quad) = previous_quad {
            for (current, previous) in quad.into_iter().zip(previous_quad) {
                maximum_quad_step = maximum_quad_step
                    .max((current.x - previous.x).abs())
                    .max((current.y - previous.y).abs());
            }
        }
        previous = Some((sample.position, sample.unit_tangent));
        previous_quad = Some(quad);
    }
    assert!(
        maximum_position_step <= Fixed::ONE,
        "{maximum_position_step:?}"
    );
    assert!(
        maximum_tangent_step <= Fixed::from_ratio(1, 32),
        "{maximum_tangent_step:?}"
    );
    assert!(
        maximum_quad_step <= Fixed::from_int(2),
        "{maximum_quad_step:?}"
    );
}

fn tap(world: &mut World, id: &'static str) {
    let target = world.find_by_id(id).expect("control id");
    GestureHandler::trigger(
        world,
        target,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        },
    );
    flush_signal_dirty(world);
}

#[test]
fn exposes_text_and_control_entities_by_id() {
    let app = fixture();
    assert_eq!(app.world.query::<CurveStage>().collect().len(), 1);
    assert!(app.world.resource::<CurveModel>().is_none());
    for id in [
        "curve_text_shell",
        "curve_text_stage",
        "curve_text_primary",
        "curve_text_multiscript",
        "curve_text_caption",
        "curve_text_amplitude",
        "curve_text_speed",
        "curve_text_direction",
        "curve_text_pause",
    ] {
        assert!(app.world.find_by_id(id).is_some(), "missing {id}");
    }
    for id in [
        "curve_text_primary",
        "curve_text_multiscript",
        "curve_text_caption",
        "curve_text_direction",
        "curve_text_pause",
    ] {
        let entity = app.world.find_by_id(id).unwrap();
        assert!(app.world.get::<Text>(entity).is_some(), "{id} is not Text");
    }
}

#[test]
fn animation_reuses_fixed_path_topology_and_capacity() {
    let mut app = fixture();
    let paths = *app.world.resource::<CurvePaths>().unwrap();
    let before = paths.ids.map(|id| {
        let store = app.world.resource::<PathStore>().unwrap();
        let path = store.get(id).unwrap();
        (
            path.commands().len(),
            path.command_capacity(),
            store.revision(id).unwrap().unwrap(),
        )
    });
    app.world.insert_resource(DeltaTimeMs(16));
    for _ in 0..12 {
        curve_text_animation_system(&mut app.world);
        flush_signal_dirty(&mut app.world);
    }
    for (index, id) in paths.ids.into_iter().enumerate() {
        let store = app.world.resource::<PathStore>().unwrap();
        let path = store.get(id).unwrap();
        assert_eq!(path.commands().len(), before[index].0);
        assert_eq!(path.command_capacity(), before[index].1);
        assert!(store.revision(id).unwrap().unwrap() != before[index].2);
    }
}

#[test]
#[should_panic(expected = "Curve Text animation requires DeltaTimeMs")]
fn animation_requires_elapsed_time() {
    let mut app = fixture();
    curve_text_animation_system(&mut app.world);
}

#[test]
#[should_panic(expected = "CurveStage requires CurveMotion")]
fn every_stage_requires_its_own_motion_state() {
    let mut app = fixture();
    let stage = stage_entity(&app.world);
    app.world.remove::<CurveMotion>(stage);
    app.world.insert_resource(DeltaTimeMs(16));
    curve_text_animation_system(&mut app.world);
}

#[test]
fn animation_advances_each_registered_stage_instance() {
    let mut app = App::headless(320, 240);
    let forward = app.add_model(CurveModel::default());
    let reverse = app.add_model(CurveModel::default());
    reverse.toggle_direction();
    app.world.spawn((
        CurveStage {
            model: forward.clone(),
        },
        CurveMotion::default(),
    ));
    app.world.spawn((
        CurveStage {
            model: reverse.clone(),
        },
        CurveMotion::default(),
    ));
    app.world.insert_resource(DeltaTimeMs(50));

    curve_text_animation_system(&mut app.world);

    assert!(forward.phase() > Fixed::ZERO);
    assert!(reverse.phase() > Fixed::from_int(300));
    assert_ne!(forward.phase(), reverse.phase());
}

#[test]
fn phase_changes_dirty_only_the_stage_and_path_text() {
    let mut app = fixture();
    let stage = stage_entity(&app.world);
    let path_text = [
        "curve_text_primary",
        "curve_text_multiscript",
        "curve_text_caption",
    ]
    .map(|id| app.world.find_by_id(id).unwrap());
    let unrelated = app.world.find_by_id("curve_text_speed").unwrap();
    let observed = [stage, path_text[0], path_text[1], path_text[2]];
    clear_visual_dirty(&mut app.world, &observed);
    app.world.remove::<VisualDirty>(unrelated);
    app.world.insert_resource(DeltaTimeMs(50));

    curve_text_animation_system(&mut app.world);
    flush_signal_dirty(&mut app.world);

    for entity in observed {
        assert!(app.world.has::<VisualDirty>(entity));
    }
    assert!(!app.world.has::<VisualDirty>(unrelated));
}

#[test]
fn pause_brakes_and_resume_continues_from_relative_phase() {
    let mut app = fixture();
    app.world.insert_resource(DeltaTimeMs(50));
    curve_text_animation_system(&mut app.world);
    let model = model_handle(&app.world);
    let moving = model.phase();

    tap(&mut app.world, "curve_text_pause");
    for _ in 0..12 {
        curve_text_animation_system(&mut app.world);
    }
    let stopped = model.phase();
    assert!(stopped > moving);
    assert_eq!(motion_rate(&app.world), Fixed::ZERO);

    app.world.insert_resource(DeltaTimeMs(5_000));
    curve_text_animation_system(&mut app.world);
    assert_eq!(model.phase(), stopped);

    tap(&mut app.world, "curve_text_pause");
    curve_text_animation_system(&mut app.world);
    assert!(model.phase() > stopped);
    assert!(motion_rate(&app.world) < Fixed::ONE);
}

#[test]
fn fully_braked_animation_does_not_redirty_the_stage() {
    let mut app = fixture();
    app.world.insert_resource(DeltaTimeMs(50));
    tap(&mut app.world, "curve_text_pause");
    for _ in 0..12 {
        curve_text_animation_system(&mut app.world);
        flush_signal_dirty(&mut app.world);
    }
    assert_eq!(motion_rate(&app.world), Fixed::ZERO);

    let observed = [
        stage_entity(&app.world),
        app.world.find_by_id("curve_text_primary").unwrap(),
        app.world.find_by_id("curve_text_multiscript").unwrap(),
        app.world.find_by_id("curve_text_caption").unwrap(),
    ];
    clear_visual_dirty(&mut app.world, &observed);

    curve_text_animation_system(&mut app.world);
    flush_signal_dirty(&mut app.world);

    for entity in observed {
        assert!(!app.world.has::<VisualDirty>(entity));
    }
}

#[test]
fn direction_and_sliders_publish_semantic_state() {
    let mut app = fixture();
    let model = model_handle(&app.world);
    tap(&mut app.world, "curve_text_direction");
    assert!(model.reversed());

    model.set_amplitude(Fixed::from_int(120));
    model.set_speed(Fixed::ZERO);
    flush_signal_dirty(&mut app.world);
    assert_eq!(model.amplitude(), Fixed::from_int(96));
    assert_eq!(model.speed(), Fixed::from_int(20));
    assert_eq!(
        app.world
            .get::<Slider>(app.world.find_by_id("curve_text_amplitude").unwrap())
            .unwrap()
            .value,
        Fixed::from_int(96)
    );
    assert_eq!(
        app.world
            .get::<Slider>(app.world.find_by_id("curve_text_speed").unwrap())
            .unwrap()
            .value,
        Fixed::from_int(20)
    );
    for (id, expected) in [
        ("curve_text_amplitude_value", "96"),
        ("curve_text_speed_value", "20"),
    ] {
        let text = app
            .world
            .get::<Text>(app.world.find_by_id(id).unwrap())
            .unwrap();
        assert_eq!(text.resolve(&app.world), expected);
        assert!(text.text_capacity().is_some());
    }
}

#[test]
fn reverse_control_applies_to_the_next_motion_step() {
    let mut app = fixture();
    let model = model_handle(&app.world);
    app.world.insert_resource(DeltaTimeMs(50));

    curve_text_animation_system(&mut app.world);
    let forward = model.phase();
    assert!(forward > Fixed::ZERO);

    tap(&mut app.world, "curve_text_direction");
    curve_text_animation_system(&mut app.world);
    assert!(model.reversed());
    assert!(model.phase() < forward);
}

#[test]
fn renders_curved_text_and_guides() {
    let mut app = fixture();
    app.render().unwrap();
    let texture = app.backend.framebuffer();
    let background = Theme::default().resolve(BACKGROUND);
    let non_background = texture
        .buf
        .as_slice()
        .chunks_exact(4)
        .filter(|pixel| pixel[..3] != [background.r, background.g, background.b])
        .count();
    assert!(non_background > 20_000);
}

#[test]
fn shell_and_custom_stage_resolve_the_active_theme() {
    let mut app = fixture();
    app.world.insert_resource(Theme::light());
    app.render().unwrap();

    let surface = Theme::light().resolve(BACKGROUND);
    assert_eq!(
        &app.backend.framebuffer().buf.as_slice()[..3],
        &[surface.r, surface.g, surface.b]
    );
}
