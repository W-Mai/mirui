use crate::anim::{Motion, MotionComponent, Spring, run_motion};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Color, Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

pub struct Switch {
    pub on: bool,
    pub on_color: ThemedColor,
    pub off_color: ThemedColor,
    pub thumb_color: ThemedColor,
}

impl Default for Switch {
    fn default() -> Self {
        Self {
            on: false,
            on_color: ThemedColor::Token(ColorToken::Success),
            off_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            thumb_color: ThemedColor::Token(ColorToken::OnPrimary),
        }
    }
}

impl Switch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_on_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.on_color = color.into();
        self
    }

    pub fn with_off_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.off_color = color.into();
        self
    }

    pub fn with_thumb_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.thumb_color = color.into();
        self
    }

    pub fn toggle(&mut self) {
        self.on = !self.on;
    }
}

// Track-color lerp factor 0..1 (off..on). Updated by AnimateSwitchBgT,
// read by switch_render.
pub(crate) struct SwitchBgT(pub(crate) Fixed);

// Current thumb x (entity-local). Updated by AnimateThumbX, read by
// switch_render. Seeded by switch_init_system once layout has run.
pub(crate) struct AnimatedThumbX(pub(crate) Fixed);

pub(crate) struct AnimateSwitchBgT(pub(crate) Motion);

impl MotionComponent for AnimateSwitchBgT {
    fn motion(&self) -> &Motion {
        &self.0
    }
    fn motion_mut(&mut self) -> &mut Motion {
        &mut self.0
    }
}

#[crate::system(order = ANIMATION)]
pub(crate) fn animate_switch_bg_t_system(world: &mut World) {
    run_motion::<AnimateSwitchBgT>(world, |world, entity, value| {
        if let Some(t) = world.get_mut::<SwitchBgT>(entity) {
            t.0 = value;
        } else {
            world.insert(entity, SwitchBgT(value));
        }
        world.insert(entity, Dirty);
    });
}

pub(crate) struct AnimateThumbX(pub(crate) Motion);

impl MotionComponent for AnimateThumbX {
    fn motion(&self) -> &Motion {
        &self.0
    }
    fn motion_mut(&mut self) -> &mut Motion {
        &mut self.0
    }
}

#[crate::system(order = ANIMATION)]
pub(crate) fn animate_thumb_x_system(world: &mut World) {
    run_motion::<AnimateThumbX>(world, |world, entity, value| {
        if let Some(x) = world.get_mut::<AnimatedThumbX>(entity) {
            x.0 = value;
        } else {
            world.insert(entity, AnimatedThumbX(value));
        }
        world.insert(entity, Dirty);
    });
}

const SWITCH_THUMB_MARGIN: Fixed = Fixed::from_int(3);

fn off_thumb_x(rect: &Rect) -> Fixed {
    let _ = rect;
    SWITCH_THUMB_MARGIN
}

fn on_thumb_x(rect: &Rect) -> Fixed {
    let thumb_size = (rect.h - SWITCH_THUMB_MARGIN * Fixed::from_int(2)).max(Fixed::ZERO);
    rect.w - thumb_size - SWITCH_THUMB_MARGIN
}

// Seed AnimatedThumbX once ComputedRect is available (post-layout).
// Pre-layout we don't know rect yet, so attach can't compute it.
#[crate::system(order = ANIMATION)]
pub(crate) fn switch_init_system(world: &mut World) {
    let entities: alloc::vec::Vec<Entity> = world.query::<Switch>().collect();
    for e in entities {
        if world.get::<AnimatedThumbX>(e).is_some() {
            continue;
        }
        let Some(rect) = world.get::<ComputedRect>(e).map(|r| r.0) else {
            continue;
        };
        let on = world.get::<Switch>(e).map(|s| s.on).unwrap_or(false);
        let x = if on {
            on_thumb_x(&rect)
        } else {
            off_thumb_x(&rect)
        };
        world.insert(e, AnimatedThumbX(x));
    }
}

fn switch_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(s) = world.get::<Switch>(entity) else {
        return;
    };
    let theme = ctx.theme(world);
    let on_color = s.on_color.resolve_in(theme, ctx.state);
    let off_color = s.off_color.resolve_in(theme, ctx.state);
    let thumb_color = s.thumb_color.resolve_in(theme, ctx.state);
    let cap_radius = rect.h / Fixed::from_int(2);

    let t = world
        .get::<SwitchBgT>(entity)
        .map(|x| x.0)
        .unwrap_or_else(|| if s.on { Fixed::ONE } else { Fixed::ZERO });
    let track_color = Color::lerp(off_color, on_color, t);
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: track_color,
            radius: cap_radius,
            opa: 255,
        },
        ctx.clip,
    );

    let thumb_size = (rect.h - SWITCH_THUMB_MARGIN * Fixed::from_int(2)).max(Fixed::ZERO);
    // Fallback covers the first frame before switch_init_system seeds
    // AnimatedThumbX.
    let thumb_local_x = world
        .get::<AnimatedThumbX>(entity)
        .map(|x| x.0)
        .unwrap_or_else(|| off_thumb_x(rect));
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: rect.x + thumb_local_x,
                y: rect.y + SWITCH_THUMB_MARGIN,
                w: thumb_size,
                h: thumb_size,
            },
            transform: ctx.transform,
            quad: ctx.quad,
            color: thumb_color,
            radius: thumb_size / Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );
}

pub(crate) fn switch_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }

    let on_now = {
        let Some(s) = world.get_mut::<Switch>(entity) else {
            return false;
        };
        s.toggle();
        s.on
    };

    let target_t = if on_now { Fixed::ONE } else { Fixed::ZERO };
    let cur_t = world
        .get::<SwitchBgT>(entity)
        .map(|t| t.0)
        .unwrap_or_else(|| if on_now { Fixed::ZERO } else { Fixed::ONE });
    world.insert(
        entity,
        AnimateSwitchBgT(Spring::new(cur_t, target_t, 250, Fixed::ZERO).into()),
    );

    if let Some(rect) = world.get::<ComputedRect>(entity).map(|r| r.0) {
        let target_x = if on_now {
            on_thumb_x(&rect)
        } else {
            off_thumb_x(&rect)
        };
        let cur_x = world
            .get::<AnimatedThumbX>(entity)
            .map(|x| x.0)
            .unwrap_or_else(|| {
                if on_now {
                    off_thumb_x(&rect)
                } else {
                    on_thumb_x(&rect)
                }
            });
        world.insert(
            entity,
            AnimateThumbX(Spring::new(cur_x, target_x, 200, Fixed::ZERO).into()),
        );
    }

    world.insert(entity, Dirty);
    true
}

fn switch_attach(world: &mut World, entity: Entity) {
    if world.get::<Switch>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: switch_handler,
        },
    );
    let on = world.get::<Switch>(entity).map(|s| s.on).unwrap_or(false);
    let initial_t = if on { Fixed::ONE } else { Fixed::ZERO };
    world.insert(entity, SwitchBgT(initial_t));
    // AnimatedThumbX seeded later by switch_init_system once
    // ComputedRect exists.
}

pub fn view() -> View {
    View::new("Switch", 60, switch_render)
        .with_attach(switch_attach)
        .with_systems(
            const {
                &[
                    switch_init_system::system(),
                    animate_switch_bg_t_system::system(),
                    animate_thumb_x_system::system(),
                ]
            },
        )
}
