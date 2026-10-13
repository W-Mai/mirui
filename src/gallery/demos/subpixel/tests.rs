use super::composition::build_widgets;
use super::motion::bar_move_system;
use super::runtime::setup_app;
use super::state::{BAR_MARGIN, BAR_W, BarState, START_Y};
use crate::ecs::{SystemScheduler, World};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::dirty::{Dirty, VisualDirty};
use crate::ui::render_system::try_update_layout;
use crate::ui::{Children, ComputedRect, Style};

fn run_bar_motion(world: &mut World) {
    let mut scheduler = SystemScheduler::new();
    scheduler.add(bar_move_system::system());
    scheduler.run_all(world).unwrap();
}

#[test]
fn bars_use_their_own_parent_bounds() {
    let mut app = App::headless(320, 240);
    let root = app.spawn_root().id();
    let arenas = app.compose(root, |tree| {
        [140, 240].map(|width| {
            let (arena, bar) = tree.children(Style::default(), |children| {
                children.spawn(BarState {
                    y: Fixed::from_int(40),
                    speed_per_second: Fixed::ZERO,
                    snap: false,
                    x: Fixed::ZERO,
                    right_anchored: true,
                })
            });
            (arena, bar, width)
        })
    });
    for (arena, _, width) in arenas {
        app.world.insert(
            arena,
            ComputedRect(crate::types::Rect::new(0, 0, width, width - 100)),
        );
    }
    app.add_resource(crate::ecs::DeltaTimeMs(1));
    run_bar_motion(&mut app.world);

    for (_, entity, width) in arenas {
        let bar = app.world.get::<BarState>(entity).unwrap();
        assert_eq!(bar.x, Fixed::from_int(width - BAR_W - BAR_MARGIN));
        let expected_y = if width == 140 { START_Y } else { 40 };
        assert_eq!(bar.y, Fixed::from_int(expected_y));
        let style = app.world.get::<Style>(entity).unwrap();
        assert_eq!(style.layout.left, Dimension::Px(bar.x));
        assert_eq!(style.layout.top, Dimension::Px(bar.y));
    }

    app.world.remove::<ComputedRect>(arenas[0].0);
    app.world.insert(
        arenas[1].0,
        ComputedRect(crate::types::Rect::new(0, 0, 260, 160)),
    );
    run_bar_motion(&mut app.world);
    assert_eq!(
        app.world.get::<BarState>(arenas[0].1).unwrap().x,
        Fixed::from_int(80)
    );
    assert_eq!(
        app.world.get::<BarState>(arenas[1].1).unwrap().x,
        Fixed::from_int(200)
    );
}

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(320, 240);
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn stage_and_arena_fit_phone_and_desktop_viewports() {
    for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        try_update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        )
        .unwrap();

        for id in ["subpixel_stage", "subpixel_arena"] {
            let entity = app.world.find_by_id(id).unwrap();
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.x >= Fixed::ZERO);
            assert!(rect.y >= Fixed::ZERO);
            assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
            assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
        }
    }
}

#[test]
fn bars_follow_theme_and_live_arena_bounds() {
    let mut app = App::headless(320, 568);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    try_update_layout(&mut app.world, root, &Viewport::new(320, 568, Fixed::ONE)).unwrap();

    let snapped = app.world.find_by_id("subpixel_snapped_bar").unwrap();
    let smooth = app.world.find_by_id("subpixel_smooth_bar").unwrap();
    assert_eq!(
        app.world.get::<Style>(snapped).unwrap().bg_color,
        Some(ColorToken::Error.into()),
    );
    assert_eq!(
        app.world.get::<Style>(smooth).unwrap().bg_color,
        Some(ColorToken::Secondary.into()),
    );

    app.world.insert_resource(crate::ecs::DeltaTimeMs(250));
    run_bar_motion(&mut app.world);
    let arena_width = app
        .world
        .get::<ComputedRect>(app.world.find_by_id("subpixel_arena").unwrap())
        .unwrap()
        .0
        .w
        .to_int();
    let smooth_state = app.world.get::<BarState>(smooth).unwrap();
    assert_eq!(
        smooth_state.x,
        Fixed::from_int(arena_width - BAR_W - BAR_MARGIN),
    );
    assert!(smooth_state.y > Fixed::from_int(START_Y));
}

#[test]
fn snapped_bar_stays_clean_until_its_pixel_position_changes() {
    let mut app = App::headless(320, 568);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    try_update_layout(&mut app.world, root, &Viewport::new(320, 568, Fixed::ONE)).unwrap();

    let snapped = app.world.find_by_id("subpixel_snapped_bar").unwrap();
    app.world.remove::<Dirty>(snapped);
    app.world.remove::<VisualDirty>(snapped);
    app.world.insert_resource(crate::ecs::DeltaTimeMs(1));

    run_bar_motion(&mut app.world);

    assert!(!app.world.has::<Dirty>(snapped));
    assert!(!app.world.has::<VisualDirty>(snapped));
}

#[test]
#[should_panic(expected = "requires resource")]
fn bar_motion_requires_frame_delta() {
    run_bar_motion(&mut World::new());
}
