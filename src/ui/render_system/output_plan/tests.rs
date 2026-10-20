use super::*;
use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::render::texture::ColorFormat;
use crate::surface::FramebufferAccess;
use crate::ui::OffscreenRender;
use crate::ui::widgets::MirrorOf;
use core::cell::Cell;

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

fn app(width: u16, height: u16) -> TestApp {
    let mut app = App::headless(width, height);
    app.with_default_widgets()
        .with_offscreen_pool_budget(8 * 1024);
    app
}

fn output_pixel(world: &World, name: &'static str, x: i32, y: i32) -> Color {
    let entity = world.find_by_id(name).unwrap();
    world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(entity)
        .unwrap()
        .texture()
        .read_pixels()
        .unwrap()
        .get_pixel(x, y)
}

fn output_version(world: &World, name: &'static str) -> u64 {
    let entity = world.find_by_id(name).unwrap();
    world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .state(entity)
        .unwrap()
        .current
        .unwrap()
        .version
}

#[crate::component]
struct MeasurementSample {
    calls: Cell<usize>,
    available: Cell<bool>,
    global: Cell<bool>,
    bounds: Rect,
    workspace: u16,
}

#[crate::view(component = MeasurementSample)]
fn measurement_render(
    sample: component!(),
    samples: query!(MeasurementSample),
    entity: entity!(),
    paint: paint!(),
) {
    if sample.global.get() {
        for _ in samples {}
    } else {
        let _ = samples.get(entity);
    }
    let _ = paint.rect();
}

fn sample_measurement(
    world: &World,
    entity: Entity,
    _rect: Rect,
    _source: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<crate::ui::view::PaintMeasurement> {
    let sample = world.get::<MeasurementSample>(entity)?;
    sample.calls.set(sample.calls.get() + 1);
    sample
        .available
        .get()
        .then(|| crate::ui::view::PaintMeasurement {
            coverage: if sample.bounds.w == Fixed::ZERO {
                crate::ui::dirty::PaintCoverage::EMPTY
            } else {
                crate::ui::dirty::PaintCoverage::solid(sample.bounds)
            },
            workspace: (sample.workspace != 0)
                .then(|| PlaneLayout::packed(sample.workspace, 1, ColorFormat::RGBA8888).unwrap()),
        })
}

#[crate::compose]
fn measured_scene() {
    ui! {
        View (id: "measured", width: 4, height: 4) [
            OffscreenRender::new(),
            MeasurementSample {
                calls: Cell::new(0),
                available: Cell::new(true),
                global: Cell::new(false),
                bounds: Rect::new(0, 0, 6, 4),
                workspace: 8,
            },
        ]
    };
}

fn measured_app() -> (TestApp, Entity) {
    let mut app = app(16, 8);
    app.with_widget(measurement_render::view().with_measurement(sample_measurement));
    let root = app.spawn_root().id();
    app.compose(root, measured_scene());
    let entity = app.world.find_by_id("measured").unwrap();
    (app, entity)
}

fn measure_snapshot(app: &TestApp) {
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let mut plan = outputs.take_texture_plan().unwrap();
    plan.measure(&app.world, &snapshot.layout_tree, &snapshot.entities);
    outputs.put_texture_plan(plan);
}

fn sample_calls(app: &TestApp, entity: Entity) -> usize {
    app.world
        .get::<MeasurementSample>(entity)
        .unwrap()
        .calls
        .get()
}

#[test]
fn stable_measurements_reuse_local_bounds_through_transform_only_changes() {
    let (mut app, entity) = measured_app();
    app.dirty_regions().unwrap();
    let cold = sample_calls(&app, entity);
    measure_snapshot(&app);
    assert!(sample_calls(&app, entity) > cold);
    app.render().unwrap();
    measure_snapshot(&app);
    let warm = sample_calls(&app, entity);
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..4 {
            measure_snapshot(&app);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(sample_calls(&app, entity), warm);

    app.world.insert(
        entity,
        crate::ui::widgets::WidgetTransform(Transform {
            tx: Fixed::from_int(2),
            ..Transform::IDENTITY
        }),
    );
    app.world.invalidate_transform(entity);
    measure_snapshot(&app);
    assert_eq!(sample_calls(&app, entity), warm);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert_eq!(
        plan.measured_output_layout(entity).unwrap().local_bounds.w,
        Fixed::from_int(6)
    );
    outputs.put_texture_plan(plan);
}

#[test]
fn prepared_query_remeasures_new_content_before_same_batch_preparation() {
    let (mut app, entity) = measured_app();
    app.render().unwrap();
    app.dirty_regions().unwrap();
    let calls = sample_calls(&app, entity);
    app.world
        .get_mut::<MeasurementSample>(entity)
        .unwrap()
        .bounds
        .w = Fixed::from_int(9);
    ViewRegistry::reconcile_observations(&mut app.world);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let mut pixels = [0_u8; 16 * 8 * 4];
    let mut renderer = crate::render::sw::SwRenderer::new(
        Texture::new(&mut pixels, 16, 8, ColorFormat::RGBA8888).unwrap(),
    );
    let _batch = outputs.begin_batch();
    let prepared = prepare(
        &app.world,
        &snapshot.layout_tree,
        &snapshot.entities,
        &mut renderer,
    )
    .unwrap()
    .unwrap();
    assert!(sample_calls(&app, entity) > calls);
    assert_eq!(
        prepared
            .plan
            .measured_output_layout(entity)
            .unwrap()
            .local_bounds
            .w,
        Fixed::from_int(9)
    );
}

#[test]
fn completed_batch_recomputes_transformed_descendants_without_remeasuring_them() {
    let (mut app, entity) = measured_app();
    register_counter(&mut app);
    app.world.insert(entity, PaintCounter(Cell::new(0)));
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    app.world.insert(root, OffscreenRender::new());
    app.world.remove::<OffscreenRender>(entity);
    app.render().unwrap();
    measure_snapshot(&app);
    let calls = sample_calls(&app, entity);
    let paints = paint_calls(&app.world, "measured");
    let prepare_width = |app: &TestApp| {
        let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
        let mut pixels = [0_u8; 16 * 8 * 4];
        let mut renderer = crate::render::sw::SwRenderer::new(
            Texture::new(&mut pixels, 16, 8, ColorFormat::RGBA8888).unwrap(),
        );
        let prepared = prepare(
            &app.world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        assert!(prepared.plan.batch_complete);
        prepared
            .plan
            .measured_output_layout(root)
            .unwrap()
            .local_bounds
            .w
    };
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let batch = outputs.batch_id();
    assert_eq!(prepare_width(&app), Fixed::from_int(16));
    assert_eq!(sample_calls(&app, entity), calls);
    app.world.insert(
        entity,
        crate::ui::widgets::WidgetTransform(Transform {
            tx: Fixed::from_int(12),
            ..Transform::IDENTITY
        }),
    );
    app.world.invalidate_transform(entity);
    assert_eq!(prepare_width(&app), Fixed::from_int(18));
    // Exact paint coverage still measures once per paint; plan preparation reuses its result.
    assert_eq!(
        sample_calls(&app, entity) - calls,
        paint_calls(&app.world, "measured") - paints
    );
    assert_eq!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .batch_id(),
        batch
    );
}

#[test]
fn empty_measurements_cache_but_unavailable_and_overflowing_measurements_retry() {
    let (mut app, entity) = measured_app();
    app.world
        .get_mut::<MeasurementSample>(entity)
        .unwrap()
        .bounds = Rect::ZERO;
    app.render().unwrap();
    measure_snapshot(&app);
    let calls = sample_calls(&app, entity);
    measure_snapshot(&app);
    assert_eq!(sample_calls(&app, entity), calls);

    app.world
        .get::<MeasurementSample>(entity)
        .unwrap()
        .available
        .set(false);
    app.world.invalidate_visual(entity);
    measure_snapshot(&app);
    VisualDirty::prepare(&mut app.world, entity);
    let unavailable = sample_calls(&app, entity);
    measure_snapshot(&app);
    assert!(sample_calls(&app, entity) > unavailable);
    app.world
        .get::<MeasurementSample>(entity)
        .unwrap()
        .available
        .set(true);
    measure_snapshot(&app);
    let recovered = sample_calls(&app, entity);
    measure_snapshot(&app);
    assert_eq!(sample_calls(&app, entity), recovered);

    app.world
        .get_mut::<MeasurementSample>(entity)
        .unwrap()
        .bounds = Rect::new(Fixed::MAX, Fixed::ZERO, Fixed::ONE, Fixed::ONE);
    app.world.invalidate_visual(entity);
    measure_snapshot(&app);
    VisualDirty::prepare(&mut app.world, entity);
    let overflowed = sample_calls(&app, entity);
    measure_snapshot(&app);
    assert!(sample_calls(&app, entity) > overflowed);
}

#[test]
fn cached_workspace_maximum_shrinks_and_disappears_with_its_measurement() {
    let (mut app, entity) = measured_app();
    app.render().unwrap();
    measure_snapshot(&app);
    let workspace = |app: &TestApp| {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        let bytes = plan.workspace_bytes;
        outputs.put_texture_plan(plan);
        bytes
    };
    assert_eq!(workspace(&app), 32);
    app.world
        .get_mut::<MeasurementSample>(entity)
        .unwrap()
        .workspace = 2;
    ViewRegistry::reconcile_observations(&mut app.world);
    measure_snapshot(&app);
    assert_eq!(workspace(&app), 8);
    VisualDirty::prepare(&mut app.world, entity);
    measure_snapshot(&app);
    assert_eq!(workspace(&app), 8);
    app.world.remove::<MeasurementSample>(entity);
    ViewRegistry::reconcile_observations(&mut app.world);
    measure_snapshot(&app);
    assert_eq!(workspace(&app), 0);
}

fn empty_sample_measurement(
    world: &World,
    entity: Entity,
    rect: Rect,
    source: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<crate::ui::view::PaintMeasurement> {
    let mut measured = sample_measurement(world, entity, rect, source)?;
    measured.coverage = crate::ui::dirty::PaintCoverage::EMPTY;
    measured.workspace = None;
    Some(measured)
}

#[test]
fn registry_identity_and_observation_scope_bound_measurement_reuse() {
    let (mut app, entity) = measured_app();
    app.render().unwrap();
    measure_snapshot(&app);
    let original = app.world.remove_resource::<ViewRegistry>().unwrap();
    let mut alternate = ViewRegistry::with_builtins();
    alternate.insert(measurement_render::view().with_measurement(empty_sample_measurement));
    app.world.insert_resource(alternate);
    app.render().unwrap();
    measure_snapshot(&app);
    let mut parked = original;
    for expected in [32, 0, 32, 0] {
        let active = app.world.remove_resource::<ViewRegistry>().unwrap();
        app.world.insert_resource(parked);
        parked = active;
        ViewRegistry::reconcile_observations(&mut app.world);
        let calls = sample_calls(&app, entity);
        measure_snapshot(&app);
        assert!(sample_calls(&app, entity) > calls);
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        assert_eq!(plan.workspace_bytes, expected);
        outputs.put_texture_plan(plan);
        VisualDirty::prepare(&mut app.world, entity);
    }

    app.render().unwrap();
    measure_snapshot(&app);
    let relations = app.world.remove_resource::<OutputRelations>().unwrap();
    let calls = sample_calls(&app, entity);
    measure_snapshot(&app);
    measure_snapshot(&app);
    assert_eq!(sample_calls(&app, entity), calls + 2);
    app.world.insert_resource(relations);

    app.world
        .get::<MeasurementSample>(entity)
        .unwrap()
        .global
        .set(true);
    app.world.invalidate_visual(entity);
    app.render().unwrap();
    let calls = sample_calls(&app, entity);
    measure_snapshot(&app);
    measure_snapshot(&app);
    assert_eq!(sample_calls(&app, entity), calls + 2);

    app.world.remove_resource::<ViewRegistry>();
    measure_snapshot(&app);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert!(
        plan.nodes
            .iter()
            .all(|node| !node.measurement_valid && node.extra.is_none())
    );
    assert_eq!(plan.workspace_bytes, 0);
    outputs.put_texture_plan(plan);
}

fn overflowing_measurement(
    _world: &World,
    _entity: Entity,
    _rect: Rect,
    _source: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<crate::ui::view::PaintMeasurement> {
    Some(crate::ui::view::PaintMeasurement {
        coverage: crate::ui::dirty::PaintCoverage::solid(Rect::new(Fixed::MAX, 0, 1, 1)),
        workspace: None,
    })
}

#[crate::view(component = MirrorOf, priority = 99)]
fn failed_bounds_render(paint: paint!()) {
    let _ = paint.rect();
}

#[test]
fn measurement_failure_retains_successful_workspace_for_inline_fallback() {
    let mut app = app(8, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, inline_dependency_inside_capture());
    app.render().unwrap();
    app.with_widget(failed_bounds_render::view().with_measurement(overflowing_measurement));
    ViewRegistry::reconcile_observations(&mut app.world);
    let mirror = app.world.find_by_id("inline_mirror").unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let mut pixels = [0_u8; 8 * 4 * 4];
    {
        let mut renderer = crate::render::sw::SwRenderer::new(
            Texture::new(&mut pixels, 8, 4, ColorFormat::RGBA8888).unwrap(),
        );
        let prepared = prepare(
            &app.world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        let rect = app.world.get::<crate::ui::ComputedRect>(mirror).unwrap().0;
        let style = app.world.get::<Style>(mirror).unwrap();
        let mut paint = crate::ui::view::PaintCtx::new(
            &mut renderer,
            style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            crate::ui::theme::WidgetState::Enabled,
        );
        paint.set_workspace(&prepared.plan.workspace);
        crate::ui::widgets::mirror::view().paint(&app.world, mirror, &mut paint);
        assert!(paint.error().is_none());
    }
    let plan = outputs.take_texture_plan().unwrap();
    let index = snapshot
        .entities
        .iter()
        .position(|&entity| entity == mirror)
        .unwrap();
    assert_eq!(plan.nodes[index].workspace_bytes, 4 * 4 * 4);
    assert_eq!(plan.workspace_bytes, 4 * 4 * 4);
    assert!(!plan.nodes[index].measurement_valid);
    outputs.put_texture_plan(plan);
    assert_eq!(&pixels[(8 + 1) * 4..(8 + 2) * 4], &[31, 173, 91, 255]);
}

#[crate::compose]
fn reversed_mirror_chain() {
    ui! {
        Row (width: 12, height: 4) {
            View (id: "chain_last", width: 4, height: 4) [
                OffscreenRender::new(),
                MirrorOf::new(id("chain_middle")),
            ]
            View (id: "chain_middle", width: 4, height: 4) [
                MirrorOf::new(id("chain_source")),
            ]
            Column (id: "chain_source", width: 4, height: 4) {
                View (width: 4, height: 2, bg_color: Color::rgb(240, 20, 40))
                View (width: 4, height: 2, bg_color: Color::rgb(20, 40, 240))
            }
        }
    };
}

#[test]
fn reverse_tree_order_prepares_a_complete_mirror_chain_in_the_first_batch() {
    let mut app = app(12, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, reversed_mirror_chain());
    app.render().unwrap();

    let red = Color::rgb(240, 20, 40);
    let blue = Color::rgb(20, 40, 240);
    assert_eq!(output_pixel(&app.world, "chain_source", 1, 0), red);
    assert_eq!(output_pixel(&app.world, "chain_middle", 1, 0), blue);
    assert_eq!(output_pixel(&app.world, "chain_last", 1, 0), red);
    assert_eq!(output_pixel(&app.world, "chain_last", 1, 3), blue);
    assert!(
        output_version(&app.world, "chain_source") < output_version(&app.world, "chain_middle")
    );
    assert!(output_version(&app.world, "chain_middle") < output_version(&app.world, "chain_last"));
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 0), red);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 3), blue);
}

#[crate::compose]
fn inline_dependency_inside_capture() {
    ui! {
        Row (width: 8, height: 4) {
            View (id: "capture", width: 4, height: 4) [
                OffscreenRender::new(),
            ] {
                View (id: "inline_mirror", width: 4, height: 4) [
                    MirrorOf::new(id("external_source")),
                ]
            }
            View (
                id: "external_source",
                width: 4,
                height: 4,
                bg_color: Color::rgb(31, 173, 91)
            )
        }
    };
}

#[test]
fn repeated_queries_do_not_republish_or_reinvalidate_prepared_textures() {
    let mut app = app(8, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, inline_dependency_inside_capture());
    app.render().unwrap();
    let capture = app.world.find_by_id("capture").unwrap();
    let source = app.world.find_by_id("external_source").unwrap();
    let published = output_version(&app.world, "capture");
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    for color in [Color::rgb(220, 40, 80), Color::rgb(40, 80, 220)] {
        app.world.get_mut::<Style>(source).unwrap().bg_color = Some(color.into());
        app.dirty_regions().unwrap();
        let generation = app
            .world
            .get::<crate::ui::OffscreenGeneration>(capture)
            .unwrap()
            .0;
        for _ in 0..3 {
            assert!(!app.dirty_regions().unwrap().is_empty());
            assert_eq!(
                app.world
                    .get::<crate::ui::OffscreenGeneration>(capture)
                    .unwrap()
                    .0,
                generation
            );
            assert_eq!(output_version(&app.world, "capture"), published);
            assert_eq!(
                app.backend.framebuffer().read_pixels().unwrap().bytes(),
                pixels
            );
        }
    }
    app.render_dirty().unwrap();
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        Color::rgb(40, 80, 220)
    );
    assert!(app.dirty_regions().unwrap().is_empty());
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        dirty
    );
}

#[test]
fn capture_inherits_its_inline_consumers_external_dependency() {
    let mut app = app(8, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, inline_dependency_inside_capture());
    app.render().unwrap();

    let inline = app.world.find_by_id("inline_mirror").unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(inline)
            .is_none()
    );
    assert_eq!(
        output_pixel(&app.world, "capture", 1, 1),
        Color::rgb(31, 173, 91)
    );
    assert!(output_version(&app.world, "external_source") < output_version(&app.world, "capture"));
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        Color::rgb(31, 173, 91)
    );
}

#[test]
fn nested_capture_orders_external_source_before_inner_and_outer_outputs() {
    let mut app = app(8, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, inline_dependency_inside_capture());
    let inner = app.world.find_by_id("inline_mirror").unwrap();
    app.world.insert(inner, OffscreenRender::new());
    app.render().unwrap();

    let green = Color::rgb(31, 173, 91);
    assert_eq!(output_pixel(&app.world, "inline_mirror", 1, 1), green);
    assert_eq!(output_pixel(&app.world, "capture", 1, 1), green);
    assert!(
        output_version(&app.world, "external_source") < output_version(&app.world, "inline_mirror")
    );
    assert!(output_version(&app.world, "inline_mirror") < output_version(&app.world, "capture"));
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        green
    );
}

#[crate::compose]
fn nested_captures() {
    ui! {
        View (id: "outer", width: 4, height: 4) [
            OffscreenRender::new(),
        ] {
            View (
                id: "inner",
                width: 4,
                height: 4,
                bg_color: Color::rgb(220, 30, 50)
            ) [
                OffscreenRender::new(),
            ]
        }
    };
}

#[test]
fn dirty_nested_output_refreshes_inner_and_outer_in_the_same_batch() {
    let mut app = app(4, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, nested_captures());
    app.render().unwrap();
    let inner_version = output_version(&app.world, "inner");
    let outer_version = output_version(&app.world, "outer");
    let inner = app.world.find_by_id("inner").unwrap();
    let blue = Color::rgb(30, 50, 220);
    app.world.get_mut::<Style>(inner).unwrap().bg_color = Some(blue.into());
    app.world.invalidate_visual(inner);
    app.render_dirty().unwrap();

    assert!(output_version(&app.world, "inner") > inner_version);
    assert!(output_version(&app.world, "outer") > outer_version);
    assert!(output_version(&app.world, "inner") < output_version(&app.world, "outer"));
    assert_eq!(output_pixel(&app.world, "inner", 1, 1), blue);
    assert_eq!(output_pixel(&app.world, "outer", 1, 1), blue);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        blue
    );
}

#[crate::component]
struct PaintCounter(Cell<usize>);

fn count_paint(world: &World, entity: Entity, _: &mut PaintCtx<'_>) {
    let calls = &world.get::<PaintCounter>(entity).unwrap().0;
    calls.set(calls.get() + 1);
}

fn register_counter(app: &mut TestApp) {
    app.with_widget(
        crate::ui::View::new("OutputPlanCounter", 100, count_paint).with_filter::<PaintCounter>(),
    );
}

fn paint_calls(world: &World, name: &'static str) -> usize {
    world
        .get::<PaintCounter>(world.find_by_id(name).unwrap())
        .unwrap()
        .0
        .get()
}

#[crate::compose]
fn cyclic_captures() {
    ui! {
        Row (width: 12, height: 4) {
            View (id: "cycle_a", width: 4, height: 4) [
                MirrorOf::new(id("cycle_b")),
                PaintCounter(Cell::new(0)),
            ]
            View (id: "cycle_b", width: 4, height: 4) [
                MirrorOf::new(id("cycle_a")),
                PaintCounter(Cell::new(0)),
            ]
            View (
                id: "independent",
                width: 4,
                height: 4,
                bg_color: Color::rgb(17, 191, 67)
            ) [
                OffscreenRender::new(),
                PaintCounter(Cell::new(0)),
            ]
        }
    };
}

#[test]
fn current_cycle_runs_no_affected_views_and_preserves_independent_outputs() {
    let mut app = app(12, 4);
    register_counter(&mut app);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, cyclic_captures());
    app.render().unwrap();

    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    for name in ["cycle_a", "cycle_b"] {
        assert!(outputs.get(app.world.find_by_id(name).unwrap()).is_none());
        assert_eq!(paint_calls(&app.world, name), 0);
    }
    assert_eq!(paint_calls(&app.world, "independent"), 1);
    assert_eq!(
        output_pixel(&app.world, "independent", 1, 1),
        Color::rgb(17, 191, 67)
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(9, 1),
        Color::rgb(17, 191, 67)
    );
}

#[crate::compose]
fn missing_inline_source(source: Entity) {
    ui!(View(width: 1, height: 1) [MirrorOf::new(source)]);
}

#[test]
fn missing_external_source_does_not_enable_a_cyclic_capture() {
    let mut app = app(12, 4);
    register_counter(&mut app);
    let root = app.spawn_root().id();
    app.compose(root, cyclic_captures());
    let detached = app.world.spawn(Widget);
    app.world.insert(detached, Style::default());
    let cycle = app.world.find_by_id("cycle_a").unwrap();
    app.compose(cycle, missing_inline_source(detached));

    for _ in 0..2 {
        app.render().unwrap();
        assert_eq!(paint_calls(&app.world, "cycle_a"), 0);
        assert_eq!(paint_calls(&app.world, "cycle_b"), 0);
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let plan = outputs.take_texture_plan().unwrap();
        assert!(plan.blocked(cycle));
        outputs.put_texture_plan(plan);
    }
}

#[test]
fn relationship_topology_ignores_publications_and_reconciles_registry_lifetimes() {
    let mut app = app(12, 4);
    let root = app.spawn_root().id();
    app.compose(root, reversed_mirror_chain());
    app.render().unwrap();
    let source = app.world.find_by_id("chain_source").unwrap();
    let middle = app.world.find_by_id("chain_middle").unwrap();
    let last = app.world.find_by_id("chain_last").unwrap();
    let revision = |world: &World| {
        world
            .resource::<OutputRelations>()
            .unwrap()
            .structure_revision()
    };
    let stable = revision(&app.world);
    app.world.get_mut::<Style>(source).unwrap().bg_color = Some(Color::rgb(12, 34, 56).into());
    app.render().unwrap();
    assert_eq!(revision(&app.world), stable);

    app.world.insert(source, crate::ui::Hidden);
    app.render().unwrap();
    assert!(revision(&app.world) > stable);
    app.world.remove::<crate::ui::Hidden>(source);
    app.render().unwrap();
    let visible = revision(&app.world);
    app.world.insert(middle, MirrorOf::new(last));
    app.render().unwrap();
    assert!(revision(&app.world) > visible);
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(last)
            .is_none()
    );
    app.world.insert(middle, MirrorOf::new(source));
    app.render().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(last)
            .is_some()
    );

    let registered = revision(&app.world);
    let registry = app.world.remove_resource::<ViewRegistry>().unwrap();
    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(revision(&app.world) > registered);
    app.render().unwrap();
    app.world.insert_resource(registry);
    ViewRegistry::reconcile_observations(&mut app.world);
    app.render().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(last)
            .is_some()
    );
}

#[crate::component]
struct RetryPaint {
    fail: Cell<bool>,
    calls: Cell<usize>,
}

#[crate::view(component = RetryPaint)]
fn retry_paint(sample: component!(), paint: paint!()) {
    sample.calls.set(sample.calls.get() + 1);
    if sample.fail.get() {
        paint.fail(RenderError::BackendFailure);
    }
}

#[crate::compose]
fn retry_captures() {
    ui! {
        Row (width: 8, height: 4) {
            View (id: "failed", width: 4, height: 4) [
                OffscreenRender::new(),
                RetryPaint {
                    fail: Cell::new(true),
                    calls: Cell::new(0),
                },
            ]
            View (id: "healthy", width: 4, height: 4) [
                OffscreenRender::new(),
                RetryPaint {
                    fail: Cell::new(false),
                    calls: Cell::new(0),
                },
            ]
        }
    };
}

#[test]
fn failed_preparation_retries_in_the_same_batch_without_replaying_successes() {
    let mut app = app(8, 4);
    app.with_widget(retry_paint::view());
    let root = app.spawn_root().id();
    app.compose(root, retry_captures());
    app.dirty_regions().unwrap();
    let failed = app.world.find_by_id("failed").unwrap();
    let healthy = app.world.find_by_id("healthy").unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let mut pixels = [0_u8; 8 * 4 * 4];
    let mut renderer = crate::render::sw::SwRenderer::new(
        Texture::new(&mut pixels, 8, 4, ColorFormat::RGBA8888).unwrap(),
    );
    let _batch = outputs.begin_batch();
    assert!(matches!(
        prepare(
            &app.world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &mut renderer
        ),
        Err(RenderError::BackendFailure)
    ));
    let plan = outputs.take_texture_plan().unwrap();
    assert!(!plan.batch_complete);
    outputs.put_texture_plan(plan);
    let healthy_version = output_version(&app.world, "healthy");
    app.world.get::<RetryPaint>(failed).unwrap().fail.set(false);

    for _ in 0..2 {
        let prepared = prepare(
            &app.world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &mut renderer,
        )
        .unwrap()
        .unwrap();
        assert!(prepared.plan.batch_complete);
        assert!(prepared.plan.ready(failed));
        assert!(prepared.plan.ready(healthy));
        assert_eq!(output_version(&app.world, "healthy"), healthy_version);
    }
    assert_eq!(app.world.get::<RetryPaint>(failed).unwrap().calls.get(), 2);
    assert_eq!(app.world.get::<RetryPaint>(healthy).unwrap().calls.get(), 1);
}

#[crate::compose]
fn ancestor_cycle() {
    ui! {
        Row (width: 8, height: 4) {
            View (id: "ancestor", width: 4, height: 4) [
                PaintCounter(Cell::new(0)),
            ] {
                View (id: "descendant", width: 4, height: 4) [
                    MirrorOf::new(id("ancestor")),
                    PaintCounter(Cell::new(0)),
                ]
            }
            View (
                id: "independent",
                width: 4,
                height: 4,
                bg_color: Color::rgb(17, 191, 67)
            ) [
                OffscreenRender::new(),
                PaintCounter(Cell::new(0)),
            ]
        }
    };
}

#[test]
fn reading_an_ancestor_capture_is_a_cycle_before_any_subtree_view_runs() {
    let mut app = app(8, 4);
    register_counter(&mut app);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, ancestor_cycle());
    app.render().unwrap();

    assert_eq!(paint_calls(&app.world, "ancestor"), 0);
    assert_eq!(paint_calls(&app.world, "descendant"), 0);
    assert_eq!(paint_calls(&app.world, "independent"), 1);
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(app.world.find_by_id("ancestor").unwrap())
            .is_none()
    );
}

#[crate::compose]
fn transformed_nested_captures() {
    ui! {
        View (
            id: "scaled_outer",
            width: 12,
            height: 12,
            bg_color: Color::rgb(0, 0, 0)
        ) [
            OffscreenRender::with_scale(Fixed::HALF).with_opacity(128),
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(2),
            },
        ] {
            View (
                id: "scaled_inner",
                position: Position::Absolute,
                left: 4,
                top: 4,
                width: 4,
                height: 4,
                bg_color: Color::rgb(240, 40, 20)
            ) [
                OffscreenRender::with_scale(Fixed::HALF).with_opacity(128),
            ]
        }
    };
}

fn assert_rgb_close(actual: Color, expected: Color) {
    for (actual, expected) in [actual.r, actual.g, actual.b]
        .into_iter()
        .zip([expected.r, expected.g, expected.b])
    {
        assert!(actual.abs_diff(expected) <= 2, "{actual} != {expected}");
    }
}

#[test]
fn nested_scale_scroll_and_opacity_apply_once_at_each_composition_boundary() {
    let mut app = app(12, 12);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, transformed_nested_captures());
    app.render().unwrap();

    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let inner = outputs
        .get(app.world.find_by_id("scaled_inner").unwrap())
        .unwrap();
    let outer = outputs
        .get(app.world.find_by_id("scaled_outer").unwrap())
        .unwrap();
    assert_eq!((inner.texture().width, inner.texture().height), (2, 2));
    assert_eq!((outer.texture().width, outer.texture().height), (6, 6));
    assert_eq!(
        inner.texture().read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(240, 40, 20)
    );
    assert_rgb_close(
        outer.texture().read_pixels().unwrap().get_pixel(3, 2),
        Color::rgb(120, 20, 10),
    );
    let pixels = app.backend.framebuffer();
    assert_rgb_close(
        pixels.read_pixels().unwrap().get_pixel(5, 3),
        Color::rgb(60, 10, 5),
    );
    assert_eq!(
        pixels.read_pixels().unwrap().get_pixel(5, 7),
        Color::rgb(0, 0, 0)
    );
    assert_eq!(
        pixels.read_pixels().unwrap().get_pixel(1, 3),
        Color::rgb(0, 0, 0)
    );
}

#[cfg(feature = "std")]
#[test]
fn warm_output_preparation_reuses_the_plan_and_pixel_slots_without_allocating() {
    let mut app = app(12, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, reversed_mirror_chain());
    app.render().unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let mut pixels = [0_u8; 12 * 4 * 4];
    let mut renderer = crate::render::sw::SwRenderer::new(
        Texture::new(&mut pixels, 12, 4, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );

    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..16 {
            let _batch = outputs.begin_batch();
            let prepared = prepare(
                &app.world,
                &snapshot.layout_tree,
                &snapshot.entities,
                &mut renderer,
            )
            .unwrap()
            .unwrap();
            assert!(
                prepared
                    .plan
                    .captures
                    .iter()
                    .all(|capture| capture.state == CaptureState::Ready)
            );
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn a_successful_chain_is_idle_immediately_after_the_completed_batch() {
    let mut app = app(12, 4);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, reversed_mirror_chain());
    app.render().unwrap();
    let versions =
        ["chain_source", "chain_middle", "chain_last"].map(|name| output_version(&app.world, name));

    for _ in 0..2 {
        app.render_dirty().unwrap();
        assert!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .is_empty()
        );
        assert_eq!(
            ["chain_source", "chain_middle", "chain_last"]
                .map(|name| output_version(&app.world, name)),
            versions
        );
    }
}

#[crate::compose]
fn retired_root_source() {
    ui! {
        View (
            id: "retired_source",
            width: 4,
            height: 4,
            bg_color: Color::rgb(227, 41, 53)
        ) [
            OffscreenRender::new(),
        ]
    };
}

#[crate::compose]
fn active_root_consumer(source: Entity) {
    ui! {
        View (width: 4, height: 4) [
            MirrorOf::new(source),
        ]
    };
}

#[test]
fn switching_roots_rejects_a_live_source_without_current_layout() {
    let mut app = app(4, 4);
    let background = Color::rgb(0, 0, 0);
    let retired_root = app.spawn_root().bg_color(background).id();
    app.compose(retired_root, retired_root_source());
    let source = app.world.find_by_id("retired_source").unwrap();
    let active_root = app.spawn_root().bg_color(background).id();
    app.compose(active_root, active_root_consumer(source));

    app.set_root(retired_root);
    app.render().unwrap();
    assert_eq!(
        output_pixel(&app.world, "retired_source", 1, 1),
        Color::rgb(227, 41, 53)
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        Color::rgb(227, 41, 53)
    );

    app.set_root(active_root);
    app.render().unwrap();
    assert!(app.world.is_alive(source));
    assert!(app.world.has::<OffscreenRender>(source));
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .is_none()
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        background
    );
}

#[crate::compose]
fn resizing_source() {
    ui! {
        View (
            id: "source_container",
            position: Position::Absolute,
            left: 4,
            top: 0,
            width: 4,
            height: 4
        ) {
            View (
                id: "resizing_source",
                width: Dimension::percent(100),
                height: 4,
                bg_color: Color::rgb(23, 47, 229)
            ) {
                View (width: 4, height: 4, bg_color: Color::rgb(229, 47, 23))
            }
        }
    };
}

#[crate::compose]
fn fixed_size_consumer(source: Entity) {
    ui! {
        View (
            id: "fixed_consumer",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 4,
            height: 4
        ) [
            OffscreenRender::new(),
            MirrorOf::new(source),
        ]
    };
}

#[test]
fn source_layout_resize_rebuilds_a_fixed_size_cached_consumer_in_the_same_batch() {
    for dirty in [false, true] {
        let mut app = app(12, 4);
        let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
        app.compose(root, resizing_source());
        let source = app.world.find_by_id("resizing_source").unwrap();
        app.compose(root, fixed_size_consumer(source));
        app.render().unwrap();
        let source_version = output_version(&app.world, "resizing_source");
        let consumer_version = output_version(&app.world, "fixed_consumer");
        assert_eq!(
            output_pixel(&app.world, "fixed_consumer", 3, 1),
            Color::rgb(229, 47, 23)
        );

        // Only the container changes; neither source nor consumer is manually dirtied.
        let container = app.world.find_by_id("source_container").unwrap();
        app.world.get_mut::<Style>(container).unwrap().layout.width = Dimension::px(8);
        if dirty {
            app.render_dirty().unwrap();
        } else {
            app.render().unwrap();
        }

        let source = app.world.find_by_id("resizing_source").unwrap();
        let consumer = app.world.find_by_id("fixed_consumer").unwrap();
        assert_eq!(
            app.world
                .get::<crate::ui::ComputedRect>(source)
                .unwrap()
                .0
                .w,
            Fixed::from_int(8)
        );
        assert_eq!(
            app.world
                .get::<crate::ui::ComputedRect>(consumer)
                .unwrap()
                .0
                .w,
            Fixed::from_int(4)
        );
        assert!(
            output_version(&app.world, "resizing_source") > source_version,
            "dirty={dirty}"
        );
        assert!(
            output_version(&app.world, "fixed_consumer") > consumer_version,
            "dirty={dirty}"
        );
        assert!(
            output_version(&app.world, "resizing_source")
                < output_version(&app.world, "fixed_consumer")
        );
        assert_eq!(
            output_pixel(&app.world, "fixed_consumer", 3, 1),
            Color::rgb(23, 47, 229),
            "dirty={dirty}"
        );
        assert_eq!(
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .get_pixel(3, 1),
            Color::rgb(23, 47, 229),
            "dirty={dirty}"
        );

        let completed_version = output_version(&app.world, "fixed_consumer");
        app.render_dirty().unwrap();
        assert_eq!(
            output_version(&app.world, "fixed_consumer"),
            completed_version
        );
    }
}

#[crate::component]
#[retain_previous(WidgetTexture)]
#[derive(Default)]
struct RetainedBatchProbe {
    calls: Cell<usize>,
    previous: Cell<Option<(Color, u64)>>,
}

#[crate::view(component = RetainedBatchProbe)]
fn retained_batch_probe(
    probe: component!(),
    owner: entity!(),
    textures: res!(Outputs<WidgetTexture>),
    _paint: paint!(),
) {
    probe.calls.set(probe.calls.get() + 1);
    probe.previous.set(textures.previous(owner).map(|output| {
        let texture = output.texture();
        (
            texture.read_pixels().unwrap().get_pixel(1, 1),
            texture.cache_revision,
        )
    }));
}

#[crate::component]
struct SplitHistoryProbe {
    source: Entity,
    calls: Cell<usize>,
    previous: Cell<[Option<(Color, u64)>; 2]>,
    current: Cell<[Option<(Color, u64)>; 2]>,
    clips: Cell<[Rect; 2]>,
}

#[crate::view(component = SplitHistoryProbe)]
fn split_history_probe(
    probe: component!(),
    textures: res!(Outputs<WidgetTexture>),
    paint: paint!(),
) {
    let index = probe.calls.get();
    let mut previous = probe.previous.get();
    let mut current = probe.current.get();
    let mut clips = probe.clips.get();
    previous[index] = textures.previous(probe.source).map(|output| {
        let texture = output.texture();
        (
            texture.read_pixels().unwrap().get_pixel(1, 1),
            texture.cache_revision,
        )
    });
    current[index] = textures.get(probe.source).map(|output| {
        let texture = output.texture();
        (
            texture.read_pixels().unwrap().get_pixel(1, 1),
            texture.cache_revision,
        )
    });
    clips[index] = paint.clip();
    probe.previous.set(previous);
    probe.current.set(current);
    probe.clips.set(clips);
    probe.calls.set(index + 1);
}

#[crate::compose]
fn separated_batch_damage() {
    ui! {
        View (width: 64, height: 4) {
            View (
                id: "batch_source",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: Color::rgb(229, 47, 23)
            ) [
                RetainedBatchProbe::default(),
            ]
            View (
                id: "batch_other",
                position: Position::Absolute,
                left: 60,
                top: 0,
                width: 4,
                height: 4,
                bg_color: Color::rgb(23, 47, 229)
            )
        }
    };
}

#[test]
fn app_split_dirty_regions_share_one_publication_and_frozen_history() {
    let mut app = app(64, 4);
    app.with_widget(retained_batch_probe::view())
        .with_widget(split_history_probe::view());
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, separated_batch_damage());
    let source = app.world.find_by_id("batch_source").unwrap();
    let other = app.world.find_by_id("batch_other").unwrap();
    // History reads do not register current dependencies or enlarge damage.
    app.world.insert(
        root,
        SplitHistoryProbe {
            source,
            calls: Cell::new(0),
            previous: Cell::new([None; 2]),
            current: Cell::new([None; 2]),
            clips: Cell::new([Rect::ZERO; 2]),
        },
    );
    app.render().unwrap();
    let first_version = output_version(&app.world, "batch_source");
    {
        let producer = app.world.get::<RetainedBatchProbe>(source).unwrap();
        assert_eq!(producer.calls.get(), 1);
        assert!(producer.previous.get().is_none());
        producer.calls.set(0);
        let reader = app.world.get::<SplitHistoryProbe>(root).unwrap();
        assert_eq!(reader.calls.get(), 1);
        assert_eq!(reader.previous.get(), [None; 2]);
        reader.calls.set(0);
        reader.current.set([None; 2]);
        reader.clips.set([Rect::ZERO; 2]);
    }

    let next = Color::rgb(23, 47, 229);
    let other_next = Color::rgb(29, 191, 71);
    app.world.get_mut::<Style>(source).unwrap().bg_color = Some(next.into());
    app.world.get_mut::<Style>(other).unwrap().bg_color = Some(other_next.into());
    app.render_dirty().unwrap();

    let second_version = output_version(&app.world, "batch_source");
    assert_eq!(second_version, first_version + 1);
    {
        let plan = &app.world.resource::<LastDirtyRegions>().unwrap().0;
        assert_eq!(plan.rects.len(), 2);
        assert!(plan.shifts.is_empty());
        assert!(plan.prefers_split_redraw());
        assert!(plan.rects[0].intersect(&plan.rects[1]).is_none());
        let expected_previous = Some((Color::rgb(229, 47, 23), first_version));
        let producer = app.world.get::<RetainedBatchProbe>(source).unwrap();
        assert_eq!(producer.calls.get(), 1);
        assert_eq!(producer.previous.get(), expected_previous);
        let reader = app.world.get::<SplitHistoryProbe>(root).unwrap();
        assert_eq!(reader.calls.get(), 2);
        assert_eq!(reader.clips.get(), [plan.rects[0], plan.rects[1]]);
        assert_eq!(reader.previous.get(), [expected_previous; 2]);
        assert_eq!(reader.current.get(), [Some((next, second_version)); 2]);
    }
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 1), next);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(61, 1), other_next);

    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
    assert_eq!(output_version(&app.world, "batch_source"), second_version);
    assert_eq!(
        app.world
            .get::<RetainedBatchProbe>(source)
            .unwrap()
            .calls
            .get(),
        1
    );
    assert_eq!(
        app.world
            .get::<SplitHistoryProbe>(root)
            .unwrap()
            .calls
            .get(),
        2
    );
}

#[test]
fn split_region_failure_returns_output_workspace_for_retry() {
    let mut app = app(64, 4);
    app.with_widget(retained_batch_probe::view())
        .with_widget(retry_paint::view());
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, separated_batch_damage());
    let source = app.world.find_by_id("batch_source").unwrap();
    let other = app.world.find_by_id("batch_other").unwrap();
    app.world.insert(
        other,
        RetryPaint {
            fail: Cell::new(false),
            calls: Cell::new(0),
        },
    );
    app.render().unwrap();
    app.world.get::<RetryPaint>(other).unwrap().fail.set(true);
    let blue = Color::rgb(23, 47, 229);
    let green = Color::rgb(29, 191, 71);
    app.world.get_mut::<Style>(source).unwrap().bg_color = Some(blue.into());
    app.world.get_mut::<Style>(other).unwrap().bg_color = Some(green.into());
    assert_eq!(app.render_dirty(), Err(RenderError::BackendFailure));
    let dirty = &app.world.resource::<LastDirtyRegions>().unwrap().0;
    assert_eq!(dirty.rects.len(), 2);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let plan = outputs
        .take_texture_plan()
        .expect("failed batch returns its plan");
    outputs.put_texture_plan(plan);
    app.world.get::<RetryPaint>(other).unwrap().fail.set(false);
    app.render_dirty().unwrap();
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(1, 1), blue);
    assert_eq!(pixels.read_pixels().unwrap().get_pixel(61, 1), green);
    let expected = pixels.read_pixels().unwrap().bytes().to_vec();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        expected
    );
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[crate::compose]
fn clipped_inline_consumer() {
    ui! {
        Row (width: 8, height: 4) {
            View (
                id: "region_source",
                width: 4,
                height: 4,
                bg_color: Color::rgb(229, 47, 23)
            )
            View (id: "region_consumer", width: 4, height: 4) [
                MirrorOf::new(id("region_source")),
                PaintCounter(Cell::new(0)),
            ]
        }
    };
}

#[test]
fn public_region_render_preserves_unpainted_inline_consumer_invalidation() {
    use crate::ui::dirty::VisualDirty;

    let mut app = app(8, 4);
    register_counter(&mut app);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, clipped_inline_consumer());
    app.render().unwrap();
    let source = app.world.find_by_id("region_source").unwrap();
    let consumer = app.world.find_by_id("region_consumer").unwrap();
    let source_version = output_version(&app.world, "region_source");
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(consumer)
            .is_none()
    );
    assert_eq!(paint_calls(&app.world, "region_consumer"), 1);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(5, 1),
        Color::rgb(229, 47, 23)
    );

    let next = Color::rgb(23, 47, 229);
    app.world.get_mut::<Style>(source).unwrap().bg_color = Some(next.into());
    let viewport = Viewport::new(8, 4, Fixed::ONE);
    let source_clip = Rect::new(0, 0, 4, 4);
    let consumer_clip = Rect::new(4, 0, 4, 4);
    let damage = super::super::collect_dirty_regions(&mut app.world, root, &viewport).unwrap();
    assert!(
        damage
            .rects
            .iter()
            .any(|rect| rect.intersect(&consumer_clip).is_some())
    );
    assert!(app.world.has::<VisualDirty>(consumer));
    {
        let mut renderer = crate::render::sw::SwRenderer::new(app.backend.framebuffer());
        super::super::render_region(&app.world, root, &viewport, &source_clip, &mut renderer)
            .unwrap();
    }
    assert!(output_version(&app.world, "region_source") > source_version);
    assert_eq!(paint_calls(&app.world, "region_consumer"), 1);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        next
    );
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(5, 1),
        Color::rgb(229, 47, 23)
    );

    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(app.world.has::<VisualDirty>(consumer));
    app.render().unwrap();
    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(!app.world.has::<VisualDirty>(consumer));
    assert_eq!(paint_calls(&app.world, "region_consumer"), 2);
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .get_pixel(5, 1),
        next
    );

    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
    assert_eq!(paint_calls(&app.world, "region_consumer"), 2);
}
