extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Style};

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

pub struct ParticleArena;

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

#[derive(Clone, Copy)]
struct RingSeed {
    color: Color,
    grow_speed: Fixed,
    max_radius: Fixed,
    radius: Fixed,
}

#[derive(Clone, Copy)]
struct BarSeed {
    color: Color,
    speed: Fixed,
    start: Fixed,
    vertical: bool,
    width: i32,
    height: i32,
}

#[derive(Clone, Copy)]
struct ParticleSeed {
    color: Color,
    x: Fixed,
    y: Fixed,
    vx: Fixed,
    vy: Fixed,
}

#[compose]
pub fn build_widgets() {
    let bw = DEFAULT_VIEW.0 as i32;
    let bh = DEFAULT_VIEW.1 as i32;
    cx.world_mut()
        .insert_resource(ParticleBounds { w: bw, h: bh });

    let ring_seeds = [
        RingSeed {
            color: Color::rgba(80, 200, 255, 60),
            grow_speed: Fixed::from_ratio(3, 64),
            max_radius: Fixed::from_int(72),
            radius: Fixed::from_int(12),
        },
        RingSeed {
            color: Color::rgba(255, 100, 200, 40),
            grow_speed: Fixed::from_ratio(1, 32),
            max_radius: Fixed::from_int(92),
            radius: Fixed::from_int(36),
        },
        RingSeed {
            color: Color::rgba(100, 255, 150, 50),
            grow_speed: Fixed::from_ratio(15, 256),
            max_radius: Fixed::from_int(112),
            radius: Fixed::from_int(64),
        },
    ];
    let bar_seeds = [
        BarSeed {
            color: Color::rgba(255, 200, 50, 180),
            speed: Fixed::from_ratio(45, 256),
            start: Fixed::from_int(10),
            vertical: false,
            width: 30,
            height: 6,
        },
        BarSeed {
            color: Color::rgba(50, 255, 200, 160),
            speed: Fixed::from_ratio(33, 256),
            start: Fixed::from_int(80),
            vertical: false,
            width: 25,
            height: 5,
        },
        BarSeed {
            color: Color::rgba(200, 50, 255, 140),
            speed: Fixed::from_ratio(55, 256),
            start: Fixed::from_int(20),
            vertical: true,
            width: 5,
            height: 40,
        },
    ];
    let mut rng_state = 0x9E37_79B9_u32;
    let mut rng = || -> u32 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 17;
        rng_state ^= rng_state << 5;
        rng_state
    };

    let particle_colors = [
        Color::rgb(255, 80, 80),
        Color::rgb(80, 255, 80),
        Color::rgb(80, 80, 255),
        Color::rgb(255, 255, 80),
        Color::rgb(255, 80, 255),
        Color::rgb(80, 255, 255),
    ];
    let particle_seeds = particle_colors.map(|color| ParticleSeed {
        color,
        x: Fixed::from_int(12) + Fixed::from_ratio((rng() % ((bw - 24) as u32 * 256)) as i32, 256),
        y: Fixed::from_int(12) + Fixed::from_ratio((rng() % ((bh - 64) as u32 * 256)) as i32, 256),
        vx: Fixed::from_ratio((rng() % 201) as i32 - 100, 256),
        vy: Fixed::from_ratio((rng() % 201) as i32 - 100, 256),
    });

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(12),
            row_gap: 8,
            bg_color: ColorToken::Surface
        ) {
            Row (
                width: Dimension::percent(100),
                height: 28,
                align: AlignItems::Center
            ) {
                Text (
                    "KINETIC FIELD",
                    grow: 1.0,
                    font_size: 14,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "LIVE · 12 NODES",
                    width: 124,
                    height: 22,
                    font_size: 9,
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::Success,
                    border_color: ColorToken::Success,
                    border_width: 1,
                    border_radius: 11,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 14,
                clip_children: true
            ) [
                ParticleArena,
            ] {
                walk ring_seeds.iter() with seed {
                    View (
                        position: Position::Absolute,
                        left: bw / 2 - 10,
                        top: bh / 2 - 10,
                        width: 20,
                        height: 20,
                        bg_color: seed.color,
                        border_color: seed.color,
                        border_width: 2,
                        border_radius: 10
                    ) [
                        PulseRing {
                            radius: seed.radius,
                            grow_speed: seed.grow_speed,
                            max_radius: seed.max_radius,
                        },
                    ]
                }
                walk bar_seeds.iter() with seed {
                    View (
                        position: Position::Absolute,
                        left: 4,
                        top: 4,
                        width: seed.width,
                        height: seed.height,
                        bg_color: seed.color,
                        border_radius: 2
                    ) [
                        BouncingBar {
                            pos: seed.start,
                            speed: seed.speed,
                            vertical: seed.vertical,
                        },
                    ]
                }
                walk particle_seeds.iter() with seed {
                    View (
                        position: Position::Absolute,
                        left: seed.x,
                        top: seed.y,
                        width: 5,
                        height: 5,
                        bg_color: seed.color,
                        border_radius: 3
                    ) [
                        Particle {
                            x: seed.x,
                            y: seed.y,
                            vx: seed.vx,
                            vy: seed.vy,
                            phase: Fixed::ZERO,
                        },
                    ]
                }
            }
        }
    };
    //~focus-end
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
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Children, IdMap, UiScope};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
