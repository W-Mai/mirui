use super::orbit::OrbitRing;
use super::state::{
    KickPhase, LayoutOrigin, PhysicsBody, PhysicsScratch, PhysicsTime, SpringLength, Velocity,
    WorldBounds,
};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::root_viewport;

const PHYSICS_DT_MS: u32 = 11;
pub(super) const BODY_SIZE: i32 = 24;

impl WorldBounds {
    pub(super) fn sync(world: &mut World, width: i32, height: i32) {
        let (old_width, old_height) = world
            .resource::<Self>()
            .map(|bounds| (bounds.w, bounds.h))
            .unwrap_or((width, height));
        if old_width == width && old_height == height {
            return;
        }
        let dx = Fixed::from_int((width - old_width) / 2);
        let dy = Fixed::from_int((height - old_height) / 2);
        world.for_each_stable::<PhysicsBody>(|world, entity| {
            if let Some(body) = world.get_mut::<PhysicsBody>(entity) {
                body.x += dx;
                body.y += dy;
            }
        });
        world.insert_resource(Self {
            w: width,
            h: height,
        });
        OrbitRing::sync_all(world, width, height);
    }
}

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
pub fn physics_tick_system(world: &mut World) {
    if let Some(rect) = root_viewport(world) {
        WorldBounds::sync(world, rect.w.to_int(), rect.h.to_int());
    }
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let steps = {
        let Some(pt) = world.resource_mut::<PhysicsTime>() else {
            return;
        };
        let elapsed = now_ms.wrapping_sub(pt.last_tick_ms);
        pt.last_tick_ms = now_ms;
        pt.accumulator_ms = pt.accumulator_ms.saturating_add(elapsed);
        let steps = pt.accumulator_ms / PHYSICS_DT_MS;
        pt.accumulator_ms %= PHYSICS_DT_MS;
        steps
    };
    for _ in 0..steps.min(8) {
        three_body_step(world);
    }
}

//~focus-start
fn three_body_step(world: &mut World) {
    let (bound_w, bound_h) = world
        .resource::<WorldBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((128, 128));
    let equilibrium = world
        .resource::<SpringLength>()
        .map(|s| s.0)
        .unwrap_or(Fixed::from_int(30));

    let mut scratch = {
        let Some(s) = world.resource_mut::<PhysicsScratch>() else {
            return;
        };
        core::mem::take(s)
    };
    scratch.entities.clear();
    world
        .query::<PhysicsBody>()
        .and::<Velocity>()
        .collect_into(&mut scratch.entities);
    let n = scratch.entities.len();
    if n == 0 {
        if let Some(s) = world.resource_mut::<PhysicsScratch>() {
            *s = scratch;
        }
        return;
    }

    scratch.positions.clear();
    scratch.positions.resize(n, (Fixed::ZERO, Fixed::ZERO));
    for i in 0..n {
        if let Some(body) = world.get::<PhysicsBody>(scratch.entities[i]) {
            scratch.positions[i] = (body.x, body.y);
        }
    }

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
            let dist = Fixed::from_int(isqrt((dx_int * dx_int + dy_int * dy_int) as u32) as i32);
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

    let v_max = Fixed::from_int(5);
    let v_min = Fixed::ZERO - v_max;
    let margin = BODY_SIZE / 2;
    let min = Fixed::from_int(margin);
    let max_x = Fixed::from_int(bound_w - margin);
    let max_y = Fixed::from_int(bound_h - margin);
    for i in 0..n {
        let e = scratch.entities[i];
        if let Some(vel) = world.get_mut::<Velocity>(e) {
            vel.vx += scratch.ax[i];
            vel.vy += scratch.ay[i];
            if vel.vx > v_max {
                vel.vx = v_max;
            }
            if vel.vx < v_min {
                vel.vx = v_min;
            }
            if vel.vy > v_max {
                vel.vy = v_max;
            }
            if vel.vy < v_min {
                vel.vy = v_min;
            }
        }
        let (vx, vy) = world
            .get::<Velocity>(e)
            .map(|v| (v.vx, v.vy))
            .unwrap_or((Fixed::ZERO, Fixed::ZERO));
        if let Some(body) = world.get_mut::<PhysicsBody>(e) {
            body.x += vx;
            body.y += vy;
            if body.x < min {
                body.x = min;
            }
            if body.x > max_x {
                body.x = max_x;
            }
            if body.y < min {
                body.y = min;
            }
            if body.y > max_y {
                body.y = max_y;
            }
        }
        if let Some(body) = world.get::<PhysicsBody>(e) {
            let bx = body.x;
            let by = body.y;
            if let Some(vel) = world.get_mut::<Velocity>(e)
                && (bx <= min || bx >= max_x)
            {
                vel.vx = Fixed::ZERO - vel.vx;
            }
            if let Some(vel) = world.get_mut::<Velocity>(e)
                && (by <= min || by >= max_y)
            {
                vel.vy = Fixed::ZERO - vel.vy;
            }
        }
    }

    if let Some(s) = world.resource_mut::<PhysicsScratch>() {
        *s = scratch;
    }
}
//~focus-end

#[mirui_macros::system]
pub fn kick_system(world: &mut World) {
    let phase = {
        let Some(p) = world.resource_mut::<KickPhase>() else {
            return;
        };
        p.0 = p.0.wrapping_add(1);
        p.0
    };
    if phase % 40 == 0 {
        PhysicsScratch::with_entities(world, |world, entities| {
            if !entities.is_empty() {
                let kick_idx = (phase / 40) as usize % entities.len();
                let kick_dir = (phase / 120) as i32;
                let e = entities[kick_idx];
                let kx = (kick_dir * 7).rem_euclid(13) - 6;
                let ky = (kick_dir * 11).rem_euclid(13) - 6;
                if let Some(vel) = world.get_mut::<Velocity>(e) {
                    vel.vx += Fixed::from_int(kx) / Fixed::from_int(2);
                    vel.vy += Fixed::from_int(ky) / Fixed::from_int(2);
                }
            }
        });
    }
}

#[mirui_macros::system]
pub fn sync_layout_system(world: &mut World) {
    let half_w = Fixed::from_int(BODY_SIZE / 2);
    let half_h = Fixed::from_int(BODY_SIZE / 2);
    PhysicsScratch::with_entities(world, |world, entities| {
        for &e in entities {
            if let (Some(body), Some(origin)) =
                (world.get::<PhysicsBody>(e), world.get::<LayoutOrigin>(e))
            {
                let tx = body.x - half_w - origin.x;
                let ty = body.y - half_h - origin.y;
                crate::ui::widgets::set_transform(world, e, Transform::translate(tx, ty));
            }
        }
    });
}
