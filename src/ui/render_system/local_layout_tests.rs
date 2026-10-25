extern crate std;

use super::*;
use crate::input::event::scroll::{ScrollBounds, ScrollOffset};
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::layout::Position;
use crate::ui::widgets::Text;
use crate::ui::{ComputedRect, RespectSafeArea};
use std::cell::Cell;
use std::rc::Rc;

std::thread_local! {
    static SOLVES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    static PREPARATIONS: Cell<usize> = const { Cell::new(0) };
    static INPUT_VISITS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

pub(super) fn record_input_visit(text: bool) {
    INPUT_VISITS.with(|visits| {
        let (a, b) = visits.get();
        visits.set((a + usize::from(!text), b + usize::from(text)));
    });
}

pub(super) fn record_preparation() {
    PREPARATIONS.with(|count| count.set(count.get() + 1));
}

pub(super) fn record_solve(local: bool) {
    SOLVES.with(|counts| {
        let (a, b) = counts.get();
        counts.set((a + usize::from(local), b + usize::from(!local)));
    });
}

fn counts() -> (usize, usize) {
    SOLVES.with(|counts| counts.replace((0, 0)))
}

#[test]
fn local_scope_preserves_geometry_hits_scroll_events_and_dirty_pixels() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    app.compose(root, |cx| {
        ui! {
            View (
                id: "parent",
                width: 100,
                height: 70,
                padding: Padding::all(7),
                clip_children: true
            ) [
                ScrollOffset::default(),
            ] {
                View (
                    id: "leaf",
                    width: 20,
                    height: 10,
                    position: Position::Absolute,
                    left: 5,
                    top: 6,
                    bg_color: Color::rgb(180, 20, 10)
                ) on Tap { } on SizeChanged { observed.set(observed.get() + 1); }
                View (id: "sibling", width: 10, height: 10, bg_color: Color::rgb(0, 30, 180))
            }
        };
    });
    app.render().unwrap();
    let leaf = app.world.find_by_id("leaf").unwrap();
    let parent = app.world.find_by_id("parent").unwrap();
    let sibling = app.world.find_by_id("sibling").unwrap();
    let sibling_rect = app.world.get::<ComputedRect>(sibling).unwrap().0;
    counts();
    for (width, top) in [(30, 8), (35, 82), (45, 12)] {
        let calls_before = calls.get();
        PREPARATIONS.with(|count| count.set(0));
        let style = app.world.get_mut::<Style>(leaf).unwrap();
        style.layout.width = Dimension::px(width);
        style.layout.top = Dimension::px(top);
        app.render_dirty().unwrap();
        assert!(PREPARATIONS.with(|count| count.get()) > 0);
        assert!(
            counts().0 > 0,
            "the common render path must actually admit a leaf solve"
        );
        assert_eq!(calls.get(), calls_before + 1);
        let rect = app.world.get::<ComputedRect>(leaf).unwrap().0;
        assert_eq!(rect, Rect::new(5, top, width, 10));
        assert_eq!(
            app.world.get::<ComputedRect>(sibling).unwrap().0,
            sibling_rect
        );
        let bounds = *app.world.get::<ScrollBounds>(parent).unwrap();
        let hit = crate::input::event::hit_test::hit_test(
            &app.world,
            root,
            6.into(),
            (top + 1).into(),
            120,
            90,
        );
        if top < 60 {
            assert_eq!(hit, Some(leaf));
        }
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.world
            .resource_mut::<LayoutSnapshot>()
            .unwrap()
            .invalidate();
        app.render().unwrap();
        assert_eq!(app.world.get::<ComputedRect>(leaf).unwrap().0, rect);
        assert_eq!(*app.world.get::<ScrollBounds>(parent).unwrap(), bounds);
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            dirty
        );
        assert_eq!(
            calls.get(),
            calls_before + 1,
            "full publication must agree on size"
        );
        counts();
    }
}

#[test]
fn structural_viewport_pending_and_flex_changes_force_complete_solves() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            View (id: "leaf", width: 20, height: 10, position: Position::Absolute)
        }
    });
    app.render().unwrap();
    let leaf = app.world.find_by_id("leaf").unwrap();
    for case in 0..7 {
        counts();
        let style = app.world.get_mut::<Style>(leaf).unwrap();
        style.layout.width = Dimension::px(21 + case);
        match case {
            0 => {
                app.world
                    .resource_mut::<LayoutSnapshot>()
                    .unwrap()
                    .bindings_pending = true;
            }
            1 => {
                app.world.resource_mut::<LayoutSnapshot>().unwrap().valid = false;
            }
            2 => {
                app.world.get_mut::<Style>(root).unwrap().layout.padding = Padding::all(3);
            }
            3 => {
                app.world.get_mut::<Style>(leaf).unwrap().layout.position = Position::Flex;
            }
            4 => {
                app.world.get_mut::<Style>(leaf).unwrap().layout.position = Position::Absolute;
            }
            5 => {
                app.world.insert(leaf, RespectSafeArea);
                app.world
                    .insert_resource(crate::surface::SafeAreaInsets::default());
                app.compose(leaf, |cx| {
                    ui! {
                        View (width: 2, height: 2)
                    };
                });
            }
            _ => {
                app.world
                    .resource_mut::<LayoutSnapshot>()
                    .unwrap()
                    .logical_w = 119;
            }
        }
        app.render_dirty().unwrap();
        let (local, full) = counts();
        assert_eq!(local, 0, "case {case}");
        assert!(full > 0, "case {case}");
    }
}

#[test]
fn constrained_text_keeps_the_complete_second_solve() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Text (
                "ab cd ef gh",
                id: "text",
                width: 100,
                height: Dimension::Content,
                position: Position::Absolute,
                font_size: 8
            )
        };
    });
    app.render().unwrap();
    let text = app.world.find_by_id("text").unwrap();
    counts();
    app.world.get_mut::<Style>(text).unwrap().layout.width = Dimension::px(16);
    app.render_dirty().unwrap();
    let (local, full) = counts();
    assert!(local > 0);
    assert!(
        full > 0,
        "constrained intrinsic changes require a complete solve"
    );
    let rect = app.world.get::<ComputedRect>(text).unwrap().0;
    assert!(rect.h > Fixed::from_int(8));
    let handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(text)
        .unwrap();
    let lines = app
        .world
        .resource::<Outputs<TextGeometry>>()
        .unwrap()
        .storage()
        .layouts
        .get(handle)
        .unwrap()
        .lines()
        .len();
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.world
        .resource_mut::<LayoutSnapshot>()
        .unwrap()
        .invalidate();
    app.render().unwrap();
    assert_eq!(app.world.get::<ComputedRect>(text).unwrap().0, rect);
    let handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(text)
        .unwrap();
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .get(handle)
            .unwrap()
            .lines()
            .len(),
        lines
    );
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );
}

#[test]
fn content_box_changes_keep_follower_publication() {
    use crate::ui::widgets::FollowBounds;

    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            View (grow: 1.0) {
                View (
                    id: "source",
                    width: 40,
                    height: 30,
                    position: Position::Absolute,
                    left: 5,
                    top: 6,
                    bg_color: Color::rgb(30, 40, 100)
                ) [
                    RespectSafeArea,
                ]
                View (id: "follower", border_width: 1, border_color: Color::rgb(0, 180, 20)) [
                    FollowBounds::new(id("source")).with_content(true),
                ]
            }
        };
    });
    app.render().unwrap();
    let source = app.world.find_by_id("source").unwrap();
    let follower = app.world.find_by_id("follower").unwrap();
    for left in [3, 7, 1] {
        counts();
        let style = app.world.get_mut::<Style>(source).unwrap();
        style.layout.left = Dimension::px(5 + left);
        style.layout.padding.left = Dimension::px(left);
        app.render_dirty().unwrap();
        assert!(counts().0 > 0);
        let rect = app.world.get::<ComputedRect>(follower).unwrap().0;
        assert_eq!(rect, Rect::new(5 + left * 2, 6, 40 - left, 30));
        let pixels = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.world
            .resource_mut::<LayoutSnapshot>()
            .unwrap()
            .invalidate();
        app.render().unwrap();
        assert_eq!(app.world.get::<ComputedRect>(follower).unwrap().0, rect);
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            pixels
        );
    }
}

#[test]
fn effective_safe_area_inputs_match_complete_layout() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.world.remove::<RespectSafeArea>(root);
    app.compose(root, |cx| {
        ui! {
            View (id: "leaf", width: 40, height: 30, position: Position::Absolute) [
                RespectSafeArea,
            ]
        }
    });
    app.render().unwrap();
    let leaf = app.world.find_by_id("leaf").unwrap();
    counts();
    app.world.insert_resource(crate::surface::SafeAreaInsets {
        left: 7.into(),
        ..Default::default()
    });
    let viewport = Viewport::new(120, 90, Fixed::ONE);
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert!(counts().0 > 0);
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let content = snapshot.layout_tree.children[0].content_rect();
    assert_eq!(content, Rect::new(7, 0, 33, 30));
    let rect = app.world.get::<ComputedRect>(leaf).unwrap().0;
    app.world
        .resource_mut::<LayoutSnapshot>()
        .unwrap()
        .invalidate();
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(app.world.get::<ComputedRect>(leaf).unwrap().0, rect);
    assert_eq!(
        app.world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .layout_tree
            .children[0]
            .content_rect(),
        content
    );
}

#[test]
fn incremental_leaf_prepares_only_its_inputs_and_keeps_untouched_text_alive() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            View (grow: 1.0) {
                Text ("unchanged", id: "label", width: 100, height: 10, font_size: 8)
                View (
                    id: "leaf",
                    width: 12,
                    height: 10,
                    position: Position::Absolute,
                    bg_color: Color::rgb(200, 20, 40)
                )
                walk 0..32 with _slot {
                    View (width: 1, height: 1)
                }
            }
        }
    });
    app.render().unwrap();
    let leaf = app.world.find_by_id("leaf").unwrap();
    let label = app.world.find_by_id("label").unwrap();
    let handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(label)
        .unwrap();
    for x in 1..12 {
        INPUT_VISITS.with(|visits| visits.set((0, 0)));
        app.world.get_mut::<Style>(leaf).unwrap().layout.left = Dimension::px(x);
        assert_eq!(
            crate::test_support::tracked_allocations(|| app.render_dirty().unwrap()),
            0
        );
        assert_eq!(INPUT_VISITS.with(|visits| visits.get()), (1, 1));
        assert_eq!(
            app.world.get::<crate::text::TextLayoutHandle>(label),
            Some(&handle)
        );
        assert!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .layouts
                .get(handle)
                .is_some()
        );
    }
    for x in [18, 19, 20] {
        app.world.get_mut::<Style>(leaf).unwrap().layout.left = Dimension::px(x);
        app.dirty_regions().unwrap();
    }
    app.world.get_mut::<Style>(leaf).unwrap().layout.left = Dimension::px(21);
    app.render_dirty().unwrap();
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.world
        .resource_mut::<LayoutSnapshot>()
        .unwrap()
        .invalidate();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );

    for change in 0..4 {
        app.world.get_mut::<Style>(leaf).unwrap().layout.left = Dimension::px(14 + change);
        match change {
            0 => {
                app.world
                    .get_mut::<Text>(label)
                    .unwrap()
                    .set_content("updated");
            }
            1 => {
                app.with_fonts(crate::render::font::default_font_manager());
            }
            2 => {
                app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                    .unwrap();
            }
            _ => {
                app.handle_memory_warning();
            }
        }
        PREPARATIONS.with(|count| count.set(0));
        app.render_dirty().unwrap();
        assert_eq!(PREPARATIONS.with(|count| count.get()), 0, "change {change}");
        let pixels = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.world
            .resource_mut::<LayoutSnapshot>()
            .unwrap()
            .invalidate();
        app.render().unwrap();
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            pixels
        );
    }
}

#[test]
fn parked_dirty_nodes_do_not_block_active_leaf_preparation() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            View (grow: 1.0) {
                View (id: "parked", width: 40, height: 30) [crate::ui::Hidden] {
                    View (id: "parked-leaf", width: 8, height: 8)
                }
                View (id: "active-leaf", width: 12, height: 10, position: Position::Absolute)
            }
        }
    });
    app.render().unwrap();
    let parked = app.world.find_by_id("parked-leaf").unwrap();
    let active = app.world.find_by_id("active-leaf").unwrap();
    app.world.insert(parked, crate::ui::dirty::Dirty);
    INPUT_VISITS.with(|visits| visits.set((0, 0)));
    PREPARATIONS.with(|count| count.set(0));
    app.world.get_mut::<Style>(active).unwrap().layout.left = Dimension::px(7);
    app.render_dirty().unwrap();
    assert_eq!(INPUT_VISITS.with(|visits| visits.get()), (1, 1));
    assert_eq!(PREPARATIONS.with(|count| count.get()), 1);
    assert!(app.world.has::<crate::ui::dirty::Dirty>(parked));
}
