use super::text_layout::{
    TextInputReuse, TextLayoutFailure, apply_input_intrinsic, apply_static_glyph_intrinsic,
    apply_text_intrinsic, layout_text_node, layout_text_tree, prepare_text_geometry,
    publish_text_geometry, reuse_text_inputs, reuse_text_intrinsic, visible_text_owner,
};
use super::{output_plan, prepared_paint::PreparedPaint};
use crate::ecs::{Entity, World};
use crate::text::TextGeometry;
use crate::types::{Dimension, Fixed, Rect, Viewport};
use crate::ui::layout::{LayoutNode, LayoutStyle, compute_layout};
use crate::ui::view::ViewRegistry;
use crate::ui::{self, Children, Outputs, Parent, Style, Widget};
use alloc::{boxed::Box, vec::Vec};

#[cfg(test)]
use super::local_layout_tests;

fn layout_node(
    world: &World,
    entity: Entity,
    active: bool,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    let Some(_) = world.get::<Widget>(entity) else {
        return Ok(None);
    };
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(None);
    };
    let mut node = LayoutNode::new(layout_style(world, entity, style.layout));
    node.entity = Some(entity);
    if active {
        apply_text_intrinsic(world, entity, &mut node)?;
        apply_static_glyph_intrinsic(world, entity, style, &mut node);
        apply_input_intrinsic(world, entity, style, &mut node);
    }
    Ok(Some(node))
}

/// Recursively build a LayoutNode tree from ECS entities.
pub(super) fn build_layout_tree(
    world: &World,
    entity: Entity,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    if ui::branch::is_authored_hidden(world, entity) {
        return Ok(None);
    }
    let Some(mut node) = layout_node(world, entity, true)? else {
        return Ok(None);
    };

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if let Some(child_node) = build_layout_tree(world, child)? {
                node.add_child(child_node);
            }
        }
    }
    Ok(Some(node))
}

#[derive(Default)]
struct LayoutStorageDemand {
    nodes: usize,
    parked_roots: usize,
    max_entity_id: u32,
    missing_computed: usize,
    missing_visual_dirty: usize,
    missing_prev: usize,
    missing_text_handles: usize,
    missing_input_handles: usize,
    missing_scroll_bounds: usize,
}

fn count_layout_storage(world: &World, entity: Entity, demand: &mut LayoutStorageDemand) {
    if world.get::<Widget>(entity).is_none() || world.get::<Style>(entity).is_none() {
        return;
    }
    demand.nodes = demand.nodes.saturating_add(1);
    if world.has::<ui::Hidden>(entity) || world.has::<ui::branch::CachedBranchVisibility>(entity) {
        demand.parked_roots = demand.parked_roots.saturating_add(1);
    }
    demand.max_entity_id = demand.max_entity_id.max(entity.id);
    demand.missing_computed += usize::from(world.get::<ui::ComputedRect>(entity).is_none());
    demand.missing_visual_dirty += usize::from(!world.has::<ui::dirty::VisualDirty>(entity));
    demand.missing_prev += usize::from(world.get::<ui::dirty::PaintedCoverage>(entity).is_none());
    demand.missing_scroll_bounds += usize::from(
        world.has::<crate::input::event::scroll::ScrollOffset>(entity)
            && !world.has::<crate::input::event::scroll::ScrollBounds>(entity),
    );
    demand.missing_text_handles += usize::from(
        world.get::<crate::ui::widgets::Text>(entity).is_some()
            && world.get::<crate::text::TextLayoutHandle>(entity).is_none(),
    );
    demand.missing_input_handles += usize::from(
        world.has::<ui::widgets::TextEditing>(entity)
            && !world.has::<ui::widgets::text_editing::geometry::TextEditingLayoutHandle>(entity),
    );
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            count_layout_storage(world, child, demand);
        }
    }
}

fn build_retained_layout_tree(
    world: &World,
    entity: Entity,
    active: bool,
    parked: &mut Vec<LayoutNode>,
    parked_changed: &mut bool,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    let Some(mut node) = layout_node(world, entity, active)? else {
        return Ok(None);
    };
    if let Some(children) = world.get::<Children>(entity) {
        node.children.reserve_exact(children.0.len());
        for &child in &children.0 {
            let child_active = active && !ui::branch::is_authored_hidden(world, child);
            if let Some(child_node) =
                build_retained_layout_tree(world, child, child_active, parked, parked_changed)?
            {
                if child_active {
                    node.children.push(child_node);
                } else if active {
                    parked.push(child_node);
                    *parked_changed = true;
                } else {
                    node.children.push(child_node);
                }
            }
        }
    }
    Ok(Some(node))
}

fn take_parked_layout_node(
    parked: &mut Vec<LayoutNode>,
    entity: Entity,
    parked_changed: &mut bool,
) -> Option<LayoutNode> {
    parked
        .iter()
        .position(|node| node.entity == Some(entity))
        .map(|index| {
            *parked_changed = true;
            parked.swap_remove(index)
        })
}

fn park_layout_node(
    world: &mut World,
    parked: &mut Vec<LayoutNode>,
    node: LayoutNode,
    parked_changed: &mut bool,
) {
    if let Some(entity) = node.entity {
        world.invalidate_subtree_paint(entity);
    }
    if let Some(index) = parked.iter().position(|other| other.entity == node.entity) {
        parked[index] = node;
    } else {
        parked.push(node);
    }
    *parked_changed = true;
}

fn layout_entity_reachable(world: &World, root: Entity, target: Entity) -> bool {
    if !world.has::<Widget>(root) || !world.has::<Style>(root) {
        return false;
    }
    if root == target {
        return true;
    }
    world.get::<Children>(root).is_some_and(|children| {
        children
            .0
            .iter()
            .any(|&child| layout_entity_reachable(world, child, target))
    })
}

fn collect_layout_reachability(world: &World, root: Entity, out: &mut Vec<Entity>) -> bool {
    if !world.has::<Widget>(root) || !world.has::<Style>(root) {
        return true;
    }
    if out.try_reserve(1).is_err() {
        return false;
    }
    out.push(root);
    world.get::<Children>(root).is_none_or(|children| {
        children
            .0
            .iter()
            .all(|&child| collect_layout_reachability(world, child, out))
    })
}

fn prune_parked_children(world: &World, node: &mut LayoutNode) {
    let Some(parent) = node.entity else {
        return;
    };
    node.children.retain(|child| {
        child.entity.is_some_and(|entity| {
            world.is_alive(entity)
                && world.has::<Widget>(entity)
                && world.has::<Style>(entity)
                && world
                    .get::<Children>(parent)
                    .is_some_and(|children| children.0.contains(&entity))
        })
    });
    for child in &mut node.children {
        prune_parked_children(world, child);
    }
}

fn prune_parked_layout_nodes(
    world: &World,
    root: Entity,
    active_entities: &[Entity],
    parked: &mut Vec<LayoutNode>,
    reachable: &mut Vec<Entity>,
) {
    if parked.is_empty() {
        reachable.clear();
        if reachable.capacity() > active_entities.len().max(1).saturating_mul(4) {
            *reachable = Vec::new();
        }
        return;
    }
    reachable.clear();
    let indexed = collect_layout_reachability(world, root, reachable);
    if indexed {
        reachable.sort_unstable_by_key(|entity| (entity.id, entity.generation));
    }
    parked.retain(|node| {
        node.entity.is_some_and(|entity| {
            world.is_alive(entity)
                && if indexed {
                    reachable
                        .binary_search_by_key(&(entity.id, entity.generation), |e| {
                            (e.id, e.generation)
                        })
                        .is_ok()
                } else {
                    layout_entity_reachable(world, root, entity)
                }
                && !active_entities.contains(&entity)
        })
    });
    for node in parked.iter_mut() {
        prune_parked_children(world, node);
    }
    if parked.is_empty() || reachable.capacity() > reachable.len().max(1).saturating_mul(4) {
        *reachable = Vec::new();
    }
}

#[derive(Default)]
pub(super) struct LayoutTreeChanges {
    pub(super) parked: bool,
    structure: bool,
    reuse_inputs: TextInputReuse,
    solve: ui::layout::solve::SolveScope,
}

pub(super) fn refresh_layout_tree(
    world: &mut World,
    entity: Entity,
    node: &mut LayoutNode,
    parked: &mut Vec<LayoutNode>,
    changes: &mut LayoutTreeChanges,
) -> Result<bool, TextLayoutFailure> {
    if node.entity != Some(entity)
        || world.get::<Widget>(entity).is_none()
        || ui::branch::is_authored_hidden(world, entity)
    {
        return Ok(false);
    }
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(false);
    };
    refresh_layout_values(world, entity, node, style, changes)?;

    refresh_layout_children(world, entity, node, parked, changes)?;
    Ok(true)
}

fn refresh_layout_values(
    world: &World,
    entity: Entity,
    node: &mut LayoutNode,
    style: &Style,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let next_style = layout_style(world, entity, style.layout);
    if changes.reuse_inputs.enabled()
        && node.style == next_style
        && !world.has::<ui::dirty::Dirty>(entity)
        && !world.has::<crate::ui::widgets::Text>(entity)
        && !world.has::<crate::ui::widgets::StaticGlyphRun>(entity)
    {
        return Ok(());
    }
    #[cfg(test)]
    local_layout_tests::record_input_visit(false);
    let observe_solve = !changes.structure && changes.solve != ui::layout::solve::SolveScope::Full;
    let was_absolute = node.style.position == ui::layout::Position::Absolute;
    let style_changed = observe_solve && node.style != next_style;
    let old_intrinsic = (node.intrinsic_width, node.intrinsic_height);
    node.style = next_style;
    node.intrinsic_width = None;
    node.intrinsic_height = None;
    let reused = changes.reuse_inputs.enabled()
        && reuse_text_inputs(world, entity, changes.reuse_inputs)
        && reuse_text_intrinsic(world, entity, style, node);
    if !reused {
        apply_text_intrinsic(world, entity, node)?;
    }
    apply_static_glyph_intrinsic(world, entity, style, node);
    apply_input_intrinsic(world, entity, style, node);
    if observe_solve
        && (style_changed || old_intrinsic != (node.intrinsic_width, node.intrinsic_height))
    {
        changes.solve.observe(node, was_absolute);
    }
    Ok(())
}

// The snapshot's maintenance revisions certify membership and child order.
// Unchanged values reuse measurements from the common text cache.
pub(super) fn refresh_stable_layout_tree(
    world: &World,
    node: &mut LayoutNode,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let entity = node.entity.expect("retained layout entity");
    let style = world.get::<Style>(entity).expect("retained layout style");
    refresh_layout_values(world, entity, node, style, changes)?;
    for child in &mut node.children {
        refresh_stable_layout_tree(world, child, changes)?;
    }
    Ok(())
}

fn refresh_layout_children(
    world: &mut World,
    entity: Entity,
    node: &mut LayoutNode,
    parked: &mut Vec<LayoutNode>,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let mut used = 0;
    if let Some(count) = world
        .get::<Children>(entity)
        .map(|children| children.0.len())
    {
        for index in 0..count {
            let child = world.get::<Children>(entity).unwrap().0[index];
            if world.get::<Widget>(child).is_none()
                || world.get::<Style>(child).is_none()
                || ui::branch::is_authored_hidden(world, child)
            {
                continue;
            }
            if let Some(index) = node.children[used..]
                .iter()
                .position(|candidate| candidate.entity == Some(child))
            {
                changes.structure |= index != 0;
                node.children.swap(used, used + index);
            } else {
                let replacement = match take_parked_layout_node(parked, child, &mut changes.parked)
                {
                    Some(node) => Some(node),
                    None => {
                        build_retained_layout_tree(world, child, true, parked, &mut changes.parked)?
                    }
                };
                let Some(replacement) = replacement else {
                    continue;
                };
                changes.structure = true;
                if used < node.children.len() {
                    let displaced = core::mem::replace(&mut node.children[used], replacement);
                    park_layout_node(world, parked, displaced, &mut changes.parked);
                } else {
                    node.children.push(replacement);
                }
            }
            if refresh_layout_tree(world, child, &mut node.children[used], parked, changes)? {
                used += 1;
            }
        }
    }
    while node.children.len() > used {
        changes.structure = true;
        park_layout_node(
            world,
            parked,
            node.children.pop().expect("trailing layout child"),
            &mut changes.parked,
        );
    }
    Ok(())
}

fn layout_style(world: &World, entity: Entity, mut layout: LayoutStyle) -> LayoutStyle {
    ui::widgets::follow_bounds::layout_style(world, entity, &mut layout);
    if world.get::<ui::RespectSafeArea>(entity).is_none() {
        return layout;
    }
    let Some(insets) = world.resource::<crate::surface::SafeAreaInsets>().copied() else {
        return layout;
    };
    layout.padding.top = safe_area_padding(layout.padding.top, insets.top);
    layout.padding.right = safe_area_padding(layout.padding.right, insets.right);
    layout.padding.bottom = safe_area_padding(layout.padding.bottom, insets.bottom);
    layout.padding.left = safe_area_padding(layout.padding.left, insets.left);
    layout
}

fn safe_area_padding(authored: Dimension, inset: Fixed) -> Dimension {
    match authored {
        Dimension::Px(value) => Dimension::Px(value.max(inset)),
        Dimension::Auto | Dimension::Content => Dimension::Px(inset),
        Dimension::Percent(_) => authored,
    }
}

// Incremental callers have delivered input invalidations before entering here.
// Explicit layout preparation continues to inspect all current inputs.
fn prepare_local_layout(
    world: &mut World,
    snapshot: &mut LayoutSnapshot,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
) -> Result<bool, TextLayoutFailure> {
    use ui::layout::{Position, solve::SolveScope};

    if !snapshot.matches(root, logical_w, logical_h)
        || snapshot.bindings_pending
        || !snapshot.maintenance_current(world)
        || !snapshot.scroll_config_current(world)
    {
        return Ok(false);
    }
    // Parked branches retain their pending work. Only the active snapshot
    // participates in this solve; unrelated hidden dirt must not move its
    // parent into a full-tree preparation.
    let mut dirty = snapshot
        .entities
        .iter()
        .copied()
        .filter(|entity| world.has::<ui::dirty::Dirty>(*entity));
    let Some(target) = dirty.next() else {
        return Ok(false);
    };
    if dirty.next().is_some() {
        return Ok(false);
    }
    if target == root {
        return Ok(false);
    }
    let Some(node) = snapshot.paths.node_mut(&mut snapshot.layout_tree, target) else {
        return Ok(false);
    };
    if !node.children.is_empty() || node.style.position != Position::Absolute {
        return Ok(false);
    }
    let Some(style) = world.get::<Style>(target) else {
        return Ok(false);
    };
    let mut candidate = LayoutNode::new(node.style);
    candidate.entity = Some(target);
    candidate.rect = node.rect;
    candidate.intrinsic_width = node.intrinsic_width;
    candidate.intrinsic_height = node.intrinsic_height;
    let mut changes = LayoutTreeChanges::default();
    refresh_layout_values(world, target, &mut candidate, style, &mut changes)?;
    if changes.solve == SolveScope::Full {
        return Ok(false);
    }
    node.style = candidate.style;
    node.intrinsic_width = candidate.intrinsic_width;
    node.intrinsic_height = candidate.intrinsic_height;
    if changes.solve != SolveScope::Unchanged && !changes.solve.apply(&mut snapshot.layout_tree) {
        snapshot.invalidate();
        return Ok(false);
    }
    #[cfg(test)]
    if changes.solve != SolveScope::Unchanged {
        local_layout_tests::record_solve(true);
    }
    let node = snapshot
        .paths
        .node_mut(&mut snapshot.layout_tree, target)
        .expect("prepared leaf");
    if layout_text_node(node, world, target, TextInputReuse::Disabled)? {
        snapshot.invalidate();
        return Ok(false);
    }
    snapshot.prepared_paint.resize(
        snapshot.entities.len(),
        world
            .resource::<ViewRegistry>()
            .map_or(0, |registry| registry.paint_iter().len()),
    );
    #[cfg(test)]
    local_layout_tests::record_preparation();
    Ok(true)
}

pub(super) fn compute_layout_snapshot(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    incremental: bool,
) -> Result<Option<Box<LayoutSnapshot>>, TextLayoutFailure> {
    world.watch_component_revision::<crate::input::event::scroll::ScrollConfig>();
    LayoutSnapshot::watch_maintenance(world);
    output_plan::invalidate_geometry(world);
    let mut previous = world.take_resource_box::<LayoutSnapshot>();
    let font_inputs = FontInputs::capture(world);
    let text_inputs = TextInputRevisions::capture(world);
    if ui::branch::is_authored_hidden(world, root) {
        output_plan::invalidate_structure(world);
        if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
            cache
                .storage_mut()
                .layouts
                .begin_frame_with_owner_filter(|_| false);
        }
        return Ok(None);
    }
    if incremental
        && let Some(snapshot) = &mut previous
        && snapshot.font_inputs.matches(&font_inputs)
    {
        match prepare_local_layout(world, snapshot, root, logical_w, logical_h) {
            Ok(true) => return Ok(previous),
            Ok(false) => {}
            Err(error) => {
                snapshot.invalidate();
                if let Some(snapshot) = previous {
                    world.put_resource_box(snapshot);
                }
                return Err(error);
            }
        }
    }
    let local_solve = previous.as_ref().is_some_and(|snapshot| {
        snapshot.matches(root, logical_w, logical_h) && !snapshot.bindings_pending
    });
    let reuse_inputs = if incremental
        && local_solve
        && previous.as_ref().is_some_and(|snapshot| {
            snapshot.maintenance_current(world) && snapshot.font_inputs.matches(&font_inputs)
        }) {
        if text_inputs.is_some()
            && previous.as_ref().and_then(|snapshot| snapshot.text_inputs) == text_inputs
        {
            TextInputReuse::UnchangedInputs
        } else {
            TextInputReuse::CleanNodes
        }
    } else {
        TextInputReuse::Disabled
    };
    let initial_demand = (!previous
        .as_ref()
        .is_some_and(|snapshot| snapshot.root == root))
    .then(|| {
        let mut demand = LayoutStorageDemand::default();
        count_layout_storage(world, root, &mut demand);
        demand
    });
    if let Some(demand) = &initial_demand {
        if demand.parked_roots > 0 {
            world.reserve_dirty_traversal_stack();
        }
        if demand.missing_visual_dirty > 0 {
            world.reserve_component_storage::<ui::dirty::VisualDirty>(
                demand.max_entity_id,
                demand.missing_visual_dirty,
            );
        }
        if demand.missing_scroll_bounds > 0 {
            world.reserve_component_storage::<crate::input::event::scroll::ScrollBounds>(
                demand.max_entity_id,
                demand.missing_scroll_bounds,
            );
        }
        if demand.missing_computed > 0 {
            world.reserve_component_storage::<ui::ComputedRect>(
                demand.max_entity_id,
                demand.missing_computed,
            );
        }
        if demand.missing_prev > 0 {
            world.reserve_component_storage::<ui::dirty::PaintedCoverage>(
                demand.max_entity_id,
                demand.missing_prev,
            );
        }
        if demand.missing_text_handles > 0 {
            world.reserve_component_storage::<crate::text::TextLayoutHandle>(
                demand.max_entity_id,
                demand.missing_text_handles,
            );
        }
        if demand.missing_input_handles > 0 {
            world.reserve_component_storage::<ui::widgets::text_editing::geometry::TextEditingLayoutHandle>(
                demand.max_entity_id,
                demand.missing_input_handles,
            );
        }
    }
    let mut entities = previous
        .as_mut()
        .map(|snapshot| core::mem::take(&mut snapshot.entities))
        .unwrap_or_default();
    entities.clear();
    if let Some(demand) = &initial_demand {
        entities.reserve(demand.nodes);
    }
    collect_entities_preorder(world, root, &mut entities);
    entities.sort_unstable_by_key(|entity| (entity.id, entity.generation));
    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache
            .storage_mut()
            .layouts
            .begin_frame_with_owner_filter(|owner| visible_text_owner(world, &entities, owner));
    }
    entities.clear();
    let mut changes = LayoutTreeChanges {
        reuse_inputs,
        solve: if local_solve {
            ui::layout::solve::SolveScope::Unchanged
        } else {
            ui::layout::solve::SolveScope::Full
        },
        ..Default::default()
    };
    let mut snapshot = crate::trace_span!("layout.build_tree", {
        match previous {
            Some(mut snapshot) if snapshot.root == root => {
                let refreshed = if snapshot.maintenance_current(world) {
                    refresh_stable_layout_tree(world, &mut snapshot.layout_tree, &mut changes)
                        .map(|()| true)
                } else {
                    refresh_layout_tree(
                        world,
                        root,
                        &mut snapshot.layout_tree,
                        &mut snapshot.parked,
                        &mut changes,
                    )
                };
                if changes.structure || !matches!(refreshed, Ok(true)) {
                    snapshot.invalidate_geometry_plan();
                    output_plan::invalidate_structure(world);
                }
                match refreshed {
                    Ok(true) => {}
                    Ok(false) => return Ok(None),
                    Err(failure) => {
                        snapshot.valid = false;
                        snapshot.entities = entities;
                        world.put_resource_box(snapshot);
                        return Err(failure);
                    }
                }
                snapshot.entities = entities;
                snapshot
            }
            _ => {
                output_plan::invalidate_structure(world);
                let node_count = initial_demand.as_ref().map_or(0, |demand| demand.nodes);
                let parked_capacity = initial_demand
                    .as_ref()
                    .map_or(0, |demand| demand.parked_roots);
                let mut parked = Vec::with_capacity(parked_capacity);
                let Some(layout_tree) = build_retained_layout_tree(
                    world,
                    root,
                    true,
                    &mut parked,
                    &mut changes.parked,
                )?
                else {
                    return Ok(None);
                };
                Box::new(LayoutSnapshot {
                    root,
                    logical_w,
                    logical_h,
                    valid: false,
                    bindings_pending: false,
                    scroll_config_revision: 0,
                    maintenance_revisions: [0; 6],
                    font_inputs: FontInputs::default(),
                    text_inputs: None,
                    layout_tree,
                    parked,
                    reachable: Vec::new(),
                    entities,
                    painted_damage: Vec::with_capacity(node_count),
                    prepared_paint: PreparedPaint::with_capacity(node_count),
                    paths: ui::layout::paths::LayoutPaths::default(),
                    layout_participants: Default::default(),
                })
            }
        }
    });
    crate::trace_span!("layout.initial_compute", {
        let solved_locally =
            local_solve && !changes.structure && changes.solve.apply(&mut snapshot.layout_tree);
        #[cfg(test)]
        local_layout_tests::record_solve(solved_locally);
        if !solved_locally {
            compute_layout(
                &mut snapshot.layout_tree,
                Fixed::ZERO,
                Fixed::ZERO,
                logical_w.into(),
                logical_h.into(),
            );
        }
    });
    crate::trace_span!("layout.collect_entities", {
        snapshot.entities.clear();
        collect_entities_preorder(world, root, &mut snapshot.entities);
    });
    snapshot.prepared_paint.resize(
        snapshot.entities.len(),
        world
            .resource::<ViewRegistry>()
            .map_or(0, |registry| registry.paint_iter().len()),
    );
    snapshot
        .prepared_paint
        .prepare_subtrees(&snapshot.layout_tree, changes.structure);
    if !snapshot.maintenance_current(world) || changes.parked {
        crate::trace_span!("layout.prune_parked", {
            prune_parked_layout_nodes(
                world,
                root,
                &snapshot.entities,
                &mut snapshot.parked,
                &mut snapshot.reachable,
            );
        });
    }
    let mut text_index = 0;
    let intrinsic_changed = crate::trace_span!("layout.text", {
        layout_text_tree(
            &mut snapshot.layout_tree,
            world,
            &snapshot.entities,
            &mut text_index,
            reuse_inputs,
        )
    });
    let intrinsic_changed = match intrinsic_changed {
        Ok(changed) => changed,
        Err(failure) => {
            snapshot.valid = false;
            world.put_resource_box(snapshot);
            return Err(failure);
        }
    };
    if intrinsic_changed {
        crate::trace_span!("layout.final_compute", {
            #[cfg(test)]
            local_layout_tests::record_solve(false);
            compute_layout(
                &mut snapshot.layout_tree,
                Fixed::ZERO,
                Fixed::ZERO,
                logical_w.into(),
                logical_h.into(),
            )
        });
    }
    snapshot.logical_w = logical_w;
    snapshot.logical_h = logical_h;
    snapshot.maintenance_revisions = LayoutSnapshot::maintenance_revisions(world);
    snapshot.font_inputs = font_inputs;
    snapshot.text_inputs = text_inputs;
    snapshot.valid = true;
    Ok(Some(snapshot))
}

pub(super) fn collect_entities_preorder(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if ui::branch::is_authored_hidden(world, entity) {
        return;
    }
    out.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            collect_entities_preorder(world, child, out);
        }
    }
}

pub(super) fn reset_paint_preparation(world: &World) {
    if let Some(snapshot) = world.resource::<LayoutSnapshot>() {
        if let Some(registry) = world.resource::<ViewRegistry>() {
            snapshot.prepared_paint.clear_for_frame(world, registry);
        } else {
            snapshot.prepared_paint.clear();
        }
    }
}

pub(super) fn reconcile_layout_snapshot(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    reuse_visual_layout: bool,
) -> Result<Option<Box<LayoutSnapshot>>, TextLayoutFailure> {
    const MAX_LAYOUT_BINDING_UPDATES: usize = 32;
    let binding_count = world
        .storage::<ui::layout_binding::LayoutBinding>()
        .map_or(0, |storage| storage.len())
        + crate::input::event::runtime::size_observer_count(world)
        + ui::view::ViewRegistry::prepared_layout_count(world);
    let update_budget = if world
        .query::<ui::widgets::LazyList>()
        .iter()
        .next()
        .is_some()
    {
        MAX_LAYOUT_BINDING_UPDATES
    } else {
        binding_count
            .saturating_add(1)
            .min(MAX_LAYOUT_BINDING_UPDATES)
    };
    let pass_count = update_budget.saturating_add(1);
    let mut prepared_revision = None;

    for pass in 0..pass_count {
        let layout_unchanged = if pass == 0 {
            reuse_visual_layout && !crate::input::event::hit_test::geometry_changes(world).0
        } else {
            prepared_revision.is_some()
                && prepared_revision
                    == world.tracked_component_revision::<crate::ui::dirty::Dirty>()
        };
        let reuse = layout_unchanged
            && !ui::branch::is_authored_hidden(world, root)
            && world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
                snapshot.matches(root, logical_w, logical_h)
                    && !snapshot.bindings_pending
                    && snapshot.scroll_config_current(world)
            });
        let snapshot = if reuse {
            world.take_resource_box::<LayoutSnapshot>()
        } else {
            compute_layout_snapshot(
                world,
                root,
                logical_w,
                logical_h,
                reuse_visual_layout && pass == 0,
            )?
        };
        let Some(mut snapshot) = snapshot else {
            return Ok(None);
        };

        let mut idx = 0;
        write_computed_rects(&snapshot.layout_tree, world, &snapshot.entities, &mut idx);
        snapshot.prepare_geometry_plan(world);
        prepared_revision = world.tracked_component_revision::<crate::ui::dirty::Dirty>();
        let participated = snapshot.layout_participants.reconcile(
            world,
            &mut ui::prepared_layout::PreparedLayout {
                tree: &mut snapshot.layout_tree,
                entities: &snapshot.entities,
                paths: &snapshot.paths,
                viewport: Rect::new(0, 0, logical_w, logical_h),
            },
        );
        if participated == ui::property::PropertyChange::Layout {
            prepared_revision = None;
        }
        let invoke = pass < update_budget;
        let sizes =
            match crate::input::event::runtime::capture_sizes(world, &snapshot.entities, invoke) {
                Ok(sizes) => sizes,
                Err(error) => {
                    crate::warn!(target: "mirui::layout", "size observation skipped: {:?}", error);
                    None
                }
            };
        world.put_resource_box(snapshot);
        let mut changed =
            participated.changed() | ui::layout_binding::apply_layout_bindings(world, invoke);
        if invoke {
            changed |= ui::widgets::lazy_list::reconcile(world);
        }
        if let Some(sizes) = sizes {
            changed |= sizes.changed;
            if invoke {
                sizes.dispatch(world, ui::lifecycle::prepare);
            }
        }
        if !changed || !invoke {
            if changed {
                crate::warn!(target: "mirui::layout", "layout bindings did not settle after {} updates", update_budget);
            }
            let mut snapshot = world.take_resource_box::<LayoutSnapshot>();
            if let Some(snapshot) = &mut snapshot {
                snapshot.bindings_pending = changed;
            }
            return Ok(snapshot);
        }
        ui::flush_bindings(world);
    }
    Ok(None)
}

pub(super) fn record_text_layout_failure(
    world: &mut World,
    root: Entity,
    failure: TextLayoutFailure,
) {
    if let Some(outputs) = world.resource::<Outputs<ui::LayoutGeometry>>() {
        outputs.reset_all();
    }
    if let Some(snapshot) = world.resource_mut::<LayoutSnapshot>() {
        snapshot.valid = false;
        snapshot.invalidate_geometry_plan();
    }
    crate::input::event::hit_test::invalidate_hit_test_geometry(world);
    world.mark_subtree_dirty(root);
    world.insert_resource(failure);
}

/// Compute layout, reporting text capacity and shaping failures before drawing.
/// The last failure is also available as a [`TextLayoutFailure`] world resource.
pub fn try_update_layout(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
) -> Result<(), TextLayoutFailure> {
    update_layout(world, root, transform, false)
}

fn update_layout(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
    reuse_visual_layout: bool,
) -> Result<(), TextLayoutFailure> {
    ui::view::ViewRegistry::invalidate_queries(world);
    let (logical_w, logical_h) = transform.logical_size();

    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache.storage_mut().paths.begin_layout();
    }

    let snapshot =
        match reconcile_layout_snapshot(world, root, logical_w, logical_h, reuse_visual_layout) {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                record_text_layout_failure(world, root, failure);
                return Err(failure);
            }
        };
    let Some(mut snapshot) = snapshot else {
        if let Some(outputs) = world.resource::<Outputs<ui::LayoutGeometry>>() {
            outputs.reset_all();
        }
        crate::input::event::scroll::bounds::publish(world, &[]);
        world.remove_resource::<TextLayoutFailure>();
        ui::view::ViewRegistry::invalidate_queries(world);
        return Ok(());
    };
    if let Err(failure) = prepare_text_geometry(world, &snapshot.entities) {
        world.put_resource_box(snapshot);
        record_text_layout_failure(world, root, failure);
        return Err(failure);
    }
    crate::input::event::hit_test::update_hit_test_geometry(
        world,
        root,
        logical_w,
        logical_h,
        &snapshot.layout_tree,
        &snapshot.entities,
        &|index| snapshot.prepared_paint.subtree_end(index),
    );
    ui::view::ViewRegistry::invalidate_queries(world);
    if let Err(failure) = publish_text_geometry(world, &snapshot.entities) {
        world.put_resource_box(snapshot);
        record_text_layout_failure(world, root, failure);
        return Err(failure);
    }
    snapshot.prepare_geometry_plan(world);
    ui::layout_geometry::publish(
        world,
        &snapshot.layout_tree,
        &snapshot.entities,
        &snapshot.paths,
        Rect::new(0, 0, logical_w, logical_h),
    );
    ui::view::ViewRegistry::propagate_output_publications(world);
    snapshot.publish_scroll_bounds(world);
    world.put_resource_box(snapshot);
    world.remove_resource::<TextLayoutFailure>();
    Ok(())
}

pub(crate) fn prepare_input_geometry(
    world: &mut World,
    root: Entity,
    viewport: &Viewport,
) -> Result<(), TextLayoutFailure> {
    use crate::input::event::hit_test;
    hit_test::prepare_geometry_tracking(world);
    if ui::branch::is_authored_hidden(world, root) {
        if let Some(outputs) = world.resource::<Outputs<ui::LayoutGeometry>>() {
            outputs.reset_all();
        }
        hit_test::invalidate_hit_test_geometry(world);
        return Ok(());
    }
    ui::view::ViewRegistry::invalidate_queries(world);
    let (width, height) = viewport.logical_size();
    let (_, visual_changed) = hit_test::geometry_changes(world);
    if visual_changed && ui::widgets::lazy_list::reconcile(world) {
        ui::flush_bindings(world);
        ui::view::ViewRegistry::invalidate_queries(world);
    }
    let (layout_changed, visual_changed) = hit_test::geometry_changes(world);
    let layout_current = world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
        snapshot.matches(root, width, height)
            && !snapshot.bindings_pending
            && snapshot.scroll_config_current(world)
    });
    if layout_changed
        || !layout_current
        || (visual_changed && ui::view::ViewRegistry::prepared_layout_count(world) != 0)
    {
        return update_layout(world, root, viewport, true);
    }
    if visual_changed || !hit_test::geometry_matches(world, root, width, height) {
        world.with_resource_box::<LayoutSnapshot, _>(|snapshot, world| {
            hit_test::update_hit_test_geometry(
                world,
                root,
                width,
                height,
                &snapshot.layout_tree,
                &snapshot.entities,
                &|index| snapshot.prepared_paint.subtree_end(index),
            );
            snapshot.prepare_geometry_plan(world);
            ui::layout_geometry::publish(
                world,
                &snapshot.layout_tree,
                &snapshot.entities,
                &snapshot.paths,
                Rect::new(0, 0, width, height),
            );
        });
        ui::view::ViewRegistry::propagate_output_publications(world);
    }
    Ok(())
}

pub(crate) fn write_computed_rects(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
) {
    if *idx < entities.len() {
        let entity = entities[*idx];
        if world
            .get::<ui::ComputedRect>(entity)
            .is_none_or(|rect| rect.0 != node.rect)
        {
            world.insert(entity, ui::ComputedRect(node.rect));
            world.invalidate_visual(entity);
        }
    }
    *idx += 1;
    for child in &node.children {
        write_computed_rects(child, world, entities, idx);
    }
}

#[derive(Default)]
struct FontInputs {
    revision: Option<alloc::rc::Rc<core::cell::Cell<u64>>>,
    observed: u64,
    providers: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct TextInputRevisions {
    content: u64,
    style_metrics: u64,
}

impl TextInputRevisions {
    fn capture(world: &mut World) -> Option<Self> {
        use crate::ecs::world::ComponentChangeKind;
        use crate::ui::widgets::Text;

        let style_metrics = match world.filtered_component_revision::<Style>() {
            Some(revision) => revision,
            None => {
                if !world.has_any_by_id(core::any::TypeId::of::<Text>()) {
                    return None;
                }
                world.watch_component_revision::<Text>();
                world.watch_filtered_component_revision::<Style>(|kind| match kind {
                    ComponentChangeKind::Component => true,
                    ComponentChangeKind::InheritedStyle(fields) => {
                        ui::style::metrics_changed(fields)
                    }
                    _ => false,
                });
                world.filtered_component_revision::<Style>()?
            }
        };
        Some(Self {
            content: world.tracked_component_revision::<Text>()?,
            style_metrics,
        })
    }
}

impl FontInputs {
    fn capture(world: &World) -> Self {
        use core::hash::{Hash, Hasher};
        let fonts = world.resource::<crate::render::font::FontManager>();
        let revision = fonts.map(|fonts| fonts.revision_cell());
        let observed = revision.as_ref().map_or(0, |revision| revision.get());
        // Providers may advance independently of their manager. Inspect the
        // resident faces once per preparation, without resolving tokens or
        // changing cache recency. Order-independent accumulation survives LRU
        // reordering; manager revisions cover token-to-value replacement.
        let mut providers = 0_u64;
        if let Some(fonts) = fonts {
            fonts.visit_resident(|font| {
                let mut hash = rustc_hash::FxHasher::default();
                (
                    font.face_id().value(),
                    font.revision(),
                    font.size,
                    alloc::rc::Rc::as_ptr(&font.provider) as *const () as usize,
                )
                    .hash(&mut hash);
                providers = providers.wrapping_add(hash.finish());
            });
        }
        Self {
            revision,
            observed,
            providers,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.observed == other.observed
            && self.providers == other.providers
            && match (&self.revision, &other.revision) {
                (None, None) => true,
                (Some(a), Some(b)) => alloc::rc::Rc::ptr_eq(a, b),
                _ => false,
            }
    }
}

pub(crate) struct LayoutSnapshot {
    pub(super) root: Entity,
    pub(super) logical_w: u16,
    pub(super) logical_h: u16,
    pub(super) valid: bool,
    pub(super) bindings_pending: bool,
    pub(super) scroll_config_revision: u64,
    pub(super) maintenance_revisions: [u64; 6],
    font_inputs: FontInputs,
    text_inputs: Option<TextInputRevisions>,
    pub(crate) layout_tree: LayoutNode,
    pub(super) parked: Vec<LayoutNode>,
    pub(super) reachable: Vec<Entity>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) paths: ui::layout::paths::LayoutPaths,
    pub(crate) layout_participants: ui::prepared_layout::LayoutParticipants,
    pub(super) painted_damage: Vec<Entity>,
    pub(super) prepared_paint: PreparedPaint,
}

impl LayoutSnapshot {
    pub(crate) fn trim_memory(&mut self) {
        self.reachable = Vec::new();
        self.paths.release();
        self.layout_participants.release();
    }

    fn invalidate_geometry_plan(&mut self) {
        self.paths.invalidate();
        self.layout_participants.invalidate();
    }

    pub(crate) fn prepare_geometry_plan(&mut self, world: &mut World) {
        self.layout_participants.prepare(world, &self.entities);
        let outputs = world.resource::<Outputs<ui::LayoutGeometry>>();
        if !outputs.is_some_and(Outputs::has_requests) && !self.layout_participants.has_geometry() {
            self.paths.release();
            return;
        }
        let relation_revision = world
            .resource::<ui::outputs::relations::OutputRelations>()
            .map_or(
                0,
                ui::outputs::relations::OutputRelations::structure_revision,
            );
        self.paths.prepare(
            &self.layout_tree,
            self.entities.len(),
            [relation_revision, self.layout_participants.revision(), 0],
            |entity| {
                outputs.is_some_and(|outputs| outputs.is_requested(entity))
                    || self.layout_participants.needs_geometry(entity)
            },
        );
    }

    fn watch_maintenance(world: &mut World) {
        world.watch_component_revision::<Children>();
        world.watch_component_revision::<Parent>();
        world.watch_component_revision::<ui::Hidden>();
        world.watch_component_revision::<ui::branch::CachedBranchVisibility>();
        world.watch_component_revision::<Widget>();
        world.watch_component_revision::<Style>();
    }

    fn maintenance_revisions(world: &World) -> [u64; 6] {
        [
            world.component_revision::<Children>(),
            world.component_revision::<Parent>(),
            world.component_revision::<ui::Hidden>(),
            world.component_revision::<ui::branch::CachedBranchVisibility>(),
            world.component_membership_revision::<Widget>(),
            world.component_membership_revision::<Style>(),
        ]
    }

    pub(super) fn maintenance_current(&self, world: &World) -> bool {
        self.valid && self.maintenance_revisions == Self::maintenance_revisions(world)
    }

    pub(super) fn scroll_config_current(&self, world: &World) -> bool {
        self.scroll_config_revision
            == world.component_revision::<crate::input::event::scroll::ScrollConfig>()
    }

    pub(super) fn publish_scroll_bounds(&mut self, world: &mut World) {
        crate::input::event::scroll::bounds::publish(world, &self.entities);
        self.scroll_config_revision =
            world.component_revision::<crate::input::event::scroll::ScrollConfig>();
    }

    pub(super) fn matches(&self, root: Entity, logical_w: u16, logical_h: u16) -> bool {
        self.valid
            && self.root == root
            && self.logical_w == logical_w
            && self.logical_h == logical_h
    }

    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
        self.invalidate_geometry_plan();
    }
}
