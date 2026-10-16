use mirui::prelude::*;
use mirui::surface::FramebufferAccess;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

#[model]
struct Scene {
    #[observe]
    left: i32,
    #[observe]
    width: i32,
    #[observe]
    visible: bool,
}

#[model]
impl Scene {
    fn change(&mut self, left: i32, width: i32, visible: bool) {
        self.left = left;
        self.width = width;
        self.visible = visible;
    }
}

#[compose]
fn panel(scene: model!(Scene)) {
    ui! {
        if ${ scene.visible() } {
            View (
                position: Position::Absolute,
                left: ${ scene.left() },
                top: 8,
                width: ${ scene.width() },
                height: 12,
                bg_color: Color::rgb(20, 200, 80)
            )
        }
    };
}

fn render_sequence(queries: usize) -> Vec<u8> {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let scene = app
        .add_model(Scene {
            left: 8,
            width: 12,
            visible: true,
        })
        .finish();
    app.compose(root, panel(scene.clone()));
    app.render().unwrap();
    let painted = app
        .backend
        .framebuffer()
        .read_pixels()
        .expect("read pixels")
        .bytes()
        .to_vec();
    for (left, width, visible) in [(28, 10, true), (4, 6, false), (24, 8, true), (18, 14, true)] {
        scene.change(left, width, visible);
        for _ in 0..queries {
            let union = app.dirty_region().unwrap();
            assert_eq!(app.dirty_regions().unwrap().bounding_rect(), union);
            assert!(
                app.backend
                    .framebuffer()
                    .read_pixels()
                    .expect("read pixels")
                    .bytes()
                    == painted
            );
        }
    }
    app.render_dirty().unwrap();
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .expect("read pixels")
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert!(
        app.backend
            .framebuffer()
            .read_pixels()
            .expect("read pixels")
            .bytes()
            == pixels
    );
    assert!(app.dirty_regions().unwrap().is_empty());
    pixels
}

#[test]
fn model_and_cached_branch_updates_are_independent_of_query_count() {
    let reference = render_sequence(0);
    for queries in [1, 3] {
        assert!(render_sequence(queries) == reference);
    }
}

#[test]
fn retained_query_is_a_snapshot_while_model_changes_remain_pending() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let scene = app
        .add_model(Scene {
            left: 8,
            width: 12,
            visible: true,
        })
        .finish();
    app.compose(root, panel(scene.clone()));
    app.render().unwrap();
    scene.change(24, 8, true);
    let plan = app.dirty_regions().unwrap();
    let before = plan.bounding_rect();
    scene.change(40, 12, true);
    assert_eq!(plan.bounding_rect(), before);
    let current = app.dirty_region().unwrap().unwrap();
    assert!(current.x + current.w >= Fixed::from_int(52));
    let allocations = tracking_allocator::tracked_allocations(|| {
        for _ in 0..16 {
            assert_eq!(app.dirty_region().unwrap(), Some(current));
        }
    });
    assert_eq!(allocations, 0);
    app.render_dirty().unwrap();
    assert_eq!(
        app.backend
            .framebuffer()
            .read_pixels()
            .expect("read pixels")
            .get_pixel(45, 12),
        Color::rgb(20, 200, 80)
    );
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .expect("read pixels")
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert!(
        app.backend
            .framebuffer()
            .read_pixels()
            .expect("read pixels")
            .bytes()
            == pixels
    );
}
