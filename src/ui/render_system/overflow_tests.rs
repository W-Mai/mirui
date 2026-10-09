use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Transform;
use crate::ui::ComputedRect;
use crate::ui::widgets::WidgetTransform;

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

const BLACK: Color = Color::rgb(0, 0, 0);
const RED: Color = Color::rgb(229, 47, 23);
const BLUE: Color = Color::rgb(23, 47, 229);
const GREEN: Color = Color::rgb(29, 191, 71);
const YELLOW: Color = Color::rgb(223, 191, 29);

fn app(width: u16, height: u16) -> (TestApp, Entity) {
    let mut app = App::headless(width, height);
    app.with_default_widgets();
    let root = app.spawn_root().bg_color(BLACK).id();
    (app, root)
}

fn pixel(app: &mut TestApp, x: i32, y: i32, expected: Color) {
    assert_eq!(
        app.backend.framebuffer().get_pixel(x, y),
        expected,
        "pixel ({x}, {y})"
    );
}

fn set_color(app: &mut TestApp, entity: Entity, color: Color) {
    app.world.get_mut::<Style>(entity).unwrap().bg_color = Some(color.into());
}

fn assert_damage_excludes(app: &TestApp, parent_bounds: Rect) {
    let damage = &app.world.resource::<super::LastDirtyRegions>().unwrap().0;
    assert!(!damage.rects.is_empty());
    assert!(damage.shifts.is_empty());
    assert!(
        damage
            .rects
            .iter()
            .all(|rect| rect.intersect(&parent_bounds).is_none()),
        "the repaint must exercise a child outside its parent's paint bounds"
    );
}

#[crate::compose]
fn overflow_scene(parent_x: i32, child_x: i32) {
    ui! {
        View (
            id: "parent",
            position: Position::Absolute,
            left: parent_x,
            top: 2,
            width: 4,
            height: 4,
            bg_color: GREEN
        ) {
            View (
                id: "child",
                position: Position::Absolute,
                left: child_x,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
        }
    };
}

#[test]
fn full_render_reaches_visible_overflow_children_of_an_offscreen_parent() {
    let (mut app, root) = app(16, 8);
    app.compose(root, overflow_scene(20, -16));
    let parent = app.world.find_by_id("parent").unwrap();
    let child = app.world.find_by_id("child").unwrap();
    app.render().unwrap();

    let screen = Rect::new(0, 0, 16, 8);
    assert_eq!(
        app.world.get::<ComputedRect>(parent).unwrap().0,
        Rect::new(20, 2, 4, 4)
    );
    assert!(
        app.world
            .get::<ComputedRect>(parent)
            .unwrap()
            .0
            .intersect(&screen)
            .is_none()
    );
    assert_eq!(
        app.world.get::<ComputedRect>(child).unwrap().0,
        Rect::new(4, 2, 4, 4)
    );
    pixel(&mut app, 5, 3, RED);
    pixel(&mut app, 1, 3, BLACK);
    pixel(&mut app, 9, 3, BLACK);
}

#[test]
fn dirty_render_reaches_overflow_children_when_the_parent_misses_all_damage() {
    let (mut app, root) = app(16, 8);
    app.compose(root, overflow_scene(0, 8));
    let child = app.world.find_by_id("child").unwrap();
    app.render().unwrap();
    pixel(&mut app, 1, 3, GREEN);
    pixel(&mut app, 9, 3, RED);

    set_color(&mut app, child, BLUE);
    app.render_dirty().unwrap();
    assert_damage_excludes(&app, Rect::new(0, 2, 4, 4));
    pixel(&mut app, 9, 3, BLUE);
    pixel(&mut app, 1, 3, GREEN);
    pixel(&mut app, 5, 3, BLACK);
}

#[crate::compose]
fn clipping_scene(clip: bool) {
    ui! {
        View (
            id: "parent",
            position: Position::Absolute,
            left: 2,
            top: 2,
            width: 4,
            height: 4,
            clip_children: clip
        ) {
            View (
                id: "first",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "second",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: BLUE
            )
        }
    };
}

#[test]
fn explicit_clipping_and_scroll_viewports_still_clip_full_and_dirty_children() {
    for scrolls in [false, true] {
        let (mut app, root) = app(16, 8);
        app.compose(root, clipping_scene(!scrolls));
        let parent = app.world.find_by_id("parent").unwrap();
        let first = app.world.find_by_id("first").unwrap();
        let second = app.world.find_by_id("second").unwrap();
        if scrolls {
            app.world.insert(
                parent,
                ScrollOffset {
                    x: Fixed::from_int(2),
                    y: Fixed::ZERO,
                },
            );
        }
        app.render().unwrap();
        pixel(&mut app, 3, 3, RED);
        pixel(&mut app, 5, 3, if scrolls { BLUE } else { RED });
        pixel(&mut app, 1, 3, BLACK);
        pixel(&mut app, 7, 3, BLACK);

        set_color(&mut app, first, GREEN);
        set_color(&mut app, second, YELLOW);
        app.render_dirty().unwrap();
        pixel(&mut app, 3, 3, GREEN);
        pixel(&mut app, 5, 3, if scrolls { YELLOW } else { GREEN });
        pixel(&mut app, 1, 3, BLACK);
        pixel(&mut app, 7, 3, BLACK);
    }
}

#[crate::compose]
fn rotated_overflow_scene() {
    ui! {
        View (
            id: "parent",
            position: Position::Absolute,
            left: 4,
            top: 4,
            width: 4,
            height: 4,
            bg_color: GREEN
        ) [
            WidgetTransform(Transform {
                m00: Fixed::ZERO,
                m01: Fixed::from_int(-1),
                tx: Fixed::ZERO,
                m10: Fixed::ONE,
                m11: Fixed::ZERO,
                ty: Fixed::ZERO,
            }),
        ] {
            View (
                id: "child",
                position: Position::Absolute,
                left: 4,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
        }
    };
}

#[test]
fn rotated_overflow_keeps_the_parent_transform_when_only_the_child_is_dirty() {
    let (mut app, root) = app(16, 16);
    app.compose(root, rotated_overflow_scene());
    let child = app.world.find_by_id("child").unwrap();
    app.render().unwrap();
    pixel(&mut app, 5, 5, GREEN);
    pixel(&mut app, 5, 9, RED);
    pixel(&mut app, 9, 5, BLACK);

    set_color(&mut app, child, BLUE);
    app.render_dirty().unwrap();
    assert_damage_excludes(&app, Rect::new(4, 4, 4, 4));
    pixel(&mut app, 5, 9, BLUE);
    pixel(&mut app, 5, 5, GREEN);
    pixel(&mut app, 9, 5, BLACK);
}
