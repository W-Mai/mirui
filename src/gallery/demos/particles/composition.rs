use super::DEFAULT_VIEW;
use super::state::{BouncingBar, Particle, ParticleArena, ParticleBounds, PulseRing};
use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

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
