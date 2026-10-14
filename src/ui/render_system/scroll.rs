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

/// Project painted fragments through framebuffer transfers in execution order.
pub(super) fn map_painted_damage(
    rect: Rect,
    shifts: &[RegionShift],
) -> crate::ui::dirty::RegionSet<{ super::DIRTY_REGION_CAPACITY }> {
    use crate::ui::dirty::{PaintCoverage, RegionSet};
    let mut current = RegionSet::new();
    let mut next = RegionSet::new();
    current.insert(rect);
    for shift in shifts {
        next.clear();
        for &rect in current.as_slice() {
            let Some(inside) = rect.intersect(&shift.area) else {
                next.insert(rect);
                continue;
            };
            for outside in PaintCoverage::frame(rect, inside).regions() {
                next.insert(outside);
            }
            if let Some(moved) = (Rect {
                x: inside.x + shift.dx,
                y: inside.y + shift.dy,
                ..inside
            })
            .intersect(&shift.area)
            {
                next.insert(moved);
            }
        }
        core::mem::swap(&mut current, &mut next);
    }
    current
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

#[cfg(test)]
mod damage_tests {
    use super::*;

    #[test]
    fn ordered_transfers_keep_disconnected_survivors_and_reuse_stack_storage() {
        let shifts = [
            RegionShift {
                area: Rect::new(5, 0, 10, 10),
                dx: 0.into(),
                dy: 3.into(),
            },
            RegionShift {
                area: Rect::new(0, 0, 24, 24),
                dx: 2.into(),
                dy: 1.into(),
            },
        ];
        let run = || {
            let mapped = map_painted_damage(Rect::new(0, 0, 20, 10), &shifts);
            assert_eq!(mapped.as_slice().len(), 3);
            for y in 0..14 {
                for x in 0..24 {
                    let covered = mapped
                        .as_slice()
                        .iter()
                        .any(|rect| rect.intersect(&Rect::new(x, y, 1, 1)).is_some());
                    let expected = (2..22).contains(&x)
                        && (1..11).contains(&y)
                        && (!(7..17).contains(&x) || y >= 4);
                    assert_eq!(covered, expected, "({x}, {y})");
                }
            }
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..16 {
                    run();
                }
            }),
            0
        );
    }

    #[test]
    fn mapped_damage_covers_surviving_pixels_through_ordered_transfers() {
        let shifts = [
            RegionShift {
                area: Rect::new(4, 4, 12, 12),
                dx: (-3).into(),
                dy: 2.into(),
            },
            RegionShift {
                area: Rect::new(0, 0, 24, 24),
                dx: 2.into(),
                dy: (-4).into(),
            },
        ];
        for y in -2..24 {
            for x in -2..24 {
                let rect = Rect::new(x, y, 5, 7);
                let mapped = map_painted_damage(rect, &shifts);
                for py in y..y + 7 {
                    for px in x..x + 5 {
                        let mut pixel = Some(Rect::new(px, py, 1, 1));
                        for shift in &shifts {
                            pixel = pixel.and_then(|pixel| {
                                if pixel.intersect(&shift.area).is_none() {
                                    return Some(pixel);
                                }
                                Rect {
                                    x: pixel.x + shift.dx,
                                    y: pixel.y + shift.dy,
                                    ..pixel
                                }
                                .intersect(&shift.area)
                            });
                        }
                        if let Some(pixel) = pixel {
                            assert!(
                                mapped
                                    .as_slice()
                                    .iter()
                                    .any(|mapped| mapped.intersect(&pixel) == Some(pixel))
                            );
                        }
                    }
                }
            }
        }
    }
}
