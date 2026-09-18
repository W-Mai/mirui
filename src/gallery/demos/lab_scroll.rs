use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::{Children, ComputedRect};

#[derive(Clone, Copy, crate::Component)]
pub(super) struct LabScroll {
    content: Entity,
    trailing_inset: Fixed,
}

impl LabScroll {
    pub(super) fn attach(
        world: &mut World,
        viewport: Entity,
        content: Entity,
        initial_height: Fixed,
        trailing_inset: Fixed,
    ) {
        world.insert(
            viewport,
            Self {
                content,
                trailing_inset,
            },
        );
        world.insert(
            viewport,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(
            viewport,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: initial_height,
                content_width: Fixed::ZERO,
            },
        );
    }

    fn descendant_bottom(world: &World, entity: Entity) -> Option<Fixed> {
        let rect = world.get::<ComputedRect>(entity)?.0;
        let mut bottom = rect.y + rect.h;
        if world
            .get::<Style>(entity)
            .is_some_and(|style| style.clip_children)
        {
            return Some(bottom);
        }
        if let Some(children) = world.get::<Children>(entity) {
            for &child in &children.0 {
                if let Some(child_bottom) = Self::descendant_bottom(world, child) {
                    bottom = bottom.max(child_bottom);
                }
            }
        }
        Some(bottom)
    }

    fn content_bottom(world: &World, content: Entity, origin_y: Fixed) -> Fixed {
        world
            .get::<Children>(content)
            .into_iter()
            .flat_map(|children| children.0.iter().copied())
            .filter_map(|child| Self::descendant_bottom(world, child))
            .fold(origin_y, Fixed::max)
    }

    fn sync(world: &mut World, viewport: Entity) {
        let Some(link) = world.get::<Self>(viewport).copied() else {
            return;
        };
        let (Some(viewport_rect), Some(content_rect)) = (
            world.get::<ComputedRect>(viewport).map(|rect| rect.0),
            world.get::<ComputedRect>(link.content).map(|rect| rect.0),
        ) else {
            return;
        };
        let bottom = Self::content_bottom(world, link.content, content_rect.y);
        let content_height = (bottom - content_rect.y + link.trailing_inset).max(viewport_rect.h);
        let mut relayout = false;
        if let Some(style) = world.get_mut::<Style>(link.content) {
            let height = Dimension::Px(content_height);
            if style.layout.height != height {
                style.layout.height = height;
                relayout = true;
            }
        }
        if let Some(config) = world.get_mut::<ScrollConfig>(viewport) {
            config.content_height = content_height;
        }
        let max_offset = (content_height - viewport_rect.h).max(Fixed::ZERO);
        let mut changed = false;
        if let Some(offset) = world.get_mut::<ScrollOffset>(viewport) {
            let clamped = offset.y.clamp(Fixed::ZERO, max_offset);
            changed = clamped != offset.y;
            offset.y = clamped;
            offset.x = Fixed::ZERO;
        }
        if changed {
            world.invalidate(viewport);
        }
        if relayout {
            world.invalidate(link.content);
        }
    }

    pub(super) fn sync_all(world: &mut World) {
        world.for_each_stable::<Self>(Self::sync);
    }
}

#[mirui_macros::system]
pub(super) fn sync_lab_scroll_extents(world: &mut World) {
    LabScroll::sync_all(world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_tracks_content_and_clamps_stale_offset() {
        let mut world = World::new();
        let viewport = world.spawn(Style::default());
        let content = world.spawn(Style::default());
        let child = world.spawn(Style::default());
        world.insert(viewport, ComputedRect(Rect::new(0, 0, 120, 80)));
        world.insert(content, ComputedRect(Rect::new(0, 0, 120, 80)));
        world.insert(child, ComputedRect(Rect::new(0, 0, 120, 240)));
        world.insert(content, Children(alloc::vec![child]));
        LabScroll::attach(
            &mut world,
            viewport,
            content,
            Fixed::from_int(400),
            Fixed::ZERO,
        );
        world.get_mut::<ScrollOffset>(viewport).unwrap().y = Fixed::from_int(300);

        LabScroll::sync_all(&mut world);

        assert_eq!(
            world.get::<ScrollConfig>(viewport).unwrap().content_height,
            Fixed::from_int(240),
        );
        assert_eq!(
            world.get::<ScrollOffset>(viewport).unwrap().y,
            Fixed::from_int(160),
        );
        assert_eq!(
            world.get::<Style>(content).unwrap().layout.height,
            Dimension::px(240),
        );
    }

    #[test]
    fn short_content_has_no_scroll_range() {
        let mut world = World::new();
        let viewport = world.spawn(Style::default());
        let content = world.spawn(Style::default());
        world.insert(viewport, ComputedRect(Rect::new(0, 0, 120, 160)));
        world.insert(content, ComputedRect(Rect::new(0, 0, 120, 80)));
        LabScroll::attach(
            &mut world,
            viewport,
            content,
            Fixed::from_int(400),
            Fixed::ZERO,
        );
        world.get_mut::<ScrollOffset>(viewport).unwrap().y = Fixed::from_int(10);

        LabScroll::sync_all(&mut world);

        assert_eq!(
            world.get::<ScrollConfig>(viewport).unwrap().content_height,
            Fixed::from_int(160),
        );
        assert_eq!(world.get::<ScrollOffset>(viewport).unwrap().y, Fixed::ZERO);
    }

    #[test]
    fn extent_includes_visible_overflow_but_stops_at_clip_boundaries() {
        let mut world = World::new();
        let viewport = world.spawn(Style::default());
        let content = world.spawn(Style::default());
        let overflow = world.spawn(Style::default());
        let clipped = world.spawn(Style {
            clip_children: true,
            ..Style::default()
        });
        let hidden_overflow = world.spawn(Style::default());

        world.insert(viewport, ComputedRect(Rect::new(0, 0, 120, 80)));
        world.insert(content, ComputedRect(Rect::new(0, 0, 120, 100)));
        world.insert(overflow, ComputedRect(Rect::new(0, 180, 120, 40)));
        world.insert(clipped, ComputedRect(Rect::new(0, 120, 120, 30)));
        world.insert(hidden_overflow, ComputedRect(Rect::new(0, 500, 120, 40)));
        world.insert(content, Children(alloc::vec![overflow, clipped]));
        world.insert(clipped, Children(alloc::vec![hidden_overflow]));
        LabScroll::attach(
            &mut world,
            viewport,
            content,
            Fixed::from_int(400),
            Fixed::from_int(10),
        );

        LabScroll::sync_all(&mut world);

        assert_eq!(
            world.get::<ScrollConfig>(viewport).unwrap().content_height,
            Fixed::from_int(230),
        );
    }
}
