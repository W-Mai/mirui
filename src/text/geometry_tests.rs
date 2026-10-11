use super::TextGeometry;
use crate::ecs::Entity;
use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::render::DrawCommand;
use crate::surface::FramebufferAccess;
use crate::text::{PathCaretGeometry, TextLayoutHandle, TextLayoutLimits, TextPath};
use crate::types::{Transform, Viewport};
use crate::ui::widgets::Placeholder;
use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;
use crate::ui::widgets::{Text, TextInput};
use crate::ui::{Hidden, Outputs};

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

#[crate::component]
struct GeometryDemand {
    #[depends_on(TextGeometry)]
    source: Entity,
}

#[crate::view(component = GeometryDemand)]
fn geometry_demand() {}

#[crate::compose]
fn shared_text_scene() {
    ui! {
        View (width: 96, height: 48) {
            Text (
                "AB",
                id: "geometry_source",
                position: Position::Absolute,
                left: 7,
                top: 5,
                width: 64,
                height: 24
            ) [
                TextInput::new(),
                Placeholder("XYZ"),
            ]
            View (id: "geometry_consumer", width: 1, height: 1) [
                GeometryDemand {
                    source: id("geometry_source"),
                },
            ]
        }
    };
}

fn fixture() -> (TestApp, Entity, Entity) {
    let mut app = App::headless(96, 48);
    app.with_default_widgets()
        .with_widget(geometry_demand::view());
    let root = app.spawn_root().id();
    app.compose(root, shared_text_scene());
    let source = app.world.find_by_id("geometry_source").unwrap();
    (app, root, source)
}

fn outputs(app: &TestApp) -> &Outputs<TextGeometry> {
    app.world.resource::<Outputs<TextGeometry>>().unwrap()
}

fn version(app: &TestApp, source: Entity) -> u64 {
    outputs(app).state(source).unwrap().current.unwrap().version
}

fn assert_missing_with_demand(app: &TestApp, source: Entity) {
    let outputs = outputs(app);
    assert!(outputs.is_requested(source));
    assert!(outputs.get(source).is_none());
    assert!(outputs.state(source).unwrap().current.is_none());
}

#[test]
fn declared_text_output_borrows_the_arena_without_overwriting_same_entity_input() {
    let (mut app, _, source) = fixture();
    assert!(outputs(&app).get(source).is_none());
    app.render().unwrap();

    let text = *app.world.get::<TextLayoutHandle>(source).unwrap();
    let input = app.world.get::<TextEditingLayoutHandle>(source).unwrap().0;
    assert_ne!(text, input);
    {
        let outputs = outputs(&app);
        assert!(outputs.is_requested(source));
        let read = outputs.get(source).unwrap();
        let storage = outputs.storage();
        let text_layout = storage.layouts.get(text).unwrap();
        let input_layout = storage.layouts.get(input).unwrap();
        assert_eq!(read.layout().glyphs().len(), 2);
        assert_eq!(input_layout.glyphs().len(), 3);
        assert_eq!(
            read.layout().glyphs().as_ptr(),
            text_layout.glyphs().as_ptr()
        );
        assert_ne!(
            text_layout.glyphs().as_ptr(),
            input_layout.glyphs().as_ptr()
        );
    }

    let text_version = version(&app, source);
    app.world.insert(
        source,
        crate::ui::widgets::TextEditing::default()
            .with_text("Q")
            .unwrap(),
    );
    app.render_dirty().unwrap();
    let input = app.world.get::<TextEditingLayoutHandle>(source).unwrap().0;
    let storage = outputs(&app).storage();
    assert_eq!(storage.layouts.get(input).unwrap().glyphs().len(), 1);
    assert_eq!(
        outputs(&app).get(source).unwrap().layout().glyphs().len(),
        2
    );
    assert_eq!(version(&app, source), text_version);
}

#[test]
fn unchanged_layout_preparation_and_visual_redraw_do_not_republish_geometry() {
    let (mut app, root, source) = fixture();
    app.render().unwrap();
    app.render_dirty().unwrap();
    let original = version(&app, source);
    let viewport = Viewport::new(96, 48, Fixed::ONE);

    for _ in 0..3 {
        crate::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
        assert!(outputs(&app).get(source).is_some());
        assert_eq!(version(&app, source), original);
        app.world.invalidate_visual(source);
        app.render_dirty().unwrap();
        assert_eq!(version(&app, source), original);
        app.render().unwrap();
        assert_eq!(version(&app, source), original);
    }

    app.world
        .get_mut::<Text>(source)
        .unwrap()
        .set_content("ABCD");
    app.render_dirty().unwrap();
    assert_eq!(
        outputs(&app).get(source).unwrap().layout().glyphs().len(),
        4
    );
    assert!(version(&app, source) > original);
    let changed = version(&app, source);
    app.render_dirty().unwrap();
    assert_eq!(version(&app, source), changed);
    assert!(
        app.world
            .resource::<crate::ui::render_system::LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[test]
fn hidden_or_removed_text_keeps_demand_but_cannot_publish_the_input_layout() {
    let (mut app, _, source) = fixture();
    app.render().unwrap();
    let original = version(&app, source);

    app.world.insert(source, Hidden);
    app.render_dirty().unwrap();
    assert_missing_with_demand(&app, source);
    app.world.remove::<Hidden>(source);
    app.render_dirty().unwrap();
    assert_eq!(
        outputs(&app).get(source).unwrap().layout().glyphs().len(),
        2
    );
    assert!(version(&app, source) > original);

    let text = app.world.remove::<Text>(source).unwrap();
    app.render_dirty().unwrap();
    assert_missing_with_demand(&app, source);
    let input = app.world.get::<TextEditingLayoutHandle>(source).unwrap().0;
    assert_eq!(
        outputs(&app)
            .storage()
            .layouts
            .get(input)
            .unwrap()
            .glyphs()
            .len(),
        3
    );
    app.world.insert(source, text);
    app.render_dirty().unwrap();
    assert_eq!(
        outputs(&app).get(source).unwrap().layout().glyphs().len(),
        2
    );
}

#[test]
fn shared_reads_are_allocation_free_and_trim_or_replacement_requires_preparation() {
    let (mut app, _, source) = fixture();
    app.render().unwrap();
    let original = version(&app, source);
    {
        let outputs = outputs(&app);
        let storage = outputs.storage();
        let read = outputs.get(source).unwrap();
        let glyphs = read.layout().glyphs().as_ptr();
        let resident = storage.layouts.resident_bytes();
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..32 {
                let second = outputs.get(source).unwrap();
                assert_eq!(second.layout().glyphs().as_ptr(), glyphs);
                assert_eq!(second.layout().glyphs().len(), 2);
                assert_eq!(second.metrics(), read.metrics());
                assert_eq!(second.content_rect(), read.content_rect());
                assert_eq!(second.transform(), read.transform());
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(storage.layouts.resident_bytes(), resident);
        assert_eq!(version(&app, source), original);
    }

    for replace in [false, true] {
        let previous = version(&app, source);
        if replace {
            app.try_with_text_layout_limits(TextLayoutLimits::HOST)
                .unwrap();
        } else {
            app.handle_memory_warning();
        }
        assert_missing_with_demand(&app, source);
        {
            let outputs = outputs(&app);
            let storage = outputs.storage();
            let allocations = crate::test_support::tracked_allocations(|| {
                for _ in 0..32 {
                    assert!(outputs.get(source).is_none());
                }
            });
            assert_eq!(allocations, 0);
            assert_eq!(storage.layouts.resident_bytes(), 0);
        }
        app.render_dirty().unwrap();
        assert_eq!(
            outputs(&app).get(source).unwrap().layout().glyphs().len(),
            2
        );
        assert!(version(&app, source) > previous);
    }
}

#[test]
fn retired_source_generation_does_not_transfer_demand_to_a_reused_entity_slot() {
    let (mut app, root, source) = fixture();
    app.render().unwrap();
    let consumer = app.world.find_by_id("geometry_consumer").unwrap();
    crate::ui::despawn_subtree(&mut app.world, source);
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_none());

    let replacement = app.compose(root, |cx| cx.spawn(Text::build("NEW")));
    assert_eq!(replacement.id, source.id);
    assert_ne!(replacement.generation, source.generation);
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_none());
    assert!(!outputs(&app).is_requested(replacement));
    assert!(outputs(&app).get(replacement).is_none());

    app.world
        .get_mut::<GeometryDemand>(consumer)
        .unwrap()
        .source = replacement;
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_none());
    assert_eq!(
        outputs(&app)
            .get(replacement)
            .unwrap()
            .layout()
            .glyphs()
            .len(),
        3
    );
}

#[test]
fn path_carets_stay_local_while_the_prepared_transform_maps_them_once() {
    let (mut app, _, source) = fixture();
    let mut path = crate::render::path::Path::new();
    path.move_to(Point::new(0, 16)).line_to(Point::new(64, 16));
    let path = app.paths().insert(path).unwrap();
    app.world.insert(source, TextPath::new(path));
    app.render().unwrap();

    let mut before = [PathCaretGeometry::default(); 4];
    let screen_before = {
        let read = outputs(&app).get(source).unwrap();
        assert_eq!(read.text_path(), Some(TextPath::new(path)));
        assert_eq!(read.carets_into(&mut before).unwrap().len(), 3);
        assert_eq!(
            read.transform().apply_point(Point::ZERO),
            Some(Point::new(7, 5))
        );
        read.transform()
            .apply_point(before[0].segment()[0])
            .unwrap()
    };
    crate::ui::widgets::set_transform(
        &mut app.world,
        source,
        Transform::translate(Fixed::from_int(11), Fixed::from_int(9)),
    );
    app.render_dirty().unwrap();

    let read = outputs(&app).get(source).unwrap();
    let mut after = [PathCaretGeometry::default(); 4];
    assert_eq!(read.carets_into(&mut after).unwrap().len(), 3);
    assert_eq!(before, after);
    let screen = read.transform().apply_point(after[0].segment()[0]).unwrap();
    assert_eq!(screen.x, screen_before.x + Fixed::from_int(11));
    assert_eq!(screen.y, screen_before.y + Fixed::from_int(9));
    assert_eq!(
        read.hit_test(screen, Fixed::ZERO).unwrap().unwrap().index(),
        0
    );
}

#[crate::component]
struct GeometryMarker {
    #[depends_on(TextGeometry)]
    source: Entity,
}

const MARKER_COLOR: Color = Color::rgb(29, 191, 71);
const MARKER_BACKGROUND: Color = Color::rgb(0, 0, 0);

#[crate::view(component = GeometryMarker)]
fn geometry_marker(marker: component!(), outputs: res!(Outputs<TextGeometry>), paint: paint!()) {
    let Some(source) = outputs.get(marker.source) else {
        return;
    };
    let Some(anchor) = source.transform().apply_point(Point::ZERO) else {
        return;
    };
    paint.draw(&DrawCommand::Fill {
        area: Rect {
            x: paint.rect().x + anchor.x,
            y: paint.rect().y + anchor.y,
            w: Fixed::from_int(2),
            h: Fixed::from_int(2),
        },
        transform: paint.transform(),
        quad: None,
        color: MARKER_COLOR,
        radius: Fixed::ZERO,
        opa: 255,
    });
}

#[crate::compose]
fn ancestor_geometry_scene() {
    ui! {
        View (width: 96, height: 48) {
            View (id: "geometry_parent", width: 32, height: 24) [
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                },
            ] {
                Text (
                    "A",
                    id: "geometry_source",
                    position: Position::Absolute,
                    left: 8,
                    top: 4,
                    width: 16,
                    height: 16
                )
            }
            View (
                id: "geometry_consumer",
                position: Position::Absolute,
                left: 64,
                top: 24,
                width: 24,
                height: 16
            ) [
                GeometryMarker {
                    source: id("geometry_source"),
                },
            ]
        }
    };
}

#[test]
fn ancestor_visual_geometry_changes_repaint_separate_consumers_before_acknowledgement() {
    use crate::ui::dirty::{Dirty, VisualDirty};
    use crate::ui::render_system::LastDirtyRegions;

    for scrolls in [false, true] {
        let mut app = App::headless(96, 48);
        app.with_default_widgets()
            .with_widget(geometry_marker::view());
        let root = app.spawn_root().bg_color(MARKER_BACKGROUND).id();
        app.compose(root, ancestor_geometry_scene());
        let parent = app.world.find_by_id("geometry_parent").unwrap();
        let source = app.world.find_by_id("geometry_source").unwrap();
        let consumer = app.world.find_by_id("geometry_consumer").unwrap();
        app.render().unwrap();
        app.render_dirty().unwrap();
        assert!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .is_empty()
        );
        assert_eq!(app.backend.framebuffer().get_pixel(72, 28), MARKER_COLOR);
        let previous = version(&app, source);

        let next_x = if scrolls {
            app.world.get_mut::<ScrollOffset>(parent).unwrap().x = Fixed::from_int(4);
            app.world.invalidate_visual(parent);
            68
        } else {
            crate::ui::widgets::set_transform(
                &mut app.world,
                parent,
                Transform::translate(Fixed::from_int(4), Fixed::ZERO),
            );
            76
        };
        assert!(app.world.has::<VisualDirty>(parent));
        for unchanged in [source, consumer] {
            assert!(!app.world.has::<Dirty>(unchanged));
            assert!(!app.world.has::<VisualDirty>(unchanged));
        }

        app.render_dirty().unwrap();
        assert!(version(&app, source) > previous);
        assert_eq!(
            app.backend.framebuffer().get_pixel(next_x, 28),
            MARKER_COLOR,
            "the new publication must repaint its consumer in the same frame; scroll={scrolls}"
        );
        assert_eq!(
            app.backend.framebuffer().get_pixel(72, 28),
            MARKER_BACKGROUND,
            "the consumer's previous marker must be cleared; scroll={scrolls}"
        );
        let current = version(&app, source);
        app.render_dirty().unwrap();
        assert_eq!(version(&app, source), current);
        assert!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .is_empty()
        );
        assert_eq!(
            app.backend.framebuffer().get_pixel(next_x, 28),
            MARKER_COLOR
        );
    }
}

#[test]
fn path_caret_availability_is_published_independently_of_prepared_arena_data() {
    let (mut app, _, source) = fixture();
    let mut path = crate::render::path::Path::new();
    path.move_to(Point::new(0, 16)).line_to(Point::new(64, 16));
    let path = app.paths().insert(path).unwrap();
    app.world.insert(source, TextPath::new(path));
    app.render().unwrap();

    let outputs = outputs(&app);
    let mut record = outputs.state(source).unwrap().current.unwrap().record;
    assert!(record.caret_ready);
    assert_eq!(
        outputs
            .storage()
            .paths
            .caret_frames(record.placement.unwrap(), <[_]>::len),
        Some(3)
    );
    record.caret_ready = false;
    outputs.publish_text(source, record).unwrap();
    let unavailable_version = version(&app, source);
    let mut carets = [PathCaretGeometry::default(); 4];
    {
        let read = outputs.get(source).unwrap();
        assert_eq!(read.layout().glyphs().len(), 2);
        assert_eq!(
            read.carets_into(&mut carets),
            Err(crate::text::PathTextGeometryError::Unavailable)
        );
    }

    record.caret_ready = true;
    outputs.publish_text(source, record).unwrap();
    let available_version = version(&app, source);
    assert!(available_version > unavailable_version);
    assert_eq!(
        outputs
            .get(source)
            .unwrap()
            .carets_into(&mut carets)
            .unwrap()
            .len(),
        3
    );
    outputs.publish_text(source, record).unwrap();
    assert_eq!(version(&app, source), available_version);
}
