use super::state::{BouncingBar, Particle, ParticleArena, ParticleBounds, PulseRing};
use crate::prelude::*;
use crate::ui;
use crate::ui::{ComputedRect, Style};

#[mirui_macros::system(order = ANIMATION)]
pub fn particle_bounds_system(world: &mut World) {
    let arena = world
        .query::<ParticleArena>()
        .iter()
        .next()
        .map(|(entity, _)| entity);
    if let Some(rect) =
        arena.and_then(|entity| world.get::<ComputedRect>(entity).map(|rect| rect.0))
    {
        world.insert_resource(ParticleBounds {
            w: rect.w.to_int(),
            h: rect.h.to_int(),
        });
    }
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn particle_system(world: &mut World) {
    let (bw, bh) = world
        .resource::<ParticleBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    world.for_each_stable::<Particle>(|world, e| {
        let (new_x, new_y) = {
            let Some(p) = world.get_mut::<Particle>(e) else {
                return;
            };
            p.x += p.vx;
            p.y += p.vy;
            p.phase += Fixed::from_ratio(5, 256);

            if p.x < Fixed::from_int(2) || p.x > Fixed::from_int(bw - 6) {
                p.vx = Fixed::ZERO - p.vx;
                p.x = p.x.max(Fixed::from_int(2)).min(Fixed::from_int(bw - 6));
            }
            if p.y < Fixed::from_int(2) || p.y > Fixed::from_int(bh - 6) {
                p.vy = Fixed::ZERO - p.vy;
                p.y = p.y.max(Fixed::from_int(2)).min(Fixed::from_int(bh - 6));
            }
            (p.x, p.y)
        };
        ui::set_position(world, e, new_x, new_y);
    });
}
//~focus-end

#[mirui_macros::system(order = ANIMATION)]
pub fn pulse_ring_system(world: &mut World) {
    let (bw, bh) = world
        .resource::<ParticleBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    world.for_each_stable::<PulseRing>(|world, e| {
        let new_radius = {
            let Some(ring) = world.get_mut::<PulseRing>(e) else {
                return;
            };
            ring.radius += ring.grow_speed;
            if ring.radius > ring.max_radius {
                ring.radius = Fixed::from_int(2);
            }
            ring.radius
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            let center_x = Fixed::from_int(bw / 2);
            let center_y = Fixed::from_int(bh / 2);
            style.layout.left = Dimension::Px(center_x - new_radius);
            style.layout.top = Dimension::Px(center_y - new_radius);
            style.layout.width = Dimension::Px(new_radius * 2);
            style.layout.height = Dimension::Px(new_radius * 2);
            style.border_radius = new_radius;
        }
        world.invalidate(e);
    });
}

#[mirui_macros::system(order = ANIMATION)]
pub fn bar_system(world: &mut World) {
    let (bw, bh) = world
        .resource::<ParticleBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    world.for_each_stable::<BouncingBar>(|world, e| {
        let (new_x, new_y) = {
            let Some(bar) = world.get_mut::<BouncingBar>(e) else {
                return;
            };
            bar.pos += bar.speed;
            let max = if bar.vertical {
                Fixed::from_int(bh - 20)
            } else {
                Fixed::from_int(bw - 30)
            };
            if bar.pos < Fixed::from_int(4) || bar.pos > max {
                bar.speed = Fixed::ZERO - bar.speed;
                bar.pos = bar.pos.max(Fixed::from_int(4)).min(max);
            }
            if bar.vertical {
                (Fixed::from_int(4), bar.pos)
            } else {
                (bar.pos, Fixed::from_int(4))
            }
        };
        ui::set_position(world, e, new_x, new_y);
    });
}
