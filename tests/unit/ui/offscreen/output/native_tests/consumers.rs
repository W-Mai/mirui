use super::*;
use crate::prelude::*;
use crate::ui::widgets::TemporalMix;

#[crate::component]
struct Producer {
    color: Color,
    calls: Cell<usize>,
}

#[crate::view(component = Producer)]
fn producer(producer: component!(), paint: paint!()) {
    producer.calls.set(producer.calls.get() + 1);
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color: producer.color,
        radius: Fixed::ZERO,
        opa: 255,
    });
    paint.background_handled();
}

#[crate::component]
struct Preview {
    #[depends_on(WidgetTexture)]
    source: Entity,
}

#[crate::view(component = Preview)]
fn preview(preview: component!(), outputs: res!(Outputs<WidgetTexture>), paint: paint!()) {
    let source = outputs.get(preview.source).unwrap();
    let rect = paint.rect();
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: paint.quad(),
        texture: source.texture(),
        opa: 255,
        radius: Fixed::ZERO,
        composite: crate::render::command::CompositeMode::SourceOver,
    });
    paint.background_handled();
}

#[crate::compose]
fn panel() -> (Entity, Entity, Entity, Entity) {
    let source = ui! {
        View (width: 4, height: 4) [
            Producer {
                color: Color::rgb(200, 0, 0),
                calls: Cell::new(0),
            },
        ]
    };
    let first = ui! {
        View (width: 4, height: 4) [
            Preview { source },
        ]
    };
    let second = ui! {
        View (width: 4, height: 4) [
            Preview { source: first },
        ]
    };
    let temporal = ui! {
        View (width: 4, height: 4)
    };
    (source, first, second, temporal)
}

#[test]
fn consumer_native_priority_preserves_every_mandatory_source_and_single_execution() {
    for (budget, native_consumer) in [(native_bytes() + 128, true), (native_bytes(), false)] {
        let mut app = App::headless(16, 16);
        app.with_default_widgets()
            .with_widget(producer::view())
            .with_widget(preview::view())
            .with_offscreen_pool_budget(budget);
        let root = app.spawn_root().id();
        let (source, first, second, _) = app.compose(root, panel());
        app.world.insert(second, crate::ui::OffscreenRender::new());
        let viewport = Viewport::new(16, 16, Fixed::ONE);
        let mut renderer = Fixture::default();
        app.dirty_regions().unwrap();
        crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(!outputs.get(source).unwrap().texture().is_native());
        assert!(!outputs.get(first).unwrap().texture().is_native());
        assert_eq!(
            outputs.get(second).unwrap().texture().is_native(),
            native_consumer
        );
        assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 1);
        assert_eq!(renderer.device.reads.get(), 0);
        assert_eq!(renderer.device.creates.get(), usize::from(native_consumer));
        assert_eq!(
            outputs
                .get(second)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(0, 0),
            Color::rgb(200, 0, 0)
        );
        for _ in 0..3 {
            app.world.invalidate_visual(source);
            app.dirty_regions().unwrap();
            assert_eq!(
                tracked_allocations(|| {
                    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer)
                        .unwrap();
                }),
                0
            );
        }
    }
}

#[crate::compose]
fn differently_sized_dependency() -> (Entity, Entity) {
    let source = ui! {
        View (width: 4, height: 4) [Producer { color: Color::rgb(20, 80, 160), calls: Cell::new(0) }]
    };
    let consumer = ui! {
        View (width: 8, height: 4) [Preview { source }, crate::ui::OffscreenRender::new()]
    };
    (source, consumer)
}

#[test]
fn native_support_is_queried_for_each_real_layout_without_a_capability_probe() {
    let mut app = App::headless(16, 16);
    app.with_default_widgets()
        .with_widget(producer::view())
        .with_widget(preview::view())
        .with_offscreen_pool_budget(4096);
    let root = app.spawn_root().id();
    let (source, consumer) = app.compose(root, differently_sized_dependency());
    let mut renderer = Fixture {
        only_width: Some(8),
        ..Fixture::default()
    };
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(
        &app.world,
        root,
        &Viewport::new(16, 16, Fixed::ONE),
        &mut renderer,
    )
    .unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(!outputs.get(source).unwrap().texture().is_native());
    assert!(outputs.get(consumer).unwrap().texture().is_native());
    assert_eq!(renderer.unsupported_queries.get(), 1);
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 1);
    assert_eq!(renderer.device.reads.get(), 0);
}

#[test]
fn actual_native_allocation_cannot_consume_the_protected_dependency_capacity() {
    let budget = native_bytes() + 128;
    let mut app = App::headless(16, 16);
    app.with_default_widgets()
        .with_widget(producer::view())
        .with_widget(preview::view())
        .with_offscreen_pool_budget(budget);
    let root = app.spawn_root().id();
    let (source, _, second, _) = app.compose(root, panel());
    app.world.insert(second, crate::ui::OffscreenRender::new());
    let mut renderer = Fixture::default();
    renderer.device.allocation_extra.set(1);
    app.dirty_regions().unwrap();
    assert_eq!(
        crate::ui::render_system::render(
            &app.world,
            root,
            &Viewport::new(16, 16, Fixed::ONE),
            &mut renderer
        ),
        Err(RenderError::ResourceLimit(
            crate::render::RenderResource::Texture
        ))
    );
    assert_eq!(renderer.device.creates.get(), 1);
    assert_eq!(renderer.bindings, 0);
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 0);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert_eq!(outputs.storage().budget.available(), budget);
    assert_eq!(outputs.storage().budget.allocated(), 0);
}

#[test]
fn native_metadata_allocation_failure_keeps_the_existing_admission_path() {
    let mut app = App::headless(16, 16);
    app.with_default_widgets()
        .with_widget(producer::view())
        .with_widget(preview::view())
        .with_offscreen_pool_budget(4096);
    let root = app.spawn_root().id();
    let (source, _, second, _) = app.compose(root, panel());
    app.world.insert(second, crate::ui::OffscreenRender::new());
    let viewport = Viewport::new(16, 16, Fixed::ONE);
    let mut renderer = Fixture::default();
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    let creates = renderer.device.creates.get();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let mut plan = outputs.take_texture_plan().unwrap();
        plan.trim_memory();
        outputs.put_texture_plan(plan);
    }
    app.world.invalidate_visual(source);
    app.dirty_regions().unwrap();
    let result = crate::test_support::failing_allocations_after(0, || {
        crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer)
    });
    result.unwrap();
    assert_eq!(renderer.device.creates.get(), creates);
    assert_eq!(renderer.device.reads.get(), 0);
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 2);
}

#[crate::compose]
fn shared_consumers(source_size: u16, second_width: u16) -> (Entity, Entity, Entity) {
    let source = ui! {
        View (width: source_size, height: source_size) [Producer { color: Color::rgb(20, 80, 160), calls: Cell::new(0) }]
    };
    let first = ui! {
        View (width: 4, height: 4) [Preview { source }, crate::ui::OffscreenRender::new()]
    };
    let second = ui! {
        View (width: second_width, height: 4) [Preview { source }, crate::ui::OffscreenRender::new()]
    };
    (source, first, second)
}

type SharedApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

fn shared_app(budget: usize) -> SharedApp {
    let mut app = App::headless(32, 32);
    app.with_default_widgets()
        .with_widget(producer::view())
        .with_widget(preview::view())
        .with_offscreen_pool_budget(budget);
    app
}

#[test]
fn a_shared_pending_source_prefers_cpu_for_its_cpu_consumer_without_storage_churn() {
    let mut app = shared_app(4096);
    let root = app.spawn_root().id();
    let (source, native, cpu) = app.compose(root, shared_consumers(4, 8));
    let mut renderer = Fixture {
        only_width: Some(4),
        ..Fixture::default()
    };
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    for iteration in 0..3 {
        app.world.invalidate_visual(source);
        app.dirty_regions().unwrap();
        crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(!outputs.get(source).unwrap().texture().is_native());
        assert!(outputs.get(native).unwrap().texture().is_native());
        assert!(!outputs.get(cpu).unwrap().texture().is_native());
        assert_eq!(renderer.device.creates.get(), 1);
        assert_eq!(renderer.device.reads.get(), 0);
        assert_eq!(
            app.world.get::<Producer>(source).unwrap().calls.get(),
            iteration + 1
        );
    }
}

#[test]
fn a_ready_native_source_is_reused_without_reexecuting_its_view_for_a_cpu_consumer() {
    let mut app = shared_app(4096);
    let root = app.spawn_root().id();
    let (source, _, cpu) = app.compose(root, shared_consumers(4, 8));
    let mut renderer = Fixture::default();
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    renderer.only_width = Some(4);
    app.world.invalidate_visual(cpu);
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(outputs.get(source).unwrap().texture().is_native());
    assert!(!outputs.get(cpu).unwrap().texture().is_native());
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 1);
    assert_eq!(renderer.device.reads.get(), 1);
}

#[test]
fn cpu_preference_keeps_a_reusable_native_source_when_replacement_has_no_spare_capacity() {
    let budget = 3 * native_bytes() + 2 * (16 * 16 * 4 - 64);
    let mut app = shared_app(budget);
    let root = app.spawn_root().id();
    let (source, _, _) = app.compose(root, shared_consumers(16, 4));
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    let mut renderer = Fixture::default();
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    renderer.only_width = Some(16);
    for iteration in 0..2 {
        app.world.invalidate_visual(source);
        app.dirty_regions().unwrap();
        crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(outputs.get(source).unwrap().texture().is_native());
        assert!(outputs.texture_budget_available() < 16 * 16 * 4);
        assert_eq!(
            app.world.get::<Producer>(source).unwrap().calls.get(),
            iteration + 2
        );
        assert_eq!(renderer.device.reads.get(), iteration + 1);
    }
}

#[test]
fn dependency_consumers_sample_native_outputs_and_cpu_effects_read_each_revision_once() {
    let mut app = App::headless(16, 16);
    app.with_default_widgets()
        .with_widget(producer::view())
        .with_widget(preview::view())
        .with_offscreen_pool_budget(4096);
    let root = app.spawn_root().id();
    let (source, first, second, temporal) = app.compose(root, panel());
    app.world.insert(second, crate::ui::OffscreenRender::new());
    let viewport = Viewport::new(16, 16, Fixed::ONE);
    let mut renderer = Fixture::default();
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    for entity in [source, first, second] {
        assert!(outputs.get(entity).unwrap().texture().is_native());
    }
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 1);
    assert_eq!(renderer.device.reads.get(), 0);
    assert!(renderer.device.samples.get() >= 3);
    let bindings = renderer.bindings;
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    assert_eq!(renderer.bindings, bindings);
    assert_eq!(renderer.device.reads.get(), 0);

    app.world
        .insert(temporal, TemporalMix::new(source).with_mix(128));
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    assert_eq!(renderer.device.reads.get(), 1);
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(temporal)
            .unwrap()
            .texture()
            .is_native()
    );
    app.world.get_mut::<Producer>(source).unwrap().color = Color::rgb(0, 0, 200);
    app.world.invalidate_visual(source);
    app.dirty_regions().unwrap();
    crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
    assert_eq!(app.world.get::<Producer>(source).unwrap().calls.get(), 2);
    let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
    let previous = outputs.previous(temporal).unwrap();
    assert_eq!(
        previous.texture().read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(200, 0, 0)
    );
    let reads = renderer.device.reads.get();
    assert_eq!(reads, 4);
    let current = outputs.get(temporal).unwrap();
    // Change detection already materialized the successful output before publication.
    assert!(current.texture().pixels().unwrap().is_some());
    assert_eq!(
        current.texture().read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(100, 0, 99)
    );
    let after = renderer.device.reads.get();
    assert_eq!(after, reads);
    current.texture().read_pixels().unwrap();
    assert_eq!(renderer.device.reads.get(), after);
}
