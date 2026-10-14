use crate::ecs::{Component, Entity, World};
use crate::input::event::scroll::ScrollOffset;
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
use crate::ui::Style;
use crate::ui::dirty::RegionShift;

#[derive(Clone, Copy)]
pub(super) struct PaintedScroll {
    pub offset: ScrollOffset,
    area: Rect,
    parent: Option<Entity>,
    reusable: bool,
}

impl Component for PaintedScroll {}

impl PaintedScroll {
    pub fn invalidate_all(world: &mut World) {
        world.for_each_stable::<Self>(|world, entity| {
            world.get_mut::<Self>(entity).unwrap().reusable = false;
        });
    }

    pub fn capture(
        world: &World,
        entity: Entity,
        rect: Rect,
        clip: Option<Rect>,
        affine: Transform,
        projective: Transform3D,
        parent: Option<Entity>,
    ) -> Option<Self> {
        let offset = *world.get::<ScrollOffset>(entity)?;
        Some(Self {
            offset,
            area: clip.map_or(rect, |clip| rect.intersect(&clip).unwrap_or(Rect::ZERO)),
            parent,
            reusable: affine.is_identity()
                && projective.is_identity()
                && world.get::<Style>(entity).is_none_or(|style| {
                    style.border_radius == Fixed::ZERO && style.border_width == Fixed::ZERO
                }),
        })
    }

    pub fn shift_from(
        self,
        painted: Self,
        carrier: Option<Point>,
        scale: Fixed,
    ) -> Option<RegionShift> {
        let carrier = carrier?;
        let moved_area = Rect {
            x: painted.area.x + carrier.x,
            y: painted.area.y + carrier.y,
            ..painted.area
        };
        if !self.reusable
            || !painted.reusable
            || self.parent != painted.parent
            || self.area != moved_area
        {
            return None;
        }
        let dx = painted.offset.x - self.offset.x;
        let dy = painted.offset.y - self.offset.y;
        let physical_x = dx * scale;
        let physical_y = dy * scale;
        if (dx == Fixed::ZERO && dy == Fixed::ZERO)
            || dx.abs() >= self.area.w
            || dy.abs() >= self.area.h
            || physical_x != Fixed::from_int(physical_x.trunc_to_int())
            || physical_y != Fixed::from_int(physical_y.trunc_to_int())
        {
            return None;
        }
        Some(RegionShift {
            area: painted.area,
            dx,
            dy,
        })
    }
}

pub(super) fn children_are_transferable(world: &World, entity: Entity) -> bool {
    crate::ui::offscreen::config(world, entity).is_none()
        && world
            .get::<Style>(entity)
            .is_none_or(|style| !style.clip_children || style.border_radius == Fixed::ZERO)
}

pub(crate) fn has_pending_scroll(world: &World) -> bool {
    world.storage::<ScrollOffset>().is_some_and(|offsets| {
        offsets.iter().any(|(entity, offset)| {
            world
                .get::<PaintedScroll>(entity)
                .is_some_and(|painted| painted.offset != *offset)
        })
    })
}
