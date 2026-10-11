use super::{ScrollConfig, ScrollOffset};
use crate::ecs::{Entity, World};
use crate::types::Fixed;

fn descendant_extent(world: &World, entity: Entity) -> Option<(Fixed, Fixed)> {
    let rect = world.get::<crate::ui::ComputedRect>(entity)?.0;
    let mut right = rect.x + rect.w;
    let mut bottom = rect.y + rect.h;
    if world
        .get::<crate::ui::Style>(entity)
        .is_some_and(|style| style.clip_children)
    {
        return Some((right, bottom));
    }
    if let Some(children) = world.get::<crate::ui::Children>(entity) {
        for &child in &children.0 {
            if let Some((child_right, child_bottom)) = descendant_extent(world, child) {
                right = right.max(child_right);
                bottom = bottom.max(child_bottom);
            }
        }
    }
    Some((right, bottom))
}

/// Scrollable geometry from the most recent successful layout. Absent before the first layout.
///
/// Configure content extents through [`ScrollConfig`]; changing configuration does not update this snapshot until layout succeeds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollBounds {
    pub viewport_width: Fixed,
    pub viewport_height: Fixed,
    pub content_width: Fixed,
    pub content_height: Fixed,
    pub max_x: Fixed,
    pub max_y: Fixed,
}

impl crate::ecs::Component for ScrollBounds {}

pub(crate) fn scroll_bounds(world: &World, entity: Entity) -> Option<ScrollBounds> {
    let rect = world.get::<crate::ui::ComputedRect>(entity)?.0;
    let mut content_width = rect.w;
    let mut content_height = rect.h;
    if let Some(children) = world.get::<crate::ui::Children>(entity) {
        for &child in &children.0 {
            if let Some((right, bottom)) = descendant_extent(world, child) {
                content_width = content_width.max(right - rect.x);
                content_height = content_height.max(bottom - rect.y);
            }
        }
    }
    if let Some(config) = world.get::<ScrollConfig>(entity) {
        if config.content_width > Fixed::ZERO {
            content_width = config.content_width;
        }
        if config.content_height > Fixed::ZERO {
            content_height = config.content_height;
        }
    }
    if let Some((horizontal, extent)) = crate::ui::widgets::lazy_list::content_extent(world, entity)
    {
        if horizontal {
            content_width = extent;
            content_height = rect.h;
        } else {
            content_height = extent;
            content_width = rect.w;
        }
    }
    Some(ScrollBounds {
        viewport_width: rect.w,
        viewport_height: rect.h,
        content_width,
        content_height,
        max_x: (content_width - rect.w).max(Fixed::ZERO),
        max_y: (content_height - rect.h).max(Fixed::ZERO),
    })
}

pub(crate) fn publish(world: &mut World, entities: &[Entity]) {
    let mut index = 0;
    while let Some(entity) = world
        .storage::<ScrollBounds>()
        .and_then(|storage| storage.entities().get(index))
        .copied()
    {
        if !world.has::<ScrollOffset>(entity) || !entities.contains(&entity) {
            world.remove::<ScrollBounds>(entity);
            crate::ui::view::ViewRegistry::component_published::<ScrollBounds>(world, entity);
        } else {
            index += 1;
        }
    }
    for &entity in entities {
        if !world.has::<ScrollOffset>(entity) {
            continue;
        }
        if let Some(bounds) = scroll_bounds(world, entity)
            && world.get::<ScrollBounds>(entity) != Some(&bounds)
        {
            world.insert(entity, bounds);
            crate::ui::view::ViewRegistry::component_published::<ScrollBounds>(world, entity);
        }
    }
}

#[cfg(test)]
mod tests;
