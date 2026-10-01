use super::state::{BouncingBar, Particle, ParticleArena, ParticleBounds, PulseRing};
use crate::prelude::*;
use crate::ui::{ComputedRect, Style};

#[mirui_macros::system(order = Animation)]
pub fn particle_bounds_system(arenas: query!(ParticleArena, ComputedRect), commands: commands!()) {
    if let Some((_, rect)) = arenas.into_iter().next() {
        commands.insert_resource(ParticleBounds {
            w: rect.0.w.to_int(),
            h: rect.0.h.to_int(),
        });
    }
}

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn particle_system(
    bounds: res!(optional ParticleBounds),
    particles: query!(mut Particle, mut Style),
) {
    let (bw, bh) = bounds.map(|b| (b.w, b.h)).unwrap_or((128, 128));
    for (particle, style) in particles {
        particle.x += particle.vx;
        particle.y += particle.vy;
        particle.phase += Fixed::from_ratio(5, 256);

        if particle.x < Fixed::from_int(2) || particle.x > Fixed::from_int(bw - 6) {
            particle.vx = Fixed::ZERO - particle.vx;
            particle.x = particle
                .x
                .max(Fixed::from_int(2))
                .min(Fixed::from_int(bw - 6));
        }
        if particle.y < Fixed::from_int(2) || particle.y > Fixed::from_int(bh - 6) {
            particle.vy = Fixed::ZERO - particle.vy;
            particle.y = particle
                .y
                .max(Fixed::from_int(2))
                .min(Fixed::from_int(bh - 6));
        }
        style.layout.left = Dimension::Px(particle.x);
        style.layout.top = Dimension::Px(particle.y);
    }
}
//~focus-end

#[mirui_macros::system(order = Animation)]
pub fn pulse_ring_system(
    bounds: res!(optional ParticleBounds),
    rings: query!(mut PulseRing, mut Style),
) {
    let (bw, bh) = bounds.map(|b| (b.w, b.h)).unwrap_or((128, 128));
    for (ring, style) in rings {
        ring.radius += ring.grow_speed;
        if ring.radius > ring.max_radius {
            ring.radius = Fixed::from_int(2);
        }
        let center_x = Fixed::from_int(bw / 2);
        let center_y = Fixed::from_int(bh / 2);
        style.layout.left = Dimension::Px(center_x - ring.radius);
        style.layout.top = Dimension::Px(center_y - ring.radius);
        style.layout.width = Dimension::Px(ring.radius * 2);
        style.layout.height = Dimension::Px(ring.radius * 2);
        style.border_radius = ring.radius;
    }
}

#[mirui_macros::system(order = Animation)]
pub fn bar_system(bounds: res!(optional ParticleBounds), bars: query!(mut BouncingBar, mut Style)) {
    let (bw, bh) = bounds.map(|b| (b.w, b.h)).unwrap_or((128, 128));
    for (bar, style) in bars {
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
        let (new_x, new_y) = if bar.vertical {
            (Fixed::from_int(4), bar.pos)
        } else {
            (bar.pos, Fixed::from_int(4))
        };
        style.layout.left = Dimension::Px(new_x);
        style.layout.top = Dimension::Px(new_y);
    }
}
