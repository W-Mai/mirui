use super::*;
use crate::types::Dimension;
use crate::ui::dirty::Dirty;
use crate::ui::layout::LayoutStyle;
use crate::ui::property::{apply_to_world, prop};
use crate::ui::widgets::Text;
use crate::ui::{Hidden, branch::CachedBranchVisibility};

fn widget(world: &mut World, width: i32) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, Widget);
    world.insert(
        entity,
        Style {
            layout: LayoutStyle {
                width: Dimension::px(width),
                height: Dimension::px(10),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    entity
}

fn child(world: &mut World, parent: Entity, text: Option<&'static str>) -> Entity {
    let entity = widget(world, 20);
    world.insert(entity, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(entity);
    } else {
        world.insert(parent, Children(vec![entity]));
    }
    if let Some(text) = text {
        world.insert(entity, Text::from(text));
    }
    entity
}

fn text_handle(world: &World, entity: Entity) -> crate::text::TextLayoutHandle {
    *world.get::<crate::text::TextLayoutHandle>(entity).unwrap()
}

#[test]
fn stable_value_refresh_matches_structural_refresh_without_allocating() {
    fn same_tree(left: &LayoutNode, right: &LayoutNode) {
        assert_eq!(left.entity, right.entity);
        assert_eq!(left.style, right.style);
        assert_eq!(left.rect, right.rect);
        assert_eq!(left.intrinsic_width, right.intrinsic_width);
        assert_eq!(left.intrinsic_height, right.intrinsic_height);
        assert_eq!(left.children.len(), right.children.len());
        for (left, right) in left.children.iter().zip(&right.children) {
            same_tree(left, right);
        }
    }

    let mut world = crate::app::App::headless(128, 128).world;
    let root = widget(&mut world, 128);
    let branch = child(&mut world, root, None);
    let label = child(&mut world, branch, Some("old"));
    let leaf = child(&mut world, root, None);
    let mut stable = compute_layout_snapshot(&mut world, root, 128, 128, false)
        .unwrap()
        .unwrap();
    let mut general = compute_layout_snapshot(&mut world, root, 128, 128, false)
        .unwrap()
        .unwrap();

    for width in [24, 28, 40, 16] {
        world.get_mut::<Style>(leaf).unwrap().layout.width = Dimension::px(width);
        world
            .get_mut::<Style>(label)
            .unwrap()
            .set_font_size(width as u16);
        world
            .get_mut::<Text>(label)
            .unwrap()
            .set_content("new text");
        assert!(stable.maintenance_current(&world));
        world.put_resource_box(stable);
        stable = compute_layout_snapshot(&mut world, root, 128, 128, false)
            .unwrap()
            .unwrap();
        general.valid = false;
        world.put_resource_box(general);
        general = compute_layout_snapshot(&mut world, root, 128, 128, false)
            .unwrap()
            .unwrap();
        same_tree(&stable.layout_tree, &general.layout_tree);
    }

    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                refresh_stable_layout_tree(
                    &world,
                    &mut stable.layout_tree,
                    &mut LayoutTreeChanges::default(),
                )
                .unwrap();
            }
        }),
        0
    );
}

#[test]
fn hidden_branches_keep_nested_storage_and_children_only_order() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let deep = widget(&mut world, 20);
    let middle = widget(&mut world, 20);
    let leaf = widget(&mut world, 20);
    let shallow = widget(&mut world, 24);
    let shallow_leaf = widget(&mut world, 12);
    let tail = widget(&mut world, 8);
    world.insert(root, Children(vec![deep, shallow, tail]));
    world.insert(deep, Children(vec![middle]));
    world.insert(middle, Children(vec![leaf]));
    world.insert(shallow, Children(vec![shallow_leaf]));
    world.insert(shallow, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert!(snapshot.parked.capacity() >= 1);
    assert!(snapshot.entities.capacity() >= 7);
    assert!(snapshot.painted_damage.capacity() >= 7);
    let root_children_ptr = snapshot.layout_tree.children.as_ptr();
    let nested_ptr = snapshot.layout_tree.children[0].children.as_ptr();
    let nested_leaf_ptr = snapshot.layout_tree.children[0].children[0]
        .children
        .as_ptr();
    assert_eq!(snapshot.entities, [root, deep, middle, leaf, tail]);
    assert_eq!(snapshot.layout_tree.children.len(), 2);
    assert_eq!(snapshot.layout_tree.children[0].entity, Some(deep));
    assert_eq!(snapshot.layout_tree.children[1].entity, Some(tail));
    assert!(
        snapshot
            .parked
            .iter()
            .any(|node| node.entity == Some(shallow))
    );

    world.insert(deep, Hidden);
    world.remove::<Hidden>(shallow);
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.entities, [root, shallow, shallow_leaf, tail]);
    assert_eq!(snapshot.layout_tree.children.as_ptr(), root_children_ptr);
    assert_eq!(snapshot.layout_tree.children[0].entity, Some(shallow));
    assert_eq!(snapshot.layout_tree.children[1].entity, Some(tail));

    world.remove::<Hidden>(deep);
    world.insert(shallow, Hidden);
    world.get_mut::<Children>(root).unwrap().0 = vec![tail, deep, shallow];
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.entities, [root, tail, deep, middle, leaf]);
    assert_eq!(snapshot.layout_tree.children.as_ptr(), root_children_ptr);
    assert_eq!(snapshot.layout_tree.children[0].entity, Some(tail));
    assert_eq!(snapshot.layout_tree.children[1].entity, Some(deep));
    assert_eq!(
        snapshot.layout_tree.children[1].children.as_ptr(),
        nested_ptr
    );
    assert_eq!(
        snapshot.layout_tree.children[1].children[0]
            .children
            .as_ptr(),
        nested_leaf_ptr
    );
}

#[test]
fn parked_layout_nodes_do_not_alias_recycled_entity_generations() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let old = widget(&mut world, 16);
    world.insert(root, Children(vec![old]));
    world.insert(old, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.despawn(old));

    let replacement = widget(&mut world, 40);
    assert_eq!(replacement.id, old.id);
    assert_ne!(replacement.generation, old.generation);
    world.get_mut::<Children>(root).unwrap().0[0] = replacement;
    try_update_layout(&mut world, root, &viewport).unwrap();

    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.entities, [root, replacement]);
    assert_eq!(snapshot.layout_tree.children[0].entity, Some(replacement));
    assert_eq!(
        snapshot.layout_tree.children[0].style.width,
        Dimension::px(40)
    );
    assert!(!snapshot.parked.iter().any(|node| node.entity == Some(old)));
}

#[test]
fn static_layout_does_not_reserve_parked_nodes() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let first = widget(&mut world, 16);
    let second = widget(&mut world, 16);
    world.insert(root, Children(vec![first, second]));
    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.parked.capacity(), 0);
    assert_eq!(snapshot.layout_tree.children.len(), 2);
}

#[test]
fn hidden_root_discards_its_previous_layout_snapshot() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.resource::<LayoutSnapshot>().is_some());

    world.insert(root, Hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.resource::<LayoutSnapshot>().is_none());

    world.remove::<Hidden>(root);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.resource::<LayoutSnapshot>().is_some());
}

#[test]
fn detached_alive_branch_does_not_remain_parked() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let hidden = widget(&mut world, 16);
    let child = widget(&mut world, 8);
    world.insert(root, Children(vec![hidden]));
    world.insert(hidden, Children(vec![child]));
    world.insert(hidden, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.resource::<LayoutSnapshot>().unwrap().parked.len() == 1);

    world.get_mut::<Children>(root).unwrap().0.clear();
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert!(world.is_alive(hidden));
    assert!(snapshot.parked.is_empty());
    assert!(snapshot.layout_tree.children.is_empty());
    assert_eq!(snapshot.entities, [root]);
}

#[test]
fn parked_maintenance_tracks_structure_without_warm_allocations() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let hidden = child(&mut world, root, None);
    child(&mut world, hidden, None);
    world.insert(hidden, Hidden);
    world.insert(hidden, CachedBranchVisibility { selected: false });
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..4 {
                assert!(
                    world
                        .resource::<LayoutSnapshot>()
                        .unwrap()
                        .maintenance_current(&world)
                );
                try_update_layout(&mut world, root, &viewport).unwrap();
            }
        }),
        0
    );
    for kind in 0..6 {
        match kind {
            0 => {
                world.get_mut::<Children>(root).unwrap();
            }
            1 => {
                world.get_mut::<Parent>(hidden).unwrap();
            }
            2 => {
                world.get_mut::<Hidden>(hidden).unwrap();
            }
            3 => {
                world.get_mut::<CachedBranchVisibility>(hidden).unwrap();
            }
            4 => {
                world.get_mut::<Widget>(hidden).unwrap();
            }
            _ => {
                world.get_mut::<Style>(hidden).unwrap();
            }
        }
        assert_eq!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .maintenance_current(&world),
            kind >= 4
        );
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert!(snapshot.maintenance_current(&world));
        assert_eq!(snapshot.parked.len(), 1);
    }
    world.remove::<Style>(hidden);
    assert!(
        !world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .maintenance_current(&world)
    );
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(
        world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .parked
            .is_empty()
    );
}

#[test]
fn parked_refresh_reports_swaps_without_a_pool_length_change() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let first = child(&mut world, root, None);
    let second = child(&mut world, root, None);
    world.insert(second, Hidden);
    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();
    world.insert(first, Hidden);
    world.remove::<Hidden>(second);
    let mut snapshot = world.take_resource_box::<LayoutSnapshot>().unwrap();
    let mut changes = LayoutTreeChanges::default();
    let count = snapshot.parked.len();
    assert!(
        refresh_layout_tree(
            &mut world,
            root,
            &mut snapshot.layout_tree,
            &mut snapshot.parked,
            &mut changes,
        )
        .unwrap()
    );
    assert!(changes.parked);
    assert_eq!(snapshot.parked.len(), count);
    assert_eq!(snapshot.parked[0].entity, Some(first));
    assert_eq!(snapshot.layout_tree.children[0].entity, Some(second));
}

#[test]
fn parked_reachability_reuses_storage_and_releases_under_memory_pressure() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let hidden = child(&mut world, root, None);
    world.insert(hidden, Hidden);
    for _ in 0..16 {
        child(&mut world, hidden, None);
    }
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap().reachable.len(),
        18
    );
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                world.get_mut::<Children>(root).unwrap();
                try_update_layout(&mut world, root, &viewport).unwrap();
            }
        }),
        0
    );
    world
        .resource_mut::<LayoutSnapshot>()
        .unwrap()
        .trim_memory();
    assert_eq!(
        world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .reachable
            .capacity(),
        0
    );
    world.get_mut::<Children>(root).unwrap().0.clear();
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert!(snapshot.parked.is_empty());
    assert_eq!(snapshot.reachable.capacity(), 0);
}

#[test]
fn parked_nodes_release_removed_layout_components_and_restore_current_structure() {
    for remove_widget in [false, true] {
        for remove_root in [false, true] {
            let mut world = World::new();
            let root = widget(&mut world, 64);
            let hidden = child(&mut world, root, None);
            let removed = child(&mut world, hidden, None);
            let retained = child(&mut world, hidden, None);
            world.insert(hidden, Hidden);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            try_update_layout(&mut world, root, &viewport).unwrap();
            let target = if remove_root { hidden } else { removed };
            let style = if remove_widget {
                world.remove::<Widget>(target);
                None
            } else {
                world.remove::<Style>(target)
            };
            try_update_layout(&mut world, root, &viewport).unwrap();
            let snapshot = world.resource::<LayoutSnapshot>().unwrap();
            assert_eq!(snapshot.entities, [root]);
            if remove_root {
                assert!(snapshot.parked.is_empty());
            } else {
                assert_eq!(snapshot.parked.len(), 1);
                assert_eq!(snapshot.parked[0].children.len(), 1);
                assert_eq!(snapshot.parked[0].children[0].entity, Some(retained));
            }

            if let Some(style) = style {
                world.insert(target, style);
            } else {
                world.insert(target, Widget);
            }
            world.get_mut::<Children>(hidden).unwrap().0.swap(0, 1);
            world.remove::<Hidden>(hidden);
            try_update_layout(&mut world, root, &viewport).unwrap();
            let snapshot = world.resource::<LayoutSnapshot>().unwrap();
            assert!(snapshot.maintenance_current(&world));
            assert!(snapshot.parked.is_empty());
            assert_eq!(snapshot.entities, [root, hidden, retained, removed]);
        }
    }
}

#[test]
fn parked_children_follow_reparenting_without_duplicate_storage() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let hidden = child(&mut world, root, None);
    let visible = child(&mut world, root, None);
    let moved = child(&mut world, hidden, None);
    let leaf = child(&mut world, moved, None);
    world.insert(hidden, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    world.get_mut::<Children>(hidden).unwrap().0.clear();
    world.insert(visible, Children(vec![moved]));
    world.insert(moved, Parent(visible));
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert!(snapshot.maintenance_current(&world));
    assert_eq!(snapshot.entities, [root, visible, moved, leaf]);
    assert_eq!(snapshot.parked.len(), 1);
    assert_eq!(snapshot.parked[0].entity, Some(hidden));
    assert!(snapshot.parked[0].children.is_empty());

    world.remove::<Hidden>(hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.entities, [root, hidden, visible, moved, leaf]);
    assert!(snapshot.parked.is_empty());
}

#[test]
fn failed_text_preparation_does_not_confirm_parked_maintenance() {
    use crate::ui::widgets::{Placeholder, TextInput};

    let mut app = crate::app::App::headless(96, 48);
    app.with_default_widgets();
    app.with_text_layout_limits(crate::text::TextLayoutLimits {
        text_bytes: 2,
        ..crate::text::TextLayoutLimits::default()
    });
    let root = app.spawn_root().id();
    let input = app.compose(root, |cx| cx.spawn(TextInput::build()));
    app.world.insert(input, Placeholder("A"));
    let hidden = child(&mut app.world, root, None);
    app.world.insert(hidden, Hidden);
    app.render().unwrap();
    let revisions = app
        .world
        .resource::<LayoutSnapshot>()
        .unwrap()
        .maintenance_revisions;

    app.world.get_mut::<Style>(input).unwrap();
    app.world.get_mut::<Children>(root).unwrap();
    app.world.insert(input, Placeholder("Too long"));
    app.world.invalidate(input);
    for _ in 0..2 {
        assert!(matches!(app.render_dirty(), Err(RenderError::TextLayout)));
        let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
        assert!(!snapshot.maintenance_current(&app.world));
        assert_eq!(snapshot.maintenance_revisions, revisions);
    }
    app.world.insert(input, Placeholder("A"));
    app.render_dirty().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    assert!(snapshot.maintenance_current(&app.world));
    assert_ne!(snapshot.maintenance_revisions, revisions);
    assert_eq!(snapshot.parked.len(), 1);
}

#[test]
fn initially_hidden_text_keeps_only_layout_node_storage() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let hidden = child(&mut world, root, Some("deferred"));
    world.insert(hidden, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.get::<crate::text::TextLayoutHandle>(hidden).is_none());
    assert!(
        world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .parked
            .iter()
            .any(|node| node.entity == Some(hidden))
    );

    world.remove::<Hidden>(hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.get::<crate::text::TextLayoutHandle>(hidden).is_some());
}

#[test]
fn children_only_hidden_branches_restore_painted_pixels() {
    use crate::surface::FramebufferAccess;
    use crate::types::Color;

    fn pixel<F: FnMut(&[u8], crate::types::PhysicalRect)>(
        app: &mut crate::app::App<crate::surface::framebuf::FramebufSurface<F>>,
    ) -> [u8; 3] {
        let framebuffer = app.backend.framebuffer();
        let offset = (2 * 64 + 2) * 4;
        let pixels = framebuffer.read_pixels().unwrap();
        let bytes = pixels.bytes();
        [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
    }

    let mut app = crate::app::App::headless(64, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let red = widget(&mut app.world, 64);
    let red_child = widget(&mut app.world, 16);
    let blue = widget(&mut app.world, 64);
    app.world
        .get_mut::<Style>(red)
        .unwrap()
        .set_bg_color(Color::rgb(240, 20, 30));
    app.world
        .get_mut::<Style>(blue)
        .unwrap()
        .set_bg_color(Color::rgb(20, 70, 240));
    app.world.insert(root, Children(vec![red, blue]));
    app.world.insert(red, Children(vec![red_child]));
    app.world.insert(blue, Hidden);

    app.render().unwrap();
    assert_eq!(pixel(&mut app), [240, 20, 30]);
    app.world.insert(red, Hidden);
    app.world.remove::<Hidden>(blue);
    app.world.mark_subtree_dirty(root);
    app.render_dirty().unwrap();
    assert_eq!(pixel(&mut app), [20, 70, 240]);

    app.world.remove::<Hidden>(red);
    app.world.insert(blue, Hidden);
    app.world.mark_subtree_dirty(root);
    app.render_dirty().unwrap();
    assert_eq!(pixel(&mut app), [240, 20, 30]);
}

#[test]
fn visible_property_keeps_snapshot_storage_and_never_renders_stale_text() {
    #[derive(Default)]
    struct GlyphRecorder(usize);

    impl Renderer for GlyphRecorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            if matches!(request.command, DrawCommand::GlyphRun { .. }) {
                self.0 += 1;
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let root = widget(&mut world, 64);
    let label = child(&mut world, root, Some("VISIBLE"));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    let snapshot_ptr = snapshot as *const LayoutSnapshot;
    let entities_ptr = snapshot.entities.as_ptr();
    let entities_capacity = snapshot.entities.capacity();
    let mut drawn = GlyphRecorder::default();
    render(&world, root, &viewport, &mut drawn).unwrap();
    assert!(drawn.0 > 0);

    apply_to_world::<prop::Visible>(&mut world, label, false);
    assert!(world.resource::<LayoutSnapshot>().is_some());
    let mut drawn = GlyphRecorder::default();
    render(&world, root, &viewport, &mut drawn).unwrap();
    assert_eq!(drawn.0, 0);
    try_update_layout(&mut world, root, &viewport).unwrap();
    let hidden = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(hidden as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(hidden.entities.as_ptr(), entities_ptr);
    assert_eq!(hidden.entities.capacity(), entities_capacity);
    assert_eq!(hidden.entities, [root]);

    apply_to_world::<prop::Visible>(&mut world, label, true);
    assert!(
        !world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .matches(root, 64, 64)
    );
    try_update_layout(&mut world, root, &viewport).unwrap();
    let mut drawn = GlyphRecorder::default();
    render(&world, root, &viewport, &mut drawn).unwrap();
    assert!(drawn.0 > 0);
    let visible = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(visible as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(visible.entities.as_ptr(), entities_ptr);
    assert_eq!(visible.entities.capacity(), entities_capacity);
    assert_eq!(visible.entities, [root, label]);
}

#[test]
fn dirty_layout_reuses_snapshot_storage_when_viewport_changes() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    world.get_mut::<Style>(root).unwrap().layout.width = Dimension::Auto;
    let label = child(&mut world, root, None);
    let wide = Viewport::new(64, 64, Fixed::ONE);
    let narrow = Viewport::new(32, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &wide).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    let snapshot_ptr = snapshot as *const LayoutSnapshot;
    let entities_ptr = snapshot.entities.as_ptr();
    assert_eq!(snapshot.entities, [root, label]);

    let mut plan = DirtyRegions::default();
    try_collect_dirty_regions_into(&mut world, root, &narrow, &mut plan).unwrap();

    let resized = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(resized as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(resized.entities.as_ptr(), entities_ptr);
    assert!(resized.matches(root, 32, 64));
    assert_eq!(
        world.get::<super::super::ComputedRect>(root).unwrap().0.w,
        Fixed::from_int(32)
    );
    assert_eq!(resized.entities, [root, label]);
}

#[test]
fn dirty_layout_reuses_invalidated_snapshot_after_visible_changes() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let label = child(&mut world, root, None);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    let snapshot_ptr = snapshot as *const LayoutSnapshot;
    let entities_ptr = snapshot.entities.as_ptr();
    let mut plan = DirtyRegions::default();

    apply_to_world::<prop::Visible>(&mut world, label, false);
    assert!(
        !world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .matches(root, 64, 64)
    );
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    let hidden = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(hidden as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(hidden.entities.as_ptr(), entities_ptr);
    assert_eq!(hidden.entities, [root]);

    apply_to_world::<prop::Visible>(&mut world, label, true);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    let visible = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(visible as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(visible.entities.as_ptr(), entities_ptr);
    assert_eq!(visible.entities, [root, label]);
}

fn layout_is_live(world: &World, handle: crate::text::TextLayoutHandle) -> bool {
    world
        .resource::<Outputs<TextGeometry>>()
        .unwrap()
        .storage()
        .layouts
        .get(handle)
        .is_some()
}

#[test]
fn warm_layout_reuses_tree_and_entity_storage() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let first = widget(&mut world, 20);
    let second = widget(&mut world, 20);
    world.insert(root, Children(vec![first, second]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    let snapshot_ptr = snapshot as *const LayoutSnapshot;
    let tree_ptr = snapshot.layout_tree.children.as_ptr();
    let entities_ptr = snapshot.entities.as_ptr();

    try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(snapshot.layout_tree.children.as_ptr(), tree_ptr);
    assert_eq!(snapshot.entities.as_ptr(), entities_ptr);
    assert_eq!(snapshot.entities, [root, first, second]);
}

#[test]
fn hidden_ancestor_retires_only_its_text_layouts_before_tree_build() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let visible = child(&mut world, root, Some("visible"));
    let panel = child(&mut world, root, None);
    let hidden = child(&mut world, panel, Some("hidden"));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let visible_handle = text_handle(&world, visible);
    let hidden_handle = text_handle(&world, hidden);
    world.insert(panel, Hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();

    assert_eq!(text_handle(&world, visible), visible_handle);
    assert!(layout_is_live(&world, visible_handle));
    assert!(!layout_is_live(&world, hidden_handle));

    world.remove::<Hidden>(panel);
    try_update_layout(&mut world, root, &viewport).unwrap();
    let returned = text_handle(&world, hidden);
    assert_ne!(returned, hidden_handle);
    assert!(layout_is_live(&world, returned));
}

#[test]
fn hidden_root_retires_children_only_text_layouts() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let label = widget(&mut world, 20);
    world.insert(label, Text::from("visible"));
    world.insert(root, Children(vec![label]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let handle = text_handle(&world, label);
    assert!(layout_is_live(&world, handle));

    world.insert(root, Hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(world.resource::<LayoutSnapshot>().is_none());
    assert!(!layout_is_live(&world, handle));
}

#[test]
fn children_only_tree_preserves_visible_text_layout() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let label = widget(&mut world, 20);
    world.insert(label, Text::from("visible"));
    world.insert(root, Children(vec![label]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let first = text_handle(&world, label);
    try_update_layout(&mut world, root, &viewport).unwrap();

    assert_eq!(text_handle(&world, label), first);
    assert!(layout_is_live(&world, first));
}

#[test]
fn visible_text_input_owner_is_retained() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let input = child(&mut world, root, None);
    world.insert(input, crate::ui::widgets::TextInput::new());
    let mut visible = Vec::new();
    collect_entities_preorder(&world, root, &mut visible);
    visible.sort_unstable_by_key(|entity| (entity.id, entity.generation));

    assert!(visible_text_owner(
        &world,
        &visible,
        text_layout_owner(input)
    ));
}

#[test]
fn cached_branch_text_layout_rebuilds_when_selected_again() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let branch = child(&mut world, root, None);
    let label = child(&mut world, branch, Some("branch"));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let first = text_handle(&world, label);
    world.insert(branch, CachedBranchVisibility { selected: false });
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert!(!layout_is_live(&world, first));

    world.insert(branch, CachedBranchVisibility { selected: true });
    try_update_layout(&mut world, root, &viewport).unwrap();
    let rebuilt = text_handle(&world, label);
    assert_ne!(rebuilt, first);
    assert!(layout_is_live(&world, rebuilt));
}

#[test]
fn despawned_text_owner_cannot_reuse_a_layout_handle() {
    let mut world = crate::app::App::headless(64, 64).world;
    let root = widget(&mut world, 64);
    let old = child(&mut world, root, Some("before"));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let stale = text_handle(&world, old);
    assert!(world.despawn(old));
    world.get_mut::<Children>(root).unwrap().0.clear();
    let replacement = child(&mut world, root, Some("after"));
    assert_eq!(replacement.id, old.id);
    assert_ne!(replacement.generation, old.generation);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let current = text_handle(&world, replacement);
    assert!(!layout_is_live(&world, stale));
    assert!(layout_is_live(&world, current));
    assert_ne!(current, stale);
}

#[test]
fn switching_roots_retires_text_from_the_previous_tree() {
    let mut world = crate::app::App::headless(64, 64).world;
    let first_root = widget(&mut world, 64);
    let first_label = child(&mut world, first_root, Some("first"));
    let second_root = widget(&mut world, 64);
    let second_label = child(&mut world, second_root, Some("second"));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, first_root, &viewport).unwrap();
    let previous = text_handle(&world, first_label);
    try_update_layout(&mut world, second_root, &viewport).unwrap();
    assert!(!layout_is_live(&world, previous));
    assert!(layout_is_live(&world, text_handle(&world, second_label)));
}

#[test]
fn detached_text_owners_are_not_visible() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let first = child(&mut world, root, Some("detached"));
    world.get_mut::<Children>(root).unwrap().0.clear();
    let mut visible = Vec::new();
    collect_entities_preorder(&world, root, &mut visible);
    visible.sort_unstable_by_key(|entity| (entity.id, entity.generation));
    assert!(!visible_text_owner(
        &world,
        &visible,
        text_layout_owner(first)
    ));
}

#[test]
fn hidden_descendant_dirty_stays_idle_until_revealed() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let panel = widget(&mut world, 20);
    let leaf = widget(&mut world, 10);
    world.insert(root, Children(vec![panel]));
    world.insert(panel, Parent(root));
    world.insert(panel, Children(vec![leaf]));
    world.insert(leaf, Parent(panel));
    world.insert(panel, Hidden);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    seed_prev_rects(&mut world, root, &viewport);
    world
        .resource_mut::<LayoutSnapshot>()
        .unwrap()
        .layout_tree
        .rect
        .x = Fixed::from_int(123);
    world.insert(leaf, Dirty);
    world.insert(leaf, super::super::dirty::VisualDirty::New);

    for _ in 0..2 {
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert!(plan.is_empty());
        assert_eq!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .layout_tree
                .rect
                .x,
            Fixed::from_int(123),
        );
    }

    world.remove::<Hidden>(panel);
    world.mark_subtree_dirty(panel);
    assert!(world.has::<Dirty>(panel));
    assert!(world.has::<Dirty>(leaf));
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(
        world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .layout_tree
            .rect
            .x,
        Fixed::ZERO,
    );
    assert!(!world.has::<Dirty>(panel));
    assert!(!world.has::<Dirty>(leaf));
    assert!(world.has::<super::super::dirty::VisualDirty>(leaf));
    seed_prev_rects(&mut world, root, &viewport);
    assert!(!world.has::<super::super::dirty::VisualDirty>(leaf));
}

#[test]
fn safe_area_is_minimum_root_padding() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(root, Children(vec![child]));
    world.insert(root, super::super::RespectSafeArea);
    world.insert_resource(crate::surface::SafeAreaInsets {
        top: Fixed::from_int(7),
        right: Fixed::from_int(3),
        bottom: Fixed::from_int(5),
        left: Fixed::from_int(11),
    });

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    assert_eq!(
        world.get::<super::super::ComputedRect>(child).unwrap().0,
        Rect::new(11, 7, 10, 10)
    );
    assert_eq!(
        world.get::<Style>(root).unwrap().layout.padding,
        crate::ui::layout::Padding::default(),
        "safe-area layout must not mutate authored style",
    );
}

#[test]
fn authored_padding_larger_than_safe_area_is_preserved() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(root, Children(vec![child]));
    world.insert(root, super::super::RespectSafeArea);
    world.get_mut::<Style>(root).unwrap().layout.padding = crate::ui::layout::Padding::all(13);
    world.insert_resource(crate::surface::SafeAreaInsets {
        top: Fixed::from_int(7),
        right: Fixed::from_int(3),
        bottom: Fixed::from_int(5),
        left: Fixed::from_int(11),
    });

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    assert_eq!(
        world.get::<super::super::ComputedRect>(child).unwrap().0,
        Rect::new(13, 13, 10, 10)
    );
}

#[test]
fn refreshed_tree_tracks_reorder_visibility_and_style() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let first = widget(&mut world, 20);
    let second = widget(&mut world, 20);
    world.insert(root, Children(vec![first, second]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    world.get_mut::<Children>(root).unwrap().0.swap(0, 1);
    world.get_mut::<Style>(second).unwrap().layout.width = Dimension::px(30);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap().entities,
        [root, second, first]
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(second).unwrap().0.x,
        Fixed::ZERO
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(first).unwrap().0.x,
        Fixed::from_int(30)
    );

    world.insert(second, Hidden);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap().entities,
        [root, first]
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(first).unwrap().0.x,
        Fixed::ZERO
    );

    world.remove::<Hidden>(second);
    try_update_layout(&mut world, root, &viewport).unwrap();
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap().entities,
        [root, second, first]
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(first).unwrap().0.x,
        Fixed::from_int(30)
    );
}

#[test]
fn dirty_plan_reuses_capacity_across_active_and_idle_frames() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects.len(), 1);
    let rects_ptr = plan.rects.as_ptr();

    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects.len(), 1);
    assert_eq!(plan.rects.as_ptr(), rects_ptr);
    seed_prev_rects(&mut world, root, &viewport);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert!(plan.is_empty());
    assert_eq!(plan.rects.as_ptr(), rects_ptr);

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects.len(), 1);
    assert_eq!(plan.rects.as_ptr(), rects_ptr);
}

#[test]
fn paint_acknowledgement_updates_existing_coverage_without_replacing_its_identity() {
    use super::super::dirty::PaintedCoverage;
    use core::any::TypeId;

    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(root, Children(vec![child]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();
    let kind = TypeId::of::<PaintedCoverage>();
    world.watch_component_lifetime(kind);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    seed_prev_rects(&mut world, root, &viewport);
    let generation = world.component_generation(child, kind).unwrap();
    let previous = world.get::<PaintedCoverage>(child).unwrap().0;

    crate::ui::property::apply_to_world::<crate::ui::property::prop::Transform>(
        &mut world,
        child,
        Transform::translate(Fixed::from_int(20), Fixed::ZERO),
    );
    for _ in 0..2 {
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(world.get::<PaintedCoverage>(child).unwrap().0, previous);
    }
    seed_prev_rects(&mut world, root, &viewport);
    assert_eq!(
        world.get::<PaintedCoverage>(child).unwrap().0.outer,
        Rect::new(20, 0, 10, 10)
    );
    assert_eq!(world.component_generation(child, kind), Some(generation));

    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..4 {
            seed_prev_rects(&mut world, root, &viewport);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(world.component_generation(child, kind), Some(generation));
}

#[test]
fn unshifted_damage_preserves_coverage_until_acknowledged() {
    for count in [0, 1, 8, 17] {
        let mut world = World::new();
        for index in 0..count {
            world.invalidate_rect(Rect::new(
                Fixed::from_ratio(index * 27 - 9, 2),
                Fixed::from_ratio(index * 13 - 7, 2),
                Fixed::from_ratio(11, 2),
                Fixed::from_ratio(9, 2),
            ));
        }
        let mut expected = DirtyRegions::default();
        if let Some(regions) = world.resource_mut::<super::super::dirty::ExactDirtyRegions>() {
            regions.prepare_into(&mut expected.rects);
        }
        expected.coalesce_overlaps();
        let mut plan = DirtyRegions::default();
        prepare_dirty_rects(&mut world, &mut plan);
        assert_eq!(plan.rects, expected.rects);
        let allocations = crate::test_support::tracked_allocations(|| {
            plan.clear();
            prepare_dirty_rects(&mut world, &mut plan);
        });
        assert_eq!(allocations, 0);
        assert_eq!(plan.rects, expected.rects);
        if let Some(regions) = world.resource_mut::<super::super::dirty::ExactDirtyRegions>() {
            regions.acknowledge();
        }
        plan.clear();
        prepare_dirty_rects(&mut world, &mut plan);
        assert!(plan.is_empty());
    }
}

#[test]
fn exact_dirty_rect_bypasses_layout_walk_with_a_valid_snapshot() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(root, Children(vec![child]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;
    let exact = Rect::new(7, 9, 11, 13);
    seed_prev_rects(&mut world, root, &viewport);
    world.invalidate_rect(exact);

    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

    assert_eq!(plan.rects, [exact]);
    assert!(
        world
            .resource::<super::super::dirty::ExactDirtyRegions>()
            .is_some_and(|regions| !regions.is_empty())
    );
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
        snapshot
    );
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects, [exact]);
    seed_prev_rects(&mut world, root, &viewport);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert!(plan.is_empty());
}

#[test]
fn input_layout_tracks_repeated_invalidation_without_consuming_paint_damage() {
    use crate::input::event::hit_test::{geometry_changes, hit_test};
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(child, crate::ui::HitTarget);
    world.insert(root, Children(vec![child]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();
    world.invalidate(child);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    seed_prev_rects(&mut world, root, &viewport);
    let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;
    let allocations = crate::test_support::tracked_allocations(|| {
        for width in [20, 5, 20, 5] {
            world.get_mut::<Style>(child).unwrap().layout.width = Dimension::px(width);
            world.invalidate(child);
            prepare_input_geometry(&mut world, root, &viewport).unwrap();
            assert!(world.has::<Dirty>(child));
            assert_eq!(
                world
                    .get::<super::super::dirty::PaintedCoverage>(child)
                    .unwrap()
                    .0
                    .outer,
                Rect::new(0, 0, 10, 10)
            );
            assert_eq!(
                hit_test(&world, root, 15.into(), 5.into(), 64, 64),
                (width == 20).then_some(child)
            );
            assert_eq!(geometry_changes(&world), (false, false));
            prepare_input_geometry(&mut world, root, &viewport).unwrap();
            assert_eq!(
                world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
                snapshot
            );
        }
        for width in [20, 5] {
            try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
            let revision = world.component_revision::<Dirty>();
            world.get_mut::<Style>(child).unwrap().layout.width = Dimension::px(width);
            world.invalidate(child);
            assert_ne!(world.component_revision::<Dirty>(), revision);
            assert!(geometry_changes(&world).0);
            prepare_input_geometry(&mut world, root, &viewport).unwrap();
            assert!(world.has::<Dirty>(child));
            assert_eq!(
                world.get::<super::super::ComputedRect>(child).unwrap().0.w,
                Fixed::from_int(width)
            );
            try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
            assert_eq!(geometry_changes(&world), (false, false));
            assert_eq!(
                hit_test(&world, root, 15.into(), 5.into(), 64, 64),
                (width == 20).then_some(child)
            );
            assert_eq!(
                world
                    .get::<super::super::dirty::PaintedCoverage>(child)
                    .unwrap()
                    .0
                    .outer,
                Rect::new(0, 0, 10, 10)
            );
        }
    });
    assert_eq!(allocations, 0);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects, [Rect::new(0, 0, 10, 10)]);
}

#[test]
fn visual_transform_reuses_layout_snapshot_and_tracks_old_and_new_bounds() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let child = widget(&mut world, 10);
    world.insert(child, crate::ui::HitTarget);
    world.insert(root, Children(vec![child]));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();

    world.insert(child, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    seed_prev_rects(&mut world, root, &viewport);
    let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

    crate::ui::property::apply_to_world::<crate::ui::property::prop::Transform>(
        &mut world,
        child,
        Transform::translate(Fixed::from_int(20), Fixed::ZERO),
    );
    prepare_input_geometry(&mut world, root, &viewport).unwrap();
    assert!(world.has::<super::super::dirty::VisualDirty>(child));
    assert_eq!(
        world
            .get::<super::super::dirty::PaintedCoverage>(child)
            .unwrap()
            .0
            .outer,
        Rect::new(0, 0, 10, 10)
    );
    assert_eq!(
        crate::input::event::hit_test::hit_test(&world, root, 25.into(), 5.into(), 64, 64),
        Some(child)
    );
    assert_eq!(
        crate::input::event::hit_test::hit_test(&world, root, 5.into(), 5.into(), 64, 64),
        None
    );
    assert_eq!(
        crate::input::event::hit_test::geometry_changes(&world),
        (false, false)
    );
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..16 {
            prepare_input_geometry(&mut world, root, &viewport).unwrap();
        }
    });
    assert_eq!(allocations, 0);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

    assert_eq!(
        plan.rects,
        [Rect::new(0, 0, 10, 10), Rect::new(20, 0, 10, 10)]
    );
    assert_eq!(plan.bounding_rect(), Some(Rect::new(0, 0, 30, 10)));
    assert_eq!(
        world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
        snapshot
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(child).unwrap().0,
        Rect::new(0, 0, 10, 10)
    );
}

#[test]
fn sparse_dirty_regions_keep_bounded_flush_rects() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let children: Vec<_> = (0..5).map(|_| widget(&mut world, 10)).collect();
    world.insert(root, Children(children.clone()));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    seed_prev_rects(&mut world, root, &viewport);

    world.insert(children[0], Dirty);
    world.insert(children[2], Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(
        plan.rects,
        [Rect::new(0, 0, 10, 10), Rect::new(20, 0, 10, 10)]
    );
    assert_eq!(plan.bounding_rect(), Some(Rect::new(0, 0, 30, 10)));

    for child in children {
        world.insert(child, Dirty);
    }
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    assert_eq!(plan.rects, [Rect::new(0, 0, 50, 10)]);
}

#[test]
fn previous_scroll_rect_scratch_reuses_capacity() {
    let mut world = World::new();
    let root = widget(&mut world, 64);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut plan = DirtyRegions::default();
    world.insert(
        root,
        super::super::dirty::PaintedCoverage(crate::ui::dirty::PaintCoverage::solid(Rect::new(
            0, 0, 64, 10,
        ))),
    );

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.painted_damage.len(), 1);
    let scratch_ptr = snapshot.painted_damage.as_ptr();

    world.insert(root, Dirty);
    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(snapshot.painted_damage.len(), 1);
    assert_eq!(snapshot.painted_damage.as_ptr(), scratch_ptr);
}
