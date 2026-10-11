use super::*;
use crate::render::renderer::DrawRequest;
use crate::test_support::tracked_allocations;
use crate::ui::dirty::{Dirty, VisualDirty};

#[crate::component]
struct Tile;

#[crate::component]
struct Value;

#[crate::component]
struct CrossTile(Entity);

#[crate::view(component = Tile, name = "local")]
fn local(rows: query!(Tile, optional Value), entity: entity!()) {
    assert!(rows.get(entity).is_some());
}

#[crate::view(component = CrossTile, name = "cross")]
fn cross(tile: component!(), rows: query!(Value)) {
    let _ = rows.get(tile.0);
}

#[crate::view(component = Tile, name = "conditional")]
fn conditional(rows: query!(Tile), entity: entity!(), iterate: res!(Cell<bool>)) {
    if iterate.get() {
        for _ in rows {}
    } else {
        let _ = rows.get(entity);
    }
}

#[crate::view(component = Tile)]
fn filtered(rows: query!(Tile; without Value;), entity: entity!()) {
    let _ = rows.get(entity);
}

#[crate::view(component = Tile)]
fn failing(rows: query!(Tile), entity: entity!(), paint: paint!()) {
    let _ = rows.get(entity);
    paint.fail(RenderError::BackendFailure);
}

struct Sink;

impl Renderer for Sink {
    fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        Ok(())
    }

    fn flush(&mut self) {}
}

fn paint(world: &World, entity: Entity, name: &str) {
    let registry = world.resource::<ViewRegistry>().unwrap();
    let view = registry.iter().find(|view| view.name() == name).unwrap();
    let style = Style::default();
    let rect = Rect::new(0, 0, 8, 8);
    let mut sink = Sink;
    let mut ctx = PaintCtx::new(
        &mut sink,
        &style,
        &rect,
        &rect,
        Transform::IDENTITY,
        None,
        WidgetState::Enabled,
    );
    view.paint(world, entity, &mut ctx);
}

fn clear(world: &mut World, entities: &[Entity]) {
    for entity in entities {
        world.remove::<Dirty>(*entity);
        world.remove::<VisualDirty>(*entity);
    }
}

#[test]
fn local_optional_changes_and_global_sources_reach_their_consumers() {
    let mut app = crate::app::App::headless(32, 32);
    app.with_widget(local::view());
    app.with_widget(cross::view());
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    let follower = app.world.spawn_empty();
    app.world.insert(first, Tile);
    app.world.insert(second, Tile);
    app.world.insert(follower, CrossTile(first));
    ViewRegistry::reconcile_observations(&mut app.world);
    paint(&app.world, first, "local");
    paint(&app.world, second, "local");
    paint(&app.world, follower, "cross");
    for operation in 0..3 {
        clear(&mut app.world, &[first, second, follower]);
        match operation {
            0 => app.world.insert(first, Value),
            1 => {
                *app.world.get_mut::<Value>(first).unwrap() = Value;
            }
            _ => {
                app.world.remove::<Value>(first);
            }
        }
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.has::<VisualDirty>(first));
        assert!(app.world.has::<VisualDirty>(follower));
        assert!(!app.world.has::<VisualDirty>(second));
    }
    clear(&mut app.world, &[first, second, follower]);
    let allocations = tracked_allocations(|| {
        app.world.invalidate_query_dependency::<Value>();
        ViewRegistry::reconcile_observations(&mut app.world);
    });
    assert_eq!(allocations, 0);
    for entity in [first, second, follower] {
        assert!(app.world.has::<VisualDirty>(entity));
    }
}

#[test]
fn a_view_cannot_shrink_its_scope_after_cross_entity_iteration() {
    let mut app = crate::app::App::headless(32, 32);
    app.with_widget(conditional::view());
    app.add_resource(Cell::new(false));
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    app.world.insert(first, Tile);
    app.world.insert(second, Tile);
    ViewRegistry::reconcile_observations(&mut app.world);
    paint(&app.world, first, "conditional");
    let scope = || {
        app.world
            .resource::<ViewRegistry>()
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .definition
            .query_scope
            .get()
    };
    assert!(scope() == QueryScope::Local);
    app.world.resource::<Cell<bool>>().unwrap().set(true);
    paint(&app.world, first, "conditional");
    assert!(scope() == QueryScope::Global);
    app.world.resource::<Cell<bool>>().unwrap().set(false);
    paint(&app.world, first, "conditional");
    assert!(scope() == QueryScope::Global);
    clear(&mut app.world, &[first, second]);
    app.world.insert(second, Tile);
    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(app.world.has::<VisualDirty>(first));
}

#[test]
fn component_publications_invalidate_local_and_cross_entity_readers_without_allocation() {
    let mut app = crate::app::App::headless(32, 32);
    app.with_widget(local::view());
    app.with_widget(cross::view());
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    let follower = app.world.spawn_empty();
    app.world.insert(first, Tile);
    app.world.insert(second, Tile);
    app.world.insert(follower, CrossTile(first));
    ViewRegistry::reconcile_observations(&mut app.world);
    paint(&app.world, first, "local");
    paint(&app.world, second, "local");
    paint(&app.world, follower, "cross");
    clear(&mut app.world, &[first, second, follower]);
    assert_eq!(
        tracked_allocations(|| {
            ViewRegistry::component_published::<Value>(&mut app.world, first);
        }),
        0
    );
    assert!(app.world.has::<VisualDirty>(first));
    assert!(app.world.has::<VisualDirty>(follower));
    assert!(!app.world.has::<VisualDirty>(second));
}

#[test]
fn local_presence_filters_and_failed_paint_keep_complete_invalidation() {
    let mut app = crate::app::App::headless(32, 32);
    app.with_widget(filtered::view());
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    app.world.insert(first, Tile);
    app.world.insert(second, Tile);
    ViewRegistry::reconcile_observations(&mut app.world);
    paint(&app.world, first, "filtered");
    for present in [true, false] {
        clear(&mut app.world, &[first, second]);
        if present {
            app.world.insert(first, Value);
        } else {
            app.world.remove::<Value>(first);
        }
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.has::<VisualDirty>(first));
        assert!(!app.world.has::<VisualDirty>(second));
    }
    app.with_widget(failing::view());
    ViewRegistry::reconcile_observations(&mut app.world);
    paint(&app.world, first, "failing");
    clear(&mut app.world, &[first, second]);
    app.world.insert(second, Tile);
    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(app.world.has::<VisualDirty>(first));
}

#[test]
fn text_changes_stay_local_and_cache_replacement_still_rebuilds_layout() {
    use crate::surface::FramebufferAccess;
    use crate::ui::widgets::Text;
    let mut app = crate::app::App::headless(64, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        crate::ui! {
            Column () {
                Text ("A", id: "fixed", width: 24, height: 12)
                Text ("B", id: "changing", width: 24, height: 12)
            }
        };
    });
    let fixed = app.world.find_by_id("fixed").unwrap();
    let changing = app.world.find_by_id("changing").unwrap();
    app.render_dirty().unwrap();
    let before = app.backend.framebuffer().buf.as_slice().to_vec();
    let fixed_handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(fixed)
        .unwrap();
    app.world
        .get_mut::<Text>(changing)
        .unwrap()
        .set_content("C");
    ViewRegistry::reconcile_observations(&mut app.world);
    assert!(app.world.has::<Dirty>(changing));
    assert!(!app.world.has::<Dirty>(fixed));
    assert!(!app.world.has::<VisualDirty>(fixed));
    app.render_dirty().unwrap();
    let changed = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_ne!(before, changed);
    app.render().unwrap();
    assert_eq!(changed, app.backend.framebuffer().buf.as_slice());
    assert_eq!(
        app.world.get::<crate::text::TextLayoutHandle>(fixed),
        Some(&fixed_handle)
    );
    for source in 0..3 {
        match source {
            0 => {
                app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                    .unwrap();
            }
            1 => {
                app.with_fonts(crate::render::font::default_font_manager());
            }
            _ => app.handle_memory_warning(),
        }
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.has::<Dirty>(fixed));
        assert!(app.world.has::<Dirty>(changing));
        app.render_dirty().unwrap();
        assert_eq!(changed, app.backend.framebuffer().buf.as_slice());
        let outputs = app
            .world
            .resource::<crate::ui::Outputs<crate::text::TextGeometry>>()
            .unwrap();
        for entity in [fixed, changing] {
            let handle = *app
                .world
                .get::<crate::text::TextLayoutHandle>(entity)
                .unwrap();
            assert!(outputs.storage().layouts.get(handle).is_some());
        }
    }
}
