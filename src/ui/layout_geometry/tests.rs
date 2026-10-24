use super::*;
use crate::prelude::*;
use crate::types::{Point, Viewport};
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::WidgetTransform3D;
use crate::ui::{Hidden, Style};

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

#[crate::component]
struct GeometryReader {
    #[depends_on(LayoutGeometry)]
    source: Entity,
}

#[crate::view(component = GeometryReader)]
fn geometry_reader(reader: component!(), outputs: res!(Outputs<LayoutGeometry>)) {
    let _ = outputs.get(reader.source);
}

#[crate::compose]
fn scene() {
    ui! {
        Column (grow: 1.0) {
        View (id: "scroller", width: 96, height: 72, clip_children: true) [ScrollOffset::default()] {
            View (
                id: "source", position: Position::Absolute,
                left: 10, top: 18, width: 60, height: 40, padding: Padding::all(4)
            ) {
                View (id: "content", grow: 1.0)
            }
        }
        View (id: "reader", width: 1, height: 1) [GeometryReader { source: id("source") }]
        }
    };
}

fn fixture() -> (TestApp, Entity, Entity, Entity) {
    let mut app = App::headless(128, 128);
    app.with_default_widgets()
        .with_widget(geometry_reader::view());
    let root = app.spawn_root().id();
    app.compose(root, scene());
    let scroller = app.world.find_by_id("scroller").unwrap();
    let source = app.world.find_by_id("source").unwrap();
    (app, root, scroller, source)
}

fn outputs(app: &TestApp) -> &Outputs<LayoutGeometry> {
    app.world.resource::<Outputs<LayoutGeometry>>().unwrap()
}

#[test]
fn sparse_publication_matches_full_walk_with_live_ancestor_geometry() {
    let (mut app, root, scroller, source) = fixture();
    let content = app.world.find_by_id("content").unwrap();
    app.world
        .insert(scroller, GeometryReader { source: content });
    for _ in 0..64 {
        let entity = app.world.spawn_empty();
        app.world.insert(entity, crate::ui::Widget);
        app.world.insert(entity, crate::ui::Parent(root));
        app.world.insert(entity, Style::default());
        app.world
            .get_mut::<crate::ui::Children>(root)
            .unwrap()
            .0
            .push(entity);
    }
    app.world.invalidate(root);
    app.render().unwrap();
    let mut snapshot = app
        .world
        .take_resource_box::<crate::ui::render_system::LayoutSnapshot>()
        .unwrap();
    snapshot.prepare_geometry_plan(&mut app.world);
    assert!(snapshot.paths.selected().is_some());
    let viewport = Rect::new(0, 0, 128, 128);
    for offset in [0, 3, 17] {
        app.world.insert(
            scroller,
            ScrollOffset {
                x: offset.into(),
                y: offset.into(),
            },
        );
        app.world.insert(
            scroller,
            WidgetTransform(Transform::rotate_deg(offset.into())),
        );
        app.world.insert(
            source,
            WidgetTransform3D(Transform3D::rotate_y_perspective(offset.into(), 300.into())),
        );
        outputs(&app).reset_all();
        publish_node(
            &app.world,
            outputs(&app),
            &snapshot.layout_tree,
            Placement::root(viewport),
        );
        let expected = [source, content].map(|entity| outputs(&app).get(entity).unwrap());
        outputs(&app).reset_all();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                publish(
                    &app.world,
                    &snapshot.layout_tree,
                    &snapshot.entities,
                    &snapshot.paths,
                    viewport,
                );
            }),
            0
        );
        for (entity, expected) in [source, content].into_iter().zip(expected) {
            assert_eq!(outputs(&app).get(entity), Some(expected));
        }
        outputs(&app).reset_all();
        publish_subtree(
            &app.world,
            &snapshot.layout_tree,
            source,
            &snapshot.paths,
            viewport,
        );
        for (entity, expected) in [source, content].into_iter().zip(expected) {
            assert_eq!(outputs(&app).get(entity), Some(expected));
        }
    }
    app.world.insert(scroller, Hidden);
    publish_subtree(
        &app.world,
        &snapshot.layout_tree,
        source,
        &snapshot.paths,
        viewport,
    );
    assert!(outputs(&app).get(source).is_none());
    assert!(outputs(&app).get(content).is_none());
    app.world.remove::<Hidden>(scroller);
    app.world.put_resource_box(snapshot);
    let reader = app.world.find_by_id("reader").unwrap();
    app.world.remove::<GeometryReader>(reader);
    app.world.remove::<GeometryReader>(scroller);
    app.render().unwrap();
    assert!(
        app.world
            .resource::<crate::ui::render_system::LayoutSnapshot>()
            .unwrap()
            .paths
            .selected()
            .is_none()
    );
}

fn version(app: &TestApp, source: Entity) -> u64 {
    outputs(app).state(source).unwrap().current.unwrap().version
}

#[test]
fn declared_geometry_uses_resolved_padding_and_the_drawing_transform() {
    let (mut app, root, scroller, source) = fixture();
    assert!(app.world.resource::<Outputs<LayoutGeometry>>().is_none());
    app.world
        .insert(scroller, WidgetTransform(Transform::rotate_deg(12.into())));
    app.world.insert(
        source,
        WidgetTransform3D(Transform3D::rotate_y_perspective(20.into(), 300.into())),
    );
    app.world.insert(
        scroller,
        ScrollOffset {
            x: 3.into(),
            y: 7.into(),
        },
    );
    app.render().unwrap();
    let geometry = outputs(&app).get(source).unwrap();
    assert_eq!(geometry.border_rect(), Rect::new(0, 0, 60, 40));
    assert_eq!(geometry.content_rect(), Rect::new(4, 4, 52, 32));
    let (rect, affine, projective) =
        crate::ui::geometry::widget_render_geometry(&app.world, source).unwrap();
    let expected = if projective.is_identity() {
        Transform3D::from(affine)
    } else {
        projective
    };
    assert_eq!(
        geometry.transform(),
        expected.compose(&Transform3D::translate(rect.x, rect.y))
    );
    let (rect, affine, projective) =
        crate::ui::geometry::widget_render_geometry(&app.world, scroller).unwrap();
    let clip = crate::ui::geometry::transformed_clip_bounds(rect, affine, projective).unwrap();
    assert_eq!(
        geometry.clip(),
        clip.intersect(&Rect::new(0, 0, 128, 128)).unwrap()
    );
    assert!(!outputs(&app).is_requested(root));
    assert!(outputs(&app).get(root).is_none());
    assert!(outputs(&app).state(root).is_none());
}

#[test]
fn visual_input_preparation_updates_ancestor_scroll_and_clips_before_paint() {
    let (mut app, root, scroller, source) = fixture();
    app.render().unwrap();
    app.render_dirty().unwrap();
    let before = version(&app, source);
    app.world.insert(
        scroller,
        ScrollOffset {
            x: 0.into(),
            y: 13.into(),
        },
    );
    app.world.invalidate_visual(scroller);
    crate::ui::render_system::prepare_input_geometry(
        &mut app.world,
        root,
        &Viewport::new(128, 128, Fixed::ONE),
    )
    .unwrap();
    let geometry = outputs(&app).get(source).unwrap();
    assert_eq!(
        geometry.transform().apply_point(Point::new(0, 0)),
        Some(Point::new(10, 5))
    );
    assert!(version(&app, source) > before);
    assert_eq!(geometry.clip(), Rect::new(0, 0, 96, 72));

    app.world.insert(
        scroller,
        WidgetTransform(Transform::translate(8.into(), 3.into())),
    );
    app.world.invalidate_visual(scroller);
    crate::ui::render_system::prepare_input_geometry(
        &mut app.world,
        root,
        &Viewport::new(128, 128, Fixed::ONE),
    )
    .unwrap();
    let geometry = outputs(&app).get(source).unwrap();
    assert_eq!(
        geometry.transform().apply_point(Point::new(0, 0)),
        Some(Point::new(18, 8))
    );
    assert_eq!(geometry.clip(), Rect::new(8, 3, 96, 72));
}

#[test]
fn unchanged_geometry_keeps_publication_identity_and_steady_preparation_does_not_allocate() {
    let (mut app, root, scroller, source) = fixture();
    app.render().unwrap();
    app.render_dirty().unwrap();
    let before = version(&app, source);
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    crate::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(version(&app, source), before);
    app.render_dirty().unwrap();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for offset in 0..8 {
                app.world.insert(
                    scroller,
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: offset.into(),
                    },
                );
                app.world.invalidate_visual(scroller);
                crate::ui::render_system::prepare_input_geometry(&mut app.world, root, &viewport)
                    .unwrap();
                assert!(outputs(&app).get(source).is_some());
            }
        }),
        0
    );
    let changed = version(&app, source);
    app.render_dirty().unwrap();
    app.world.invalidate_visual(source);
    app.render_dirty().unwrap();
    assert_eq!(version(&app, source), changed);
}

#[test]
fn subtree_publication_matches_full_placement_and_retires_hidden_descendants() {
    let (mut app, root, scroller, source) = fixture();
    let content = app.world.find_by_id("content").unwrap();
    app.world.insert(root, GeometryReader { source: scroller });
    app.world
        .insert(scroller, GeometryReader { source: content });
    app.world
        .insert(scroller, WidgetTransform(Transform::rotate_deg(12.into())));
    app.world.insert(
        source,
        WidgetTransform3D(Transform3D::rotate_y_perspective(20.into(), 300.into())),
    );
    app.world.insert(
        scroller,
        ScrollOffset {
            x: 3.into(),
            y: 7.into(),
        },
    );
    app.render().unwrap();
    app.world.remove::<crate::ui::Parent>(source);
    app.world.remove::<crate::ui::Parent>(content);
    let viewport = Rect::new(0, 0, 128, 128);
    let mut snapshot = app
        .world
        .take_resource_box::<crate::ui::render_system::LayoutSnapshot>()
        .unwrap();
    snapshot.prepare_geometry_plan(&mut app.world);
    publish(
        &app.world,
        &snapshot.layout_tree,
        &snapshot.entities,
        &snapshot.paths,
        viewport,
    );
    let expected = [source, content].map(|entity| outputs(&app).get(entity).unwrap());
    for entity in [scroller, source, content] {
        outputs(&app).invalidate(entity);
    }
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            publish_subtree(
                &app.world,
                &snapshot.layout_tree,
                source,
                &snapshot.paths,
                viewport,
            );
        }),
        0
    );
    for (entity, expected) in [source, content].into_iter().zip(expected) {
        assert_eq!(outputs(&app).get(entity), Some(expected));
    }
    assert!(outputs(&app).get(scroller).is_none());

    app.world.insert(source, Hidden);
    publish_subtree(
        &app.world,
        &snapshot.layout_tree,
        source,
        &snapshot.paths,
        viewport,
    );
    assert!(outputs(&app).get(source).is_none());
    assert!(outputs(&app).get(content).is_none());
    app.world.remove::<Hidden>(source);
    publish_subtree(
        &app.world,
        &snapshot.layout_tree,
        source,
        &snapshot.paths,
        viewport,
    );
    for (entity, expected) in [source, content].into_iter().zip(expected) {
        assert_eq!(outputs(&app).get(entity), Some(expected));
    }
    app.world.insert(scroller, Hidden);
    publish_subtree(
        &app.world,
        &snapshot.layout_tree,
        source,
        &snapshot.paths,
        viewport,
    );
    assert!(outputs(&app).get(source).is_none());
    assert!(outputs(&app).get(content).is_none());
    app.world.remove::<Hidden>(scroller);
    publish_subtree(
        &app.world,
        &snapshot.layout_tree,
        source,
        &snapshot.paths,
        viewport,
    );
    assert_eq!(outputs(&app).get(source), Some(expected[0]));
    app.world.put_resource_box(snapshot);
}

#[test]
fn hidden_detached_and_replaced_sources_do_not_leave_readable_records() {
    let (mut app, root, scroller, source) = fixture();
    app.render().unwrap();
    app.world.insert(scroller, Hidden);
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_none());
    app.world.remove::<Hidden>(scroller);
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_some());
    app.world
        .get_mut::<crate::ui::Children>(scroller)
        .unwrap()
        .0
        .clear();
    app.world.invalidate(scroller);
    app.render_dirty().unwrap();
    assert!(outputs(&app).get(source).is_none());
    app.world.despawn(source);
    let replacement = app.world.spawn_empty();
    app.world.insert(replacement, crate::ui::Widget);
    app.world.insert(replacement, Style::default());
    crate::ui::render_system::try_update_layout(
        &mut app.world,
        root,
        &Viewport::new(128, 128, Fixed::ONE),
    )
    .unwrap();
    assert!(outputs(&app).get(source).is_none());
    assert!(outputs(&app).get(replacement).is_none());
}

#[test]
fn hidden_root_invalidates_geometry_at_the_input_boundary() {
    let (mut app, root, _, source) = fixture();
    app.render().unwrap();
    app.world.insert(root, Hidden);
    crate::ui::render_system::prepare_input_geometry(
        &mut app.world,
        root,
        &Viewport::new(128, 128, Fixed::ONE),
    )
    .unwrap();
    assert!(outputs(&app).get(source).is_none());
}

#[test]
fn geometry_content_box_matches_flex_children_and_safe_area() {
    let (mut app, root, _, source) = fixture();
    let reader = app.world.find_by_id("reader").unwrap();
    app.world.get_mut::<GeometryReader>(reader).unwrap().source = root;
    app.world.insert(root, crate::ui::RespectSafeArea);
    app.render().unwrap();
    app.world.insert_resource(crate::surface::SafeAreaInsets {
        top: 6.into(),
        right: 7.into(),
        bottom: 8.into(),
        left: 9.into(),
    });
    crate::ui::render_system::try_update_layout(
        &mut app.world,
        root,
        &Viewport::new(128, 128, Fixed::ONE),
    )
    .unwrap();
    assert_eq!(
        outputs(&app).get(root).unwrap().content_rect(),
        Rect::new(9, 6, 112, 114)
    );
    app.world.get_mut::<GeometryReader>(reader).unwrap().source = source;
    let padding = &mut app.world.get_mut::<Style>(source).unwrap().layout.padding;
    padding.left = Dimension::Percent(25.into());
    padding.top = Dimension::Percent(50.into());
    app.render_dirty().unwrap();
    let geometry = outputs(&app).get(source).unwrap();
    let source_rect = app.world.get::<crate::ui::ComputedRect>(source).unwrap().0;
    let child = app.world.find_by_id("content").unwrap();
    let child_rect = app.world.get::<crate::ui::ComputedRect>(child).unwrap().0;
    assert_eq!(
        geometry.content_rect(),
        Rect::new(
            child_rect.x - source_rect.x,
            child_rect.y - source_rect.y,
            child_rect.w,
            child_rect.h
        )
    );
}
