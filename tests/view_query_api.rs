use mirui::app::App;
use mirui::ecs::Entity;
use mirui::types::Dimension;
use mirui::ui::layout::LayoutStyle;
use mirui::ui::property::{apply_to_world, prop};
use mirui::ui::{ComputedRect, Style};
use mirui::{component, view};
use std::cell::Cell;
use std::marker::PhantomData;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[component]
struct Tile {
    target: Entity,
}

#[component]
struct Source(u16);

#[component]
struct Accent(u16);

#[component]
struct Active;

#[component]
struct Excluded;

#[derive(Default)]
struct Trace {
    calls: Cell<usize>,
    value: Cell<u16>,
    total: Cell<u16>,
}

#[view(component = Tile)]
fn paint_tile(
    tile: component!(),
    rows: query!(Entity, Source, optional Accent; with Active; without Excluded;),
    trace: res!(Trace),
) {
    let value = rows.get(tile.target).map_or(0, |(_, source, accent)| {
        source.0 + accent.map_or(0, |accent| accent.0)
    });
    let total = rows.into_iter().map(|(_, source, _)| source.0).sum();
    trace.value.set(value);
    trace.total.set(total);
    trace.calls.set(trace.calls.get() + 1);
}

#[test]
fn query_values_optional_data_and_filter_membership_automatically_repaint() {
    let mut app = App::headless(32, 32);
    app.add_resource(Trace::default());
    app.with_widget(paint_tile::view());
    let source = app.world.spawn_empty();
    let other = app.world.spawn_empty();
    app.world.insert(source, Source(4));
    app.world.insert(source, Active);
    app.world.insert(other, Source(7));
    app.world.insert(other, Active);
    let root = app.spawn_root().id();
    app.world.insert(root, Tile { target: source });
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 4);
    assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 11);
    let calls = app.world.resource::<Trace>().unwrap().calls.get();
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().calls.get(), calls);

    app.world.get_mut::<Source>(source).unwrap().0 = 5;
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 5);
    app.world.insert(source, Accent(9));
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 14);
    app.world.remove::<Accent>(source);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 5);

    app.world.insert(source, Excluded);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 0);
    app.world.remove::<Excluded>(source);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 5);
    app.world.remove::<Active>(source);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 7);
    app.world.insert(source, Active);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 12);
    app.world.remove::<Source>(source);
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 0);
    app.world.insert(source, Source(6));
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 13);
    assert!(app.world.despawn(other));
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 6);

    for value in [8, 9] {
        assert_eq!(
            tracked_allocations(|| {
                app.world.get_mut::<Source>(source).unwrap().0 = value;
                app.render_dirty().unwrap();
            }),
            0
        );
        assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), value);
    }
}

#[view(component = Tile)]
fn paint_geometry(tile: component!(), rects: query!(ComputedRect), trace: res!(Trace)) {
    trace
        .value
        .set(rects.get(tile.target).unwrap().0.w.trunc_to_int() as u16);
    trace.calls.set(trace.calls.get() + 1);
}

#[test]
fn layout_outputs_repaint_distant_query_consumers_in_the_same_frame_then_stay_idle() {
    let mut app = App::headless(64, 64);
    app.add_resource(Trace::default());
    app.with_widget(paint_geometry::view());
    let root = app.spawn_root().id();
    let source = app.compose(root, |tree| {
        let layout = LayoutStyle {
            width: Dimension::px(10),
            height: Dimension::px(10),
            ..LayoutStyle::default()
        };
        let source = tree.spawn(Style {
            layout,
            ..Style::default()
        });
        tree.spawn((
            Style {
                layout,
                ..Style::default()
            },
            Tile { target: source },
        ));
        source
    });
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 10);
    app.render_dirty().unwrap();
    let calls = app.world.resource::<Trace>().unwrap().calls.get();
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().calls.get(), calls);

    apply_to_world::<prop::Width>(&mut app.world, source, Dimension::px(25));
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 25);
    assert!(app.world.resource::<Trace>().unwrap().calls.get() > calls);
    let calls = app.world.resource::<Trace>().unwrap().calls.get();
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().calls.get(), calls);
}

#[component]
struct GenericTile<T: 'static>(PhantomData<T>);

#[view(component = self::GenericTile<T>)]
fn paint_generic<T: mirui::ecs::Component>(
    rows: query!(Entity, optional T; with self::Active;),
    trace: res!(Trace),
) {
    trace.total.set(
        rows.into_iter()
            .filter(|(_, value)| value.is_some())
            .count() as u16,
    );
    trace.calls.set(trace.calls.get() + 1);
}

#[test]
fn generic_shared_queries_register_empty_storage_and_keep_apps_independent() {
    let mut first = App::headless(32, 32);
    let mut second = App::headless(32, 32);
    for app in [&mut first, &mut second] {
        app.add_resource(Trace::default());
        assert_eq!(
            tracked_allocations(|| {
                let _ = paint_generic::view::<Accent>();
            }),
            0
        );
        app.with_widget(paint_generic::view::<Accent>());
        let root = app.spawn_root().id();
        app.world.insert(root, GenericTile::<Accent>(PhantomData));
        app.render_dirty().unwrap();
        assert_eq!(app.world.resource::<Trace>().unwrap().total.get(), 0);
    }
    let second_calls = second.world.resource::<Trace>().unwrap().calls.get();
    let source = first.world.spawn_empty();
    first.world.insert(source, Active);
    first.world.insert(source, Accent(1));
    first.render_dirty().unwrap();
    second.render_dirty().unwrap();
    assert_eq!(first.world.resource::<Trace>().unwrap().total.get(), 1);
    assert_eq!(second.world.resource::<Trace>().unwrap().total.get(), 0);
    assert_eq!(
        second.world.resource::<Trace>().unwrap().calls.get(),
        second_calls
    );
    first.world.get_mut::<Accent>(source).unwrap().0 = 2;
    first.render().unwrap();
    assert_eq!(first.world.resource::<Trace>().unwrap().total.get(), 1);
    first.render_dirty().unwrap();
    let calls = first.world.resource::<Trace>().unwrap().calls.get();
    first.render_dirty().unwrap();
    assert_eq!(first.world.resource::<Trace>().unwrap().calls.get(), calls);
}

#[view(component = Tile)]
fn paint_generation(
    tile: component!(),
    generations: query!(mirui::ui::OffscreenGeneration),
    trace: res!(Trace),
) {
    trace.value.set(
        generations
            .get(tile.target)
            .map_or(0, |generation| generation.0 as u16),
    );
    trace.calls.set(trace.calls.get() + 1);
}

#[test]
fn dirty_plan_outputs_remain_pending_for_the_next_frame_without_replaying_the_walker() {
    let mut app = App::headless(64, 64);
    app.add_resource(Trace::default());
    app.with_widget(paint_generation::view());
    let root = app.spawn_root().id();
    let children = app.compose(root, |tree| {
        let source = tree.spawn((
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(10),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
            mirui::ui::OffscreenRender::new(),
            mirui::ui::dirty::Dirty,
        ));
        let spacer = tree.spawn((
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(20),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
            mirui::ui::dirty::Dirty,
        ));
        let tile = tree.spawn((
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(10),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
            Tile { target: source },
            mirui::ui::dirty::Dirty,
        ));
        [source, spacer, tile]
    });
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 1);
    app.render_dirty().unwrap();
    let calls = app.world.resource::<Trace>().unwrap().calls.get();
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().calls.get(), calls);
    app.world.invalidate_visual(children[0]);
    app.render_dirty().unwrap();
    assert_eq!(
        app.world
            .get::<mirui::ui::OffscreenGeneration>(children[0])
            .unwrap()
            .0,
        2
    );
    assert_eq!(app.world.resource::<Trace>().unwrap().calls.get(), calls);
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().calls.get(),
        calls + 1
    );
    assert_eq!(app.world.resource::<Trace>().unwrap().value.get(), 2);
    assert_eq!(
        app.world
            .get::<mirui::ui::OffscreenGeneration>(children[0])
            .unwrap()
            .0,
        2
    );
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().calls.get(),
        calls + 1
    );
}
