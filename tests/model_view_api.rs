use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::render::DrawCommand;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::ui::Style;
use mirui::ui::layout::{FlexDirection, LayoutStyle};
use mirui::ui::view::{PaintCtx, ViewInvalidation};
use mirui::{component, model, view};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

static PAINT_COUNT: AtomicUsize = AtomicUsize::new(0);
static PAINT_VALUE: AtomicUsize = AtomicUsize::new(0);
static TILE_COUNTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
static TILE_VALUES: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];

fn tile_counts() -> [usize; 3] {
    core::array::from_fn(|slot| TILE_COUNTS[slot].load(Ordering::Relaxed))
}

fn tile_values() -> [usize; 3] {
    core::array::from_fn(|slot| TILE_VALUES[slot].load(Ordering::Relaxed))
}

#[model]
struct Level {
    #[observe]
    value: u16,
}

#[model]
impl Level {
    fn set(&mut self, value: u16) {
        self.value = value;
    }
}

#[component]
struct Pair {
    left: model!(Level),
    right: model!(Level),
}

#[view(component = Pair, priority = 64)]
fn paint_pair(
    left: model!(component.left, watch(value)),
    right: model!(component.right, watch(value)),
    paint: paint!(),
) {
    paint.set_background_handled(left.value < right.value);
}

#[model]
struct PaintModel {
    #[observe]
    value: u8,
}

#[model]
impl PaintModel {
    fn set_value(&mut self, value: u8) {
        self.value = value;
    }
}

#[component]
struct PaintTile {
    model: model!(PaintModel),
}

#[view(component = PaintTile)]
fn paint_tile(model: model!(component.model, watch(value))) {
    PAINT_VALUE.store(usize::from(model.value), Ordering::Relaxed);
    PAINT_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[component]
struct SharedTile {
    model: model!(PaintModel),
    slot: usize,
}

#[view(component = SharedTile)]
fn paint_shared_tile(tile: component!(), model: model!(component.model, watch(value))) {
    TILE_VALUES[tile.slot].store(usize::from(model.value), Ordering::Relaxed);
    TILE_COUNTS[tile.slot].fetch_add(1, Ordering::Relaxed);
}

#[view(component = PaintTile)]
fn paint_drop(model: model!(component.model, watch(value))) {
    let _ = model.value;
}

#[view(component = PaintTile, invalidation = Layout)]
fn observe_tile(model: model!(component.model, watch(value))) {}

#[test]
fn empty_view_retains_model_observation_without_a_paint_callback() {
    use mirui::ui::dirty::Dirty;

    let mut app = App::headless(32, 32);
    let first = app.add_model(PaintModel { value: 1 });
    let second = app.add_model(PaintModel { value: 7 });
    for _ in 0..2 {
        assert_eq!(
            tracked_allocations(|| {
                let _ = observe_tile::view();
            }),
            0
        );
    }
    app.with_widget(observe_tile::view());
    let root = app.spawn_root().id();
    app.world.insert(
        root,
        PaintTile {
            model: first.share(),
        },
    );
    app.render().unwrap();
    app.world.remove::<Dirty>(root);
    for value in [2, 3] {
        assert_eq!(
            tracked_allocations(|| {
                first.set_value(value);
                flush_signal_dirty(&mut app.world);
            }),
            0
        );
        assert!(app.world.remove::<Dirty>(root).is_some());
    }
    app.world.insert(
        root,
        PaintTile {
            model: second.share(),
        },
    );
    app.render_dirty().unwrap();
    app.world.remove::<Dirty>(root);
    first.set_value(4);
    flush_signal_dirty(&mut app.world);
    assert!(!app.world.has::<Dirty>(root));
    assert_eq!(
        tracked_allocations(|| {
            second.set_value(8);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    assert!(app.world.remove::<Dirty>(root).is_some());
}

#[component]
struct PixelTile {
    model: model!(PaintModel),
}

#[view(component = PixelTile)]
fn paint_pixel_tile(model: model!(component.model, watch(value)), paint: paint!()) {
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color: Color::rgb(model.value, 0, 0),
        radius: Fixed::ZERO,
        opa: 255,
    });
    paint.set_background_handled(true);
}

fn attempt_raw_view_write(
    world: &mirui::ecs::World,
    entity: mirui::ecs::Entity,
    _paint: &mut PaintCtx<'_>,
) {
    world.get::<PaintTile>(entity).unwrap().model.set_value(99);
}

#[test]
fn raw_view_cannot_write_a_model_during_render() {
    use mirui::__macro_support::{ViewDescriptor, assemble_view};

    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let model = app.add_model(PaintModel { value: 1 });
    app.with_widget(assemble_view::<PaintTile>(ViewDescriptor {
        name: "attempt_raw_view_write",
        paint: Some((64, attempt_raw_view_write)),
        attach: None,
        gesture: None,
        observation: None,
        query_dependencies: &[],
        systems: &[],
        invalidation: ViewInvalidation::Visual,
    }));
    app.world.insert(
        root,
        PaintTile {
            model: model.share(),
        },
    );

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.render()));
    assert!(failure.is_err());
    assert_eq!(model.value(), 1);
    model.set_value(2);
    assert_eq!(model.value(), 2);
}

#[test]
fn external_crate_registers_a_multi_model_view() {
    let mut app = App::headless(32, 32);
    let left = app.add_model(Level { value: 1 });
    let right = app.add_model(Level { value: 2 });
    app.with_widget(paint_pair::view());
    let entity = app.world.spawn_empty();
    app.world.insert(
        entity,
        Pair {
            left: left.share(),
            right: right.share(),
        },
    );
    left.set(3);
    right.set(4);
}

#[test]
fn typed_view_tracks_real_render_and_component_rebinding() {
    PAINT_COUNT.store(0, Ordering::Relaxed);
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let first = app.add_model(PaintModel { value: 1 });
    let second = app.add_model(PaintModel { value: 7 });
    app.with_widget(paint_tile::view());
    app.world.insert(
        root,
        PaintTile {
            model: first.share(),
        },
    );
    app.render().unwrap();
    assert_eq!(PAINT_VALUE.load(Ordering::Relaxed), 1);
    let before = PAINT_COUNT.load(Ordering::Relaxed);

    assert_eq!(
        tracked_allocations(|| {
            first.set_value(2);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    app.render_dirty().unwrap();
    assert_eq!(PAINT_VALUE.load(Ordering::Relaxed), 2);
    assert!(PAINT_COUNT.load(Ordering::Relaxed) > before);

    app.world.insert(
        root,
        PaintTile {
            model: second.share(),
        },
    );
    second.set_value(8);
    app.render_dirty().unwrap();
    assert_eq!(PAINT_VALUE.load(Ordering::Relaxed), 8);
    let rebound_count = PAINT_COUNT.load(Ordering::Relaxed);
    assert_eq!(tracked_allocations(|| first.set_value(3)), 0);
    app.render_dirty().unwrap();
    assert_eq!(PAINT_COUNT.load(Ordering::Relaxed), rebound_count);

    assert_eq!(tracked_allocations(|| second.set_value(9)), 0);
    app.render_dirty().unwrap();
    assert_eq!(PAINT_VALUE.load(Ordering::Relaxed), 9);

    app.world.remove::<PaintTile>(root);
    app.render_dirty().unwrap();
    let removed_count = PAINT_COUNT.load(Ordering::Relaxed);
    assert_eq!(tracked_allocations(|| second.set_value(10)), 0);
    app.render_dirty().unwrap();
    assert_eq!(PAINT_COUNT.load(Ordering::Relaxed), removed_count);
}

#[test]
fn first_model_view_update_paints_without_allocating() {
    let red = Rc::new(Cell::new(0));
    let flushes = Rc::new(Cell::new(0));
    let red_on_flush = red.clone();
    let flushes_on_flush = flushes.clone();
    let mut app = App::new(FramebufSurface::new(16, 16, move |pixels, _| {
        red_on_flush.set(pixels[0]);
        flushes_on_flush.set(flushes_on_flush.get() + 1);
    }));
    app.with_default_widgets();
    app.with_widget(paint_pixel_tile::view());
    let root = app.spawn_root().id();
    let model = app.add_model(PaintModel { value: 1 });
    app.world.insert(
        root,
        PixelTile {
            model: model.share(),
        },
    );
    app.render().unwrap();
    assert_eq!(red.get(), 1);
    assert_eq!(flushes.get(), 1);

    let action_allocations = tracked_allocations(|| model.set_value(2));
    let notify_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
    assert_eq!(
        (action_allocations, notify_allocations, render_allocations),
        (0, 0, 0)
    );
    assert_eq!(red.get(), 2);
    assert_eq!(flushes.get(), 2);
}

#[test]
fn shared_views_update_together_while_other_instances_stay_independent() {
    for count in &TILE_COUNTS {
        count.store(0, Ordering::Relaxed);
    }
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    app.with_widget(paint_shared_tile::view());
    let shared = app.add_model(PaintModel { value: 1 });
    let independent = app.add_model(PaintModel { value: 7 });
    let tile_layout = LayoutStyle {
        width: Dimension::px(32),
        height: Dimension::px(10),
        ..LayoutStyle::default()
    };
    let root = app
        .spawn_root()
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(32),
            height: Dimension::px(32),
            ..LayoutStyle::default()
        })
        .id();
    let tiles: [_; 3] = app.compose(root, |tree| {
        core::array::from_fn(|slot| {
            tree.spawn((
                Style {
                    layout: tile_layout,
                    ..Style::default()
                },
                SharedTile {
                    model: if slot == 2 {
                        independent.share()
                    } else {
                        shared.share()
                    },
                    slot,
                },
            ))
        })
    });
    app.render().unwrap();
    app.render_dirty().unwrap();
    assert_eq!(tile_values(), [1, 1, 7]);
    let before = tile_counts();

    assert_eq!(
        tracked_allocations(|| {
            shared.set_value(2);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    app.render_dirty().unwrap();
    assert_eq!(tile_values(), [2, 2, 7]);
    let after_shared = tile_counts();
    assert!(after_shared[0] > before[0]);
    assert!(after_shared[1] > before[1]);
    assert_eq!(after_shared[2], before[2]);

    assert_eq!(
        tracked_allocations(|| {
            independent.set_value(8);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    assert_eq!(
        app.world.get::<SharedTile>(tiles[0]).unwrap().model.value(),
        2
    );
    assert_eq!(
        app.world.get::<SharedTile>(tiles[1]).unwrap().model.value(),
        2
    );
    assert_eq!(
        app.world.get::<SharedTile>(tiles[2]).unwrap().model.value(),
        8
    );
    app.render_dirty().unwrap();
    assert_eq!(tile_values(), [2, 2, 8]);
    let after_independent = tile_counts();
    assert_eq!(after_independent[0], after_shared[0]);
    assert_eq!(after_independent[1], after_shared[1]);
    assert!(after_independent[2] > after_shared[2]);
}

#[test]
fn dropping_app_releases_typed_view_subscriptions() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let model = app.add_model(PaintModel { value: 1 });
    app.with_widget(paint_drop::view());
    app.world.insert(
        root,
        PaintTile {
            model: model.share(),
        },
    );
    app.render().unwrap();
    drop(app);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            model.set_value(2);
        }))
        .is_err()
    );
}
