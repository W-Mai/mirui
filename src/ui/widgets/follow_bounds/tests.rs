use super::*;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::{Transform, Viewport};
use crate::ui::widgets::WidgetTransform;
use crate::ui::{Hidden, OffscreenRender, Style};

pub(super) type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;
const BLUE: Color = Color::rgb(20, 80, 240);

#[crate::compose]
fn scene() {
    ui! {
        View (width: 128, height: 96) {
            View (id: "scroller", position: Position::Absolute, left: 12, top: 8,
                width: 40, height: 32, clip_children: true) [ScrollOffset::default()] {
                View (id: "source", position: Position::Absolute, left: 4, top: 4,
                    width: 30, height: 20, padding: Padding::all(3))
            }
            View (id: "host", position: Position::Absolute, left: 70, top: 14,
                width: 50, height: 60) {
                View (id: "follower", width: 7, height: 5, bg_color: BLUE) [
                    FollowBounds::new(id("source")), HitTarget,
                ] {
                    View (id: "fill", width: Dimension::percent(100), height: Dimension::percent(100))
                }
                View (id: "marker", width: 4, height: 4)
            }
        }
    };
}

pub(super) fn fixture() -> (TestApp, Entity, Entity, Entity) {
    let mut app = App::headless(128, 96);
    app.with_default_widgets()
        .with_offscreen_pool_budget(128 * 96 * 12);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, scene());
    let source = app.world.find_by_id("source").unwrap();
    let follower = app.world.find_by_id("follower").unwrap();
    (app, root, source, follower)
}

pub(super) fn rect(app: &TestApp, entity: Entity) -> Rect {
    app.world.get::<ComputedRect>(entity).unwrap().0
}

#[test]
fn follows_across_parents_without_changing_authored_style_or_flex_siblings() {
    let (mut app, _, source, follower) = fixture();
    app.render().unwrap();
    assert_eq!(rect(&app, source), rect(&app, follower));
    assert_eq!(rect(&app, follower), Rect::new(16, 12, 30, 20));
    assert_eq!(
        rect(&app, app.world.find_by_id("fill").unwrap()),
        rect(&app, follower)
    );
    let marker = app.world.find_by_id("marker").unwrap();
    assert_eq!(rect(&app, marker).x, 70.into());
    let style = app.world.get::<Style>(follower).unwrap();
    assert_eq!(style.layout.width, Dimension::px(7));
    assert_eq!(style.layout.position, Position::Flex);

    *app.world.get_mut::<FollowBounds>(follower).unwrap() = FollowBounds::new(source)
        .with_content(true)
        .with_outset(2)
        .with_offset(1, -1);
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert_eq!(rect(&app, follower), Rect::new(18, 12, 28, 18));
    app.world.remove::<FollowBounds>(follower);
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert_eq!(rect(&app, follower), Rect::new(70, 14, 7, 5));
    assert_eq!(rect(&app, marker).x, 77.into());
}

#[test]
fn scroll_updates_following_geometry_and_source_clip_before_input() {
    let (mut app, root, _, follower) = fixture();
    app.render().unwrap();
    let scroller = app.world.find_by_id("scroller").unwrap();
    app.world.get_mut::<ScrollOffset>(scroller).unwrap().y = 14.into();
    app.world.invalidate_visual(scroller);
    crate::ui::render_system::prepare_input_geometry(
        &mut app.world,
        root,
        &Viewport::new(128, 96, Fixed::ONE),
    )
    .unwrap();
    assert_eq!(rect(&app, follower), Rect::new(16, -2, 30, 20));
    assert_eq!(clip(&app.world, follower), Some(Rect::new(12, 8, 40, 32)));
    assert_ne!(
        crate::input::event::hit_test::hit_test(&app.world, root, 20.into(), 4.into(), 128, 96),
        Some(follower)
    );
    assert_eq!(
        crate::input::event::hit_test::hit_test(&app.world, root, 20.into(), 10.into(), 128, 96),
        Some(follower)
    );
    app.render_dirty().unwrap();
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.get_pixel(20, 4), Color::rgb(0, 0, 0));
    assert_eq!(pixels.get_pixel(20, 10), BLUE);
}

#[test]
fn hidden_deleted_and_cyclic_sources_cannot_leave_visible_followers() {
    let (mut app, _, source, follower) = fixture();
    app.render().unwrap();
    app.world.insert(source, Hidden);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, follower));
    assert_eq!(
        app.backend.framebuffer().get_pixel(20, 16),
        Color::rgb(0, 0, 0)
    );
    app.world.remove::<Hidden>(source);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert!(!hidden(&app.world, follower));
    app.world.get_mut::<FollowBounds>(follower).unwrap().target = follower;
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, follower));
    app.world.get_mut::<FollowBounds>(follower).unwrap().target = source;
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert!(!hidden(&app.world, follower));
    app.world.despawn(source);
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, follower));
}

#[test]
fn transformed_offscreen_followers_keep_source_clipping() {
    let (mut app, _, _, follower) = fixture();
    let host = app.world.find_by_id("host").unwrap();
    app.world.insert(host, OffscreenRender::new());
    app.world.insert(
        host,
        crate::ui::widgets::WidgetTransform(crate::types::Transform::rotate_deg(20.into())),
    );
    let scroller = app.world.find_by_id("scroller").unwrap();
    app.world.get_mut::<ScrollOffset>(scroller).unwrap().y = 14.into();
    app.render().unwrap();
    assert!(!hidden(&app.world, follower));
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.get_pixel(20, 4), Color::rgb(0, 0, 0));
    assert_eq!(pixels.get_pixel(20, 12), BLUE);
}

#[test]
fn long_reverse_dependency_chains_resolve_in_one_frame_and_recover_from_cycles() {
    let mut app: TestApp = App::headless(128, 96);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let (source, followers) = app.compose(root, |tree| {
        let source = tree.spawn(Style {
            layout: LayoutStyle {
                width: Dimension::px(12),
                height: Dimension::px(9),
                ..LayoutStyle::default()
            },
            ..Style::default()
        });
        let followers = [(); 40].map(|()| tree.spawn(Style::default()));
        (source, followers)
    });
    for (index, &entity) in followers.iter().enumerate() {
        let target = followers.get(index + 1).copied().unwrap_or(source);
        app.world.insert(entity, FollowBounds::new(target));
    }
    app.render().unwrap();
    for &follower in &followers {
        assert_eq!(rect(&app, follower), rect(&app, source));
    }
    app.world
        .get_mut::<FollowBounds>(followers[1])
        .unwrap()
        .target = followers[0];
    app.world.invalidate(followers[1]);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, followers[0]) && hidden(&app.world, followers[1]));
    assert!(!hidden(&app.world, followers[2]));
    app.world
        .get_mut::<FollowBounds>(followers[1])
        .unwrap()
        .target = followers[2];
    app.world.invalidate(followers[1]);
    app.render_dirty().unwrap();
    for &follower in &followers {
        assert!(!hidden(&app.world, follower));
    }
}

#[test]
fn prepared_motion_reuses_storage_and_dirty_frames_match_full_frames() {
    let (mut app, _, source, follower) = fixture();
    app.render().unwrap();
    for _ in 0..3 {
        app.render_dirty().unwrap();
    }
    let allocations = crate::test_support::tracked_allocations(|| {
        for x in [6, 10, 4, 8, 4] {
            app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(x);
            app.world.invalidate(source);
            app.render_dirty().unwrap();
            assert_eq!(rect(&app, follower), rect(&app, source));
        }
    });
    assert_eq!(allocations, 0);
    for x in [11, -10, 4] {
        app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(x);
        app.world.invalidate(source);
        app.render_dirty().unwrap();
        let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(app.backend.framebuffer().buf.as_slice(), dirty);
    }
}

#[test]
fn unavailable_followers_revoke_captured_outputs_and_recover() {
    use crate::ui::WidgetTexture;
    let (mut app, _, source, follower) = fixture();
    app.world.insert(follower, OffscreenRender::new());
    app.render().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(follower)
            .is_some()
    );
    app.world.insert(source, Hidden);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(follower)
            .is_none()
    );
    app.world.remove::<Hidden>(source);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(follower)
            .is_some()
    );
}

#[test]
fn descendant_targets_and_singular_parent_transforms_suspend_the_relation() {
    let (mut app, _, source, follower) = fixture();
    let child = app.world.find_by_id("fill").unwrap();
    let host = app.world.find_by_id("host").unwrap();
    app.render().unwrap();
    app.world.get_mut::<FollowBounds>(follower).unwrap().target = child;
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, follower));
    app.world.get_mut::<FollowBounds>(follower).unwrap().target = source;
    app.world.insert(
        host,
        WidgetTransform(Transform::scale(Fixed::ZERO, Fixed::ONE)),
    );
    app.world.invalidate_visual(host);
    app.render_dirty().unwrap();
    assert!(hidden(&app.world, follower));
    app.world.remove::<WidgetTransform>(host);
    app.world.invalidate_visual(host);
    app.render_dirty().unwrap();
    assert!(!hidden(&app.world, follower));
}
