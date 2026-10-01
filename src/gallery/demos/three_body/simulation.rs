use super::orbit::OrbitRing;
use super::state::{
    KickPhase, LayoutOrigin, PhysicsBody, PhysicsScratch, PhysicsTime, SpringLength, Velocity,
    WorldBounds,
};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::dirty::VisualDirty;
use crate::ui::widgets::WidgetTransform;
use crate::ui::{ComputedRect, WidgetRoot};

const PHYSICS_DT_MS: u32 = 11;
pub(super) const BODY_SIZE: i32 = 24;

fn isqrt(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[mirui_macros::system]
pub fn physics_tick_system(
    clock: res!(optional MonoClock),
    time: res!(mut PhysicsTime),
    bounds: res!(mut WorldBounds),
    spring: res!(optional SpringLength),
    scratch: res!(mut PhysicsScratch),
    root: res!(optional WidgetRoot),
    rects: query!(Entity, ComputedRect),
    bodies: query!(Entity, PhysicsBody, Velocity),
    commands: commands!(),
) {
    let new_bounds = root
        .and_then(|root| {
            rects
                .into_iter()
                .find_map(|(entity, rect)| (entity == root.0).then_some(rect.0))
        })
        .map(|rect| (rect.w.to_int(), rect.h.to_int()));
    let (dx, dy) = match new_bounds {
        Some((width, height)) if width != bounds.w || height != bounds.h => {
            let dx = Fixed::from_int((width - bounds.w) / 2);
            let dy = Fixed::from_int((height - bounds.h) / 2);
            bounds.w = width;
            bounds.h = height;
            (dx, dy)
        }
        _ => (Fixed::ZERO, Fixed::ZERO),
    };

    let now_ms = clock.map(MonoClock::now_ms).unwrap_or(0);
    let elapsed = now_ms.wrapping_sub(time.last_tick_ms);
    time.last_tick_ms = now_ms;
    time.accumulator_ms = time.accumulator_ms.saturating_add(elapsed);
    let steps = (time.accumulator_ms / PHYSICS_DT_MS).min(8);
    time.accumulator_ms %= PHYSICS_DT_MS;

    scratch.entities.clear();
    scratch.positions.clear();
    scratch.velocities.clear();
    for (entity, body, velocity) in bodies {
        scratch.entities.push(entity);
        scratch.positions.push((body.x + dx, body.y + dy));
        scratch.velocities.push((velocity.vx, velocity.vy));
    }
    let n = scratch.entities.len();
    if n == 0 {
        return;
    }

    let equilibrium = spring.map_or(Fixed::from_int(30), |spring| spring.0);
    let (bound_w, bound_h) = (bounds.w, bounds.h);
    let v_max = Fixed::from_int(5);
    let v_min = Fixed::ZERO - v_max;
    let margin = BODY_SIZE / 2;
    let min = Fixed::from_int(margin);
    let max_x = Fixed::from_int(bound_w - margin);
    let max_y = Fixed::from_int(bound_h - margin);

    for _ in 0..steps {
        scratch.ax.clear();
        scratch.ax.resize(n, Fixed::ZERO);
        scratch.ay.clear();
        scratch.ay.resize(n, Fixed::ZERO);
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = scratch.positions[j].0 - scratch.positions[i].0;
                let dy = scratch.positions[j].1 - scratch.positions[i].1;
                let dx_int = dx.to_int();
                let dy_int = dy.to_int();
                let dist =
                    Fixed::from_int(isqrt((dx_int * dx_int + dy_int * dy_int) as u32) as i32);
                if dist == Fixed::ZERO {
                    continue;
                }
                let force = Fixed::from_int(120) * (dist - equilibrium) / dist;
                let fx = force * dx / (dist * dist);
                let fy = force * dy / (dist * dist);
                scratch.ax[i] += fx;
                scratch.ay[i] += fy;
                scratch.ax[j] -= fx;
                scratch.ay[j] -= fy;
            }
        }

        for i in 0..n {
            let (mut vx, mut vy) = scratch.velocities[i];
            vx += scratch.ax[i];
            vy += scratch.ay[i];
            if vx > v_max {
                vx = v_max;
            }
            if vx < v_min {
                vx = v_min;
            }
            if vy > v_max {
                vy = v_max;
            }
            if vy < v_min {
                vy = v_min;
            }

            let (mut x, mut y) = scratch.positions[i];
            x += vx;
            y += vy;
            if x < min {
                x = min;
            }
            if x > max_x {
                x = max_x;
            }
            if y < min {
                y = min;
            }
            if y > max_y {
                y = max_y;
            }
            if x <= min || x >= max_x {
                vx = Fixed::ZERO - vx;
            }
            if y <= min || y >= max_y {
                vy = Fixed::ZERO - vy;
            }
            scratch.positions[i] = (x, y);
            scratch.velocities[i] = (vx, vy);
        }
    }

    if steps == 0 && dx == Fixed::ZERO && dy == Fixed::ZERO {
        return;
    }
    for i in 0..n {
        let entity = scratch.entities[i];
        let (x, y) = scratch.positions[i];
        let (vx, vy) = scratch.velocities[i];
        commands.insert(entity, PhysicsBody { x, y });
        commands.insert(entity, Velocity { vx, vy });
    }
}
//~focus-end

#[mirui_macros::system]
pub fn kick_system(
    phase: res!(mut KickPhase),
    scratch: res!(PhysicsScratch),
    velocities: query!(Entity, mut Velocity),
) {
    phase.0 = phase.0.wrapping_add(1);
    let phase = phase.0;
    if phase % 40 == 0 {
        if scratch.entities.is_empty() {
            return;
        }
        let kick_idx = (phase / 40) as usize % scratch.entities.len();
        let kick_dir = (phase / 120) as i32;
        let target = scratch.entities[kick_idx];
        let kx = (kick_dir * 7).rem_euclid(13) - 6;
        let ky = (kick_dir * 11).rem_euclid(13) - 6;
        for (entity, velocity) in velocities {
            if entity == target {
                velocity.vx += Fixed::from_int(kx) / Fixed::from_int(2);
                velocity.vy += Fixed::from_int(ky) / Fixed::from_int(2);
                break;
            }
        }
    }
}

#[mirui_macros::system]
pub fn sync_layout_system(
    bounds: res!(WorldBounds),
    rings: query!(Entity, OrbitRing, Style),
    bodies: query!(Entity, PhysicsBody, LayoutOrigin, optional WidgetTransform),
    commands: commands!(),
) {
    for (entity, ring, style) in rings {
        let size = bounds.w.min(bounds.h) * ring.diameter_percent / 100;
        let left = Dimension::px((bounds.w - size) / 2);
        let top = Dimension::px((bounds.h - size) / 2);
        let extent = Dimension::px(size);
        let radius = Fixed::from_int(size / 2);
        if style.layout.left != left
            || style.layout.top != top
            || style.layout.width != extent
            || style.layout.height != extent
            || style.border_radius != radius
        {
            let mut next = style.clone();
            next.layout.left = left;
            next.layout.top = top;
            next.layout.width = extent;
            next.layout.height = extent;
            next.border_radius = radius;
            commands.insert(entity, next);
        }
    }

    let half_w = Fixed::from_int(BODY_SIZE / 2);
    let half_h = Fixed::from_int(BODY_SIZE / 2);
    for (entity, body, origin, current) in bodies {
        let transform =
            Transform::translate(body.x - half_w - origin.x, body.y - half_h - origin.y);
        if current.is_none_or(|current| current.0 != transform) {
            commands.insert(entity, WidgetTransform(transform));
            commands.insert(entity, VisualDirty);
        }
    }
}
