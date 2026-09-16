extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui;
use crate::ui::root_viewport;
use crate::ui::widgets::{Image, ParagraphStyle, Text, assets::*};
use alloc::vec::Vec;

pub struct Velocity {
    pub vx: Fixed,
    pub vy: Fixed,
}

pub struct PhysicsBody {
    pub x: Fixed,
    pub y: Fixed,
}

#[derive(Clone, Copy)]
struct LayoutOrigin {
    x: Fixed,
    y: Fixed,
}

pub struct PhysicsTime {
    pub last_tick_ms: u32,
    pub accumulator_ms: u32,
}

pub struct WorldBounds {
    pub w: i32,
    pub h: i32,
}

pub struct SpringLength(pub Fixed);

#[derive(Default)]
pub struct PhysicsScratch {
    pub entities: Vec<Entity>,
    pub positions: Vec<(Fixed, Fixed)>,
    pub ax: Vec<Fixed>,
    pub ay: Vec<Fixed>,
}

impl PhysicsScratch {
    fn with_entities(world: &mut World, f: impl FnOnce(&mut World, &[Entity])) {
        let Some(scratch) = world.resource_mut::<Self>() else {
            return;
        };
        let scratch = core::mem::take(scratch);
        f(world, &scratch.entities);
        if let Some(slot) = world.resource_mut::<Self>() {
            *slot = scratch;
        }
    }
}

pub struct KickPhase(pub u32);

const PHYSICS_DT_MS: u32 = 11;

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
        world.insert_resource(WorldBounds {
            w: rect.w.to_int(),
            h: rect.h.to_int(),
        });
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
    let min = Fixed::from_int(8);
    let max_x = Fixed::from_int(bound_w - 8);
    let max_y = Fixed::from_int(bound_h - 8);
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
    let half_w = Fixed::from_int(IMG_THUMBS_UP.width as i32 / 2);
    let half_h = Fixed::from_int(IMG_THUMBS_UP.height as i32 / 2);
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

    ui! {
        Column (grow: 1.0) {
            Text (
                "mirui",
                bg_color: Color::rgb(88, 166, 255),
                height: 20,
                border_radius: 3,
                paragraph: ParagraphStyle::label()
            )
            Row (grow: 1.0) {
                View (bg_color: Color::rgb(63, 185, 80), grow: 1.0)
                View (bg_color: Color::rgb(248, 81, 73), grow: 1.0)
            }
            Text (
                "n-body",
                bg_color: Color::rgb(210, 168, 255),
                height: 20,
                paragraph: ParagraphStyle::label()
            )
        }
    };

    let iw = IMG_THUMBS_UP.width as i32;
    let ih = IMG_THUMBS_UP.height as i32;
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
            View (
                position: Position::Absolute,
                left: pos.0.to_int() - iw / 2,
                top: pos.1.to_int() - ih / 2,
                width: iw,
                height: ih,
                image: Image::new("thumbs_up")
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
        build_widgets(cx, info.width, info.height, 3, Fixed::from_int(30))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, 128, 128, 3, Fixed::from_int(30));
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn animation_systems_reuse_body_entities() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, 128, 128, 3, Fixed::from_int(30));
        drop(cx);

        let mut entities = Vec::new();
        world
            .query::<PhysicsBody>()
            .and::<Velocity>()
            .collect_into(&mut entities);
        assert_eq!(entities.len(), 3);
        let selected = entities[1];
        let initial_velocity = world.get::<Velocity>(selected).unwrap().vx;
        let scratch = world.resource_mut::<PhysicsScratch>().unwrap();
        scratch.entities = entities;
        let entities_ptr = scratch.entities.as_ptr();
        world.resource_mut::<KickPhase>().unwrap().0 = 39;

        kick_system(&mut world);
        assert_ne!(
            world.get::<Velocity>(selected).unwrap().vx,
            initial_velocity
        );
        let body = world.get_mut::<PhysicsBody>(selected).unwrap();
        body.x = Fixed::from_int(40);
        body.y = Fixed::from_int(50);
        sync_layout_system(&mut world);

        let origin = *world.get::<LayoutOrigin>(selected).unwrap();
        let transform = world
            .get::<crate::ui::widgets::WidgetTransform>(selected)
            .unwrap()
            .0;
        assert_eq!(
            transform,
            Transform::translate(
                Fixed::from_int(40 - IMG_THUMBS_UP.width as i32 / 2) - origin.x,
                Fixed::from_int(50 - IMG_THUMBS_UP.height as i32 / 2) - origin.y,
            )
        );
        assert!(
            world
                .get::<crate::ui::dirty::VisualDirty>(selected)
                .is_some()
        );
        assert_eq!(
            world
                .resource::<PhysicsScratch>()
                .unwrap()
                .entities
                .as_ptr(),
            entities_ptr
        );
    }
}
