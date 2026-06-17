extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, MonoClock, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui;
use crate::ui::dirty::Dirty;
use crate::ui::root_viewport;
use crate::ui::{Children, Parent, Style};
use alloc::vec::Vec;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct Particle {
    pub x: Fixed,
    pub y: Fixed,
    pub vx: Fixed,
    pub vy: Fixed,
    pub phase: Fixed,
}

pub struct PulseRing {
    pub radius: Fixed,
    pub grow_speed: Fixed,
    pub max_radius: Fixed,
}

pub struct BouncingBar {
    pub pos: Fixed,
    pub speed: Fixed,
    pub vertical: bool,
}

pub struct ParticleBounds {
    pub w: i32,
    pub h: i32,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn particle_bounds_system(world: &mut World) {
    if let Some(rect) = root_viewport(world) {
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
    let mut buf = Vec::new();
    world.query::<Particle>().collect_into(&mut buf);
    for e in buf {
        let (new_x, new_y) = {
            let Some(p) = world.get_mut::<Particle>(e) else {
                continue;
            };
            p.x += p.vx;
            p.y += p.vy;
            p.phase += Fixed::from_raw(5);

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
    }
}
//~focus-end

#[mirui_macros::system(order = ANIMATION)]
pub fn pulse_ring_system(world: &mut World) {
    let (bw, bh) = world
        .resource::<ParticleBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    let mut buf = Vec::new();
    world.query::<PulseRing>().collect_into(&mut buf);
    for e in buf {
        let new_radius = {
            let Some(ring) = world.get_mut::<PulseRing>(e) else {
                continue;
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
            style.border_radius = Fixed::ZERO;
        }
        world.insert(e, Dirty);
    }
}

#[mirui_macros::system(order = ANIMATION)]
pub fn bar_system(world: &mut World) {
    let (bw, bh) = world
        .resource::<ParticleBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    let mut buf = Vec::new();
    world.query::<BouncingBar>().collect_into(&mut buf);
    for e in buf {
        let (new_x, new_y) = {
            let Some(bar) = world.get_mut::<BouncingBar>(e) else {
                continue;
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
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let bw = DEFAULT_VIEW.0 as i32;
    let bh = DEFAULT_VIEW.1 as i32;
    world.insert_resource(ParticleBounds { w: bw, h: bh });

    let ring_colors = [
        Color::rgba(80, 200, 255, 60),
        Color::rgba(255, 100, 200, 40),
        Color::rgba(100, 255, 150, 50),
    ];
    let ring_speeds = [Fixed::from_raw(12), Fixed::from_raw(8), Fixed::from_raw(15)];
    let ring_max = [
        Fixed::from_int(20),
        Fixed::from_int(16),
        Fixed::from_int(22),
    ];

    for i in 0..3 {
        let ring = WidgetBuilder::new(world)
            .bg_color(ring_colors[i])
            .border(ring_colors[i], Fixed::from_int(2))
            .border_radius(Fixed::from_int(10))
            .layout(LayoutStyle {
                position: Position::Absolute,
                left: Dimension::px(bw / 2 - 10),
                top: Dimension::px(bh / 2 - 10),
                width: Dimension::px(20),
                height: Dimension::px(20),
                ..Default::default()
            })
            .id();
        world.insert(
            ring,
            PulseRing {
                radius: Fixed::from_int(5 + i as i32 * 8),
                grow_speed: ring_speeds[i],
                max_radius: ring_max[i],
            },
        );
        world.insert(ring, Parent(parent));
        if let Some(ch) = world.get_mut::<Children>(parent) {
            ch.0.push(ring);
        }
    }

    let bar_configs: [(Color, Fixed, Fixed, bool, i32, i32); 3] = [
        (
            Color::rgba(255, 200, 50, 180),
            Fixed::from_raw(45),
            Fixed::from_int(10),
            false,
            30,
            6,
        ),
        (
            Color::rgba(50, 255, 200, 160),
            Fixed::from_raw(33),
            Fixed::from_int(80),
            false,
            25,
            5,
        ),
        (
            Color::rgba(200, 50, 255, 140),
            Fixed::from_raw(55),
            Fixed::from_int(20),
            true,
            5,
            40,
        ),
    ];

    for (color, speed, start, vertical, ww, hh) in bar_configs {
        let bar = WidgetBuilder::new(world)
            .bg_color(color)
            .border_radius(Fixed::ZERO)
            .layout(LayoutStyle {
                position: Position::Absolute,
                left: Dimension::px(4),
                top: Dimension::px(4),
                width: Dimension::px(ww),
                height: Dimension::px(hh),
                ..Default::default()
            })
            .id();
        world.insert(
            bar,
            BouncingBar {
                pos: start,
                speed,
                vertical,
            },
        );
        world.insert(bar, Parent(parent));
        if let Some(ch) = world.get_mut::<Children>(parent) {
            ch.0.push(bar);
        }
    }

    let mut rng_state: u32 = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0)
        .wrapping_add(0x9E37_79B9);
    if rng_state == 0 {
        rng_state = 1;
    }
    let mut rng = || -> i32 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 17;
        rng_state ^= rng_state << 5;
        (rng_state % 256) as i32
    };

    let particle_colors = [
        Color::rgb(255, 80, 80),
        Color::rgb(80, 255, 80),
        Color::rgb(80, 80, 255),
        Color::rgb(255, 255, 80),
        Color::rgb(255, 80, 255),
        Color::rgb(80, 255, 255),
    ];

    for color in particle_colors {
        let px = Fixed::from_raw(rng() % (100 * 256));
        let py = Fixed::from_raw(rng() % (100 * 256));
        let vx = Fixed::from_raw(rng() % 200 - 100);
        let vy = Fixed::from_raw(rng() % 200 - 100);

        let particle = WidgetBuilder::new(world)
            .bg_color(color)
            .border_radius(Fixed::ZERO)
            .layout(LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(px),
                top: Dimension::Px(py),
                width: Dimension::px(4),
                height: Dimension::px(4),
                ..Default::default()
            })
            .id();
        world.insert(
            particle,
            Particle {
                x: px,
                y: py,
                vx,
                vy,
                phase: Fixed::ZERO,
            },
        );
        world.insert(particle, Parent(parent));
        if let Some(ch) = world.get_mut::<Children>(parent) {
            ch.0.push(particle);
        }
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.add_system(particle_bounds_system::system());
    app.add_system(particle_system::system());
    app.add_system(pulse_ring_system::system());
    app.add_system(bar_system::system());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
