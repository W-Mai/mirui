use super::chips::CHIPS;
#[cfg(feature = "std")]
use super::setup_app;
use super::{VIEWPORT, build_widgets};
use crate::prelude::*;
use crate::ui::widgets::Image;
use crate::ui::{Children, IdMap, Style, UiScope};

fn fixture() -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
    (world, parent)
}

#[test]
fn preserves_every_migrated_layout_capability() {
    let (world, _) = fixture();
    for id in [
        "layout_lab_shell",
        "layout_lab_header",
        "layout_lab_flex",
        "layout_lab_content",
        "layout_lab_grow",
        "layout_lab_surface_round",
        "layout_lab_overlay_stage",
        "layout_lab_absolute",
        "layout_lab_image_flow",
        "layout_lab_image_overlay",
        "layout_lab_walk",
        "layout_lab_conditional",
    ] {
        assert!(world.find_by_id(id).is_some(), "missing {id}");
    }

    let grow = world
        .get::<Style>(world.find_by_id("layout_lab_grow").unwrap())
        .unwrap();
    assert_eq!(grow.layout.grow, Fixed::ONE);
    assert_eq!(grow.layout.min_width, Dimension::px(88));
    assert_eq!(grow.layout.max_width, Dimension::px(220));

    let absolute = world
        .get::<Style>(world.find_by_id("layout_lab_absolute").unwrap())
        .unwrap();
    assert_eq!(absolute.layout.position, Position::Absolute);

    let walk = world
        .get::<Children>(world.find_by_id("layout_lab_walk").unwrap())
        .unwrap();
    assert_eq!(walk.0.len(), CHIPS.len());
    for id in ["layout_lab_image_flow", "layout_lab_image_overlay"] {
        assert!(world.get::<Image>(world.find_by_id(id).unwrap()).is_some());
    }
}

#[test]
fn default_viewport_keeps_the_two_by_two_grid_inside_the_shell() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let (mut world, parent) = fixture();
    update_layout(
        &mut world,
        parent,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );
    let rect = |world: &World, id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let shell = rect(&world, "layout_lab_shell");
    let flex = rect(&world, "layout_lab_flex");
    let surfaces = rect(&world, "layout_lab_surfaces");
    let overlay = rect(&world, "layout_lab_overlay_card");
    let collection = rect(&world, "layout_lab_collection");

    assert_eq!(flex.y, surfaces.y);
    assert_eq!(overlay.y, collection.y);
    assert!(overlay.y > flex.y);
    for card in [flex, surfaces, overlay, collection] {
        assert!(card.x >= shell.x && card.x + card.w <= shell.x + shell.w);
        assert!(card.y >= shell.y && card.y + card.h <= shell.y + shell.h);
    }

    let note = rect(&world, "layout_lab_surface_note");
    assert!(note.x + note.w <= surfaces.x + surfaces.w);
    assert!(note.y + note.h <= surfaces.y + surfaces.h);
}

#[test]
fn phone_viewport_stacks_cards_without_horizontal_overflow() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let (mut world, parent) = fixture();
    update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
    let rect = |world: &World, id| {
        world
            .get::<ComputedRect>(world.find_by_id(id).unwrap())
            .unwrap()
            .0
    };
    let shell = rect(&world, "layout_lab_shell");
    let cards = [
        rect(&world, "layout_lab_flex"),
        rect(&world, "layout_lab_surfaces"),
        rect(&world, "layout_lab_overlay_card"),
        rect(&world, "layout_lab_collection"),
    ];
    for card in cards {
        assert!(card.x >= shell.x);
        assert!(card.x + card.w <= shell.x + shell.w);
    }

    let overlay = rect(&world, "layout_lab_overlay_stage");
    let absolute = rect(&world, "layout_lab_absolute");
    assert!(absolute.x + absolute.w <= overlay.x + overlay.w);
}

#[test]
fn medium_portrait_cards_fill_the_single_column() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(539, 734), (320, 568)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let parent = app.spawn_root().id();
        setup_app(&mut app, parent);
        app.set_root(parent);
        update_layout(
            &mut app.world,
            parent,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let grid = rect(&app.world, "layout_lab_grid");
        for id in [
            "layout_lab_flex",
            "layout_lab_surfaces",
            "layout_lab_overlay_card",
            "layout_lab_collection",
        ] {
            let card = rect(&app.world, id);
            assert_eq!(card.x, grid.x, "{width}x{height}: {id}");
            assert_eq!(card.w, grid.w, "{width}x{height}: {id}");
        }
        super::super::assert_text_layouts_fit(&app.world);
    }
}

#[test]
fn phone_scroll_extent_ends_at_the_last_visible_descendant() {
    use crate::input::event::scroll::scroll_bounds;
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    for (width, height) in [(320, 568), (422, 600), (480, 320)] {
        let (mut world, parent) = fixture();
        update_layout(
            &mut world,
            parent,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let shell = world.find_by_id("layout_lab_shell").unwrap();
        let content = world.find_by_id("layout_lab_document").unwrap();
        let last = world.find_by_id("layout_lab_collection").unwrap();
        let shell_rect = world.get::<ComputedRect>(shell).unwrap().0;
        let content_rect = world.get::<ComputedRect>(content).unwrap().0;
        let last_rect = world.get::<ComputedRect>(last).unwrap().0;
        let bounds = scroll_bounds(&world, shell).unwrap();
        let extent = bounds.content_height;
        let max_offset = bounds.max_y;

        assert!(max_offset > Fixed::ZERO, "{width}x{height}");
        assert!(
            last_rect.y + last_rect.h - max_offset <= shell_rect.y + shell_rect.h,
            "{width}x{height}: {last_rect:?} {shell_rect:?} {extent:?}",
        );
        assert!(
            last_rect.y + last_rect.h - max_offset > shell_rect.y,
            "{width}x{height}: {last_rect:?} {shell_rect:?} {extent:?}",
        );
        assert!(extent >= last_rect.y + last_rect.h - content_rect.y);
    }
}
