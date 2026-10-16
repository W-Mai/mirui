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
