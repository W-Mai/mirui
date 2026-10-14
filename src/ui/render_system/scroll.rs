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
    pub(super) reusable: bool,
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

    #[crate::compose]
    fn scroll_scene(radius: u16) {
        use crate::prelude::*;
        ui! {
            View (width: 90, height: 60, border_radius: radius, clip_children: true) {
                Column (id: "scroll", width: 80, height: 50, clip_children: true) [
                    ScrollOffset::default(),
                ] {
                    View (id: "cached", height: 30, bg_color: Color::rgb(200, 40, 30)) [
                        crate::ui::OffscreenRender::default(),
                    ]
                    View (height: 30, bg_color: Color::rgb(30, 180, 50))
                    View (height: 30, bg_color: Color::rgb(40, 60, 200))
                }
            }
        };
    }

    #[test]
    fn nontransferable_scroll_repaints_descendants_from_the_painted_offset() {
        use crate::surface::FramebufferAccess;
        for radius in [0, 8] {
            let mut app = crate::app::App::headless(100, 80);
            app.with_default_widgets().with_default_systems();
            app.with_offscreen_pool_budget(32 * 1024);
            let root = app.spawn_root().id();
            app.compose(root, scroll_scene(radius));
            app.render().unwrap();
            let list = app.world.find_by_id("scroll").unwrap();
            let cached = app.world.find_by_id("cached").unwrap();
            let generation = app
                .world
                .get::<crate::ui::OffscreenGeneration>(cached)
                .unwrap()
                .0;
            for offset in [Fixed::from_ratio(7, 2), 75.into(), (-6).into(), 0.into()] {
                let painted = app.world.get::<PaintedScroll>(list).unwrap().offset;
                for target in [offset + Fixed::ONE, offset] {
                    app.world.get_mut::<ScrollOffset>(list).unwrap().y = target;
                    app.world.invalidate_visual(list);
                    for _ in 0..2 {
                        assert!(!app.dirty_regions().unwrap().is_empty());
                        assert_eq!(
                            app.world.get::<PaintedScroll>(list).unwrap().offset,
                            painted
                        );
                    }
                }
                app.render_dirty().unwrap();
                let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
                app.render().unwrap();
                assert!(
                    dirty == app.backend.framebuffer().buf.as_slice(),
                    "radius={radius}, offset={offset:?}"
                );
                assert_eq!(
                    app.world
                        .get::<crate::ui::OffscreenGeneration>(cached)
                        .unwrap()
                        .0,
                    generation
                );
            }
            app.world.get_mut::<ScrollOffset>(list).unwrap().y = 10.into();
            app.world.invalidate_visual(list);
            app.render_dirty().unwrap();
            let cycle = |app: &mut crate::app::App<_>| {
                for offset in [Fixed::from_ratio(7, 2), 10.into()] {
                    app.world.get_mut::<ScrollOffset>(list).unwrap().y = offset;
                    app.world.invalidate_visual(list);
                    app.dirty_regions().unwrap();
                    app.render_dirty().unwrap();
                }
            };
            cycle(&mut app);
            assert_eq!(
                crate::test_support::tracked_allocations(|| cycle(&mut app)),
                0
            );
            app.world.remove::<ScrollOffset>(list);
            app.world.invalidate_visual(list);
            app.render_dirty().unwrap();
            let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
            app.render().unwrap();
            assert!(dirty == app.backend.framebuffer().buf.as_slice());
            assert!(!app.world.has::<PaintedScroll>(list));
        }
    }

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
