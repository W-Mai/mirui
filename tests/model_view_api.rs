use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::types::Dimension;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::LayoutStyle;
use mirui::ui::view::ViewCtx;
use mirui::{component, model, view};
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

#[component(bind(left, right))]
struct Pair {
    left: Level,
    right: Level,
}

#[view(
    component = Pair,
    read(left, right),
    watch(left.value(), right.value()),
    priority = 64,
)]
fn paint_pair(left: &Level, right: &Level, ctx: &mut ViewCtx<'_>) {
    ctx.bg_handled = left.value < right.value;
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

#[component(bind(model))]
struct PaintTile {
    model: PaintModel,
}

#[view(component = PaintTile, read(model), watch(model.value()))]
fn paint_tile(model: &PaintModel) {
    PAINT_VALUE.store(usize::from(model.value), Ordering::Relaxed);
    PAINT_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[component(bind(model))]
struct SharedTile {
    model: PaintModel,
    slot: usize,
}

#[view(component = SharedTile, read(model), watch(model.value()))]
fn paint_shared_tile(model: &PaintModel, component: &SharedTile) {
    TILE_VALUES[component.slot].store(usize::from(model.value), Ordering::Relaxed);
    TILE_COUNTS[component.slot].fetch_add(1, Ordering::Relaxed);
}

#[view(component = PaintTile, read(model), watch(model.value()))]
fn paint_drop(model: &PaintModel) {
    let _ = model.value;
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
        width: Dimension::px(10),
        height: Dimension::px(10),
        ..LayoutStyle::default()
    };
    let tiles: [_; 3] =
        core::array::from_fn(|_| WidgetBuilder::new(&mut app.world).layout(tile_layout).id());
    let root = WidgetBuilder::new(&mut app.world)
        .child(tiles[0])
        .child(tiles[1])
        .child(tiles[2])
        .id();
    app.set_root(root);
    for (slot, entity) in tiles.into_iter().enumerate() {
        app.world.insert(
            entity,
            SharedTile {
                model: if slot == 2 {
                    independent.share()
                } else {
                    shared.share()
                },
                slot,
            },
        );
    }
    app.render().unwrap();
    assert_eq!(tile_values(), [1, 1, 7]);
    let before = tile_counts();

    assert_eq!(tracked_allocations(|| shared.set_value(2)), 0);
    app.render_dirty().unwrap();
    assert_eq!(tile_values(), [2, 2, 7]);
    let after_shared = tile_counts();
    assert!(after_shared[0] > before[0]);
    assert!(after_shared[1] > before[1]);

    assert_eq!(tracked_allocations(|| independent.set_value(8)), 0);
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
