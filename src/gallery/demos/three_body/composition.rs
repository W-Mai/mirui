use alloc::vec::Vec;

use super::orbit::OrbitRing;
use super::simulation::BODY_SIZE;
#[cfg(feature = "std")]
use super::simulation::{kick_system, physics_tick_system, sync_layout_system};
use super::state::{
    KickPhase, LayoutOrigin, PhysicsBody, PhysicsScratch, PhysicsTime, SpringLength, Velocity,
    WorldBounds,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16, n_bodies: usize, equilibrium: Fixed) {
    let logical_w = view_w as i32;
    let logical_h = view_h as i32;

    let now_ms = cx
        .world_mut()
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    cx.world_mut().insert_resource(PhysicsTime {
        last_tick_ms: now_ms,
        accumulator_ms: 0,
    });
    cx.world_mut().insert_resource(WorldBounds {
        w: logical_w,
        h: logical_h,
    });
    cx.world_mut().insert_resource(SpringLength(equilibrium));
    let n = n_bodies.max(1);
    cx.world_mut().insert_resource(PhysicsScratch {
        entities: Vec::with_capacity(n),
        positions: Vec::with_capacity(n),
        ax: Vec::with_capacity(n),
        ay: Vec::with_capacity(n),
    });
    cx.world_mut().insert_resource(KickPhase(0));

    let orbit_size = logical_w.min(logical_h) * 72 / 100;
    let orbit_left = (logical_w - orbit_size) / 2;
    let orbit_top = (logical_h - orbit_size) / 2;
    ui! {
        View (grow: 1.0, bg_color: ColorToken::Surface, clip_children: true) {
            Row (
                position: Position::Absolute,
                left: 0,
                top: 8,
                width: Dimension::percent(100),
                height: 32,
                align: AlignItems::Center,
                column_gap: 8,
                padding: Padding {
                    top: Dimension::px(0),
                    right: Dimension::px(16),
                    bottom: Dimension::px(0),
                    left: Dimension::px(16),
                }
            ) {
                Text (
                    "THREE-BODY FIELD",
                    grow: 1.0,
                    height: 26,
                    font_size: 15,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "FIXED · LIVE",
                    width: 104,
                    height: 22,
                    font_size: 9,
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::Primary,
                    border_radius: 11,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (
                position: Position::Absolute,
                left: orbit_left,
                top: orbit_top,
                width: orbit_size,
                height: orbit_size,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: orbit_size as u32 / 2
            ) [
                OrbitRing { diameter_percent: 72 },
            ]
            View (
                position: Position::Absolute,
                left: orbit_left + orbit_size / 4,
                top: orbit_top + orbit_size / 4,
                width: orbit_size / 2,
                height: orbit_size / 2,
                border_color: Color::rgba(97, 218, 251, 80),
                border_width: 1,
                border_radius: orbit_size as u32 / 4
            ) [
                OrbitRing { diameter_percent: 36 },
            ]
        }
    };

    let iw = BODY_SIZE;
    let ih = BODY_SIZE;
    let center_x = Fixed::from_int(logical_w / 2);
    let center_y = Fixed::from_int(logical_h / 2);
    let r = Fixed::from_int(logical_w.min(logical_h) * 35 / 100);
    let orbital = Fixed::from_int(2);

    let mut init_pos: Vec<(Fixed, Fixed, Fixed, Fixed)> = Vec::with_capacity(n);
    for i in 0..n {
        let deg = Fixed::from_int(360) * Fixed::from_int(i as i32) / Fixed::from_int(n as i32);
        let c = Fixed::cos_deg(deg);
        let s = Fixed::sin_deg(deg);
        init_pos.push((
            center_x + c * r,
            center_y + s * r,
            Fixed::ZERO - s * orbital,
            c * orbital,
        ));
    }

    //~focus-start
    ui! {
        walk init_pos.iter() with pos {
            Image (
                position: Position::Absolute,
                left: pos.0.to_int() - iw / 2,
                top: pos.1.to_int() - ih / 2,
                width: iw,
                height: ih,
                src: "thumbs_up",
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Primary,
                border_width: 1,
                border_radius: BODY_SIZE as u32 / 2
            ) [
                PhysicsBody { x: pos.0, y: pos.1 },
                Velocity { vx: pos.2, vy: pos.3 },
                LayoutOrigin {
                    x: pos.0 - Fixed::from_int(iw / 2),
                    y: pos.1 - Fixed::from_int(ih / 2),
                },
            ]
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
    let info = app.backend.display_info();
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.add_system(physics_tick_system::system());
    app.add_system(kick_system::system());
    app.add_system(sync_layout_system::system());
    app.compose(parent, |cx| {
        build_widgets(
            cx,
            info.width,
            info.height,
            3,
            Fixed::from_int((info.width.min(info.height) as i32 * 22 / 100).max(30)),
        )
    });
}
