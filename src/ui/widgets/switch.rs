use crate::anim::{Motion, MotionComponent, Spring, run_motion};
use crate::ecs::{Entity, World};
use crate::input::event::BusinessCallback;
use crate::input::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Color, Fixed, Rect};
use crate::ui::ComputedRect;
use crate::ui::dirty::Dirty;
use crate::ui::property::{PropertyChange, ValueUpdate, invalidate_for_change};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};

#[derive(Clone, Debug)]
pub enum SwitchEvent {
    Toggled { now: bool },
}

pub struct SwitchHandler {
    pub on_event: BusinessCallback<SwitchEvent>,
}

#[crate::component]
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

    pub fn build() -> SwitchBuilder {
        SwitchBuilder {
            switch: Switch::new(),
            style: None,
            handler: None,
        }
    }
}

pub struct SwitchBuilder {
    switch: Switch,
    style: Option<crate::ui::Style>,
    handler: Option<SwitchHandler>,
}

impl SwitchBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn on_change(mut self, on_event: fn(&mut World, Entity, &SwitchEvent) -> bool) -> Self {
        self.handler = Some(SwitchHandler {
            on_event: BusinessCallback::Fn(on_event),
        });
        self
    }

    pub fn on_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.switch.on_color = color.into();
        self
    }

    pub fn off_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.switch.off_color = color.into();
        self
    }

    pub fn thumb_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.switch.thumb_color = color.into();
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for SwitchBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.switch);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(handler) = self.handler {
            world.insert(entity, handler);
        }
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

#[crate::system(order = ANIMATION, expect = AnimateSwitchBgT)]
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

#[crate::system(order = ANIMATION, expect = AnimateThumbX)]
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
#[crate::system(order = ANIMATION, expect = Switch)]
pub(crate) fn switch_init_system(world: &mut World) {
    world.for_each_stable::<Switch>(|world, e| {
        if world.get::<AnimatedThumbX>(e).is_some() {
            return;
        }
        let Some(rect) = world.get::<ComputedRect>(e).map(|r| r.0) else {
            return;
        };
        let on = world.get::<Switch>(e).map(|s| s.on).unwrap_or(false);
        let x = if on {
            on_thumb_x(&rect)
        } else {
            off_thumb_x(&rect)
        };
        world.insert(e, AnimatedThumbX(x));
    });
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
    ctx.draw(
        renderer,
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
    // Mid-animation the spring owns the position; at rest derive it from the
    // live rect so a resize moves the ON knob to the new right edge instead
    // of leaving it at the baked `AnimatedThumbX`.
    let thumb_local_x = if world.get::<AnimateThumbX>(entity).is_some() {
        world
            .get::<AnimatedThumbX>(entity)
            .map(|x| x.0)
            .unwrap_or_else(|| off_thumb_x(rect))
    } else if s.on {
        on_thumb_x(rect)
    } else {
        off_thumb_x(rect)
    };
    ctx.draw(
        renderer,
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

    let Some(on_now) = world.get::<Switch>(entity).map(|s| !s.on) else {
        return false;
    };
    let Some(update) = set_switch_on(world, entity, on_now) else {
        return false;
    };
    if update.old != update.new {
        invalidate_for_change(world, entity, update.change);
        emit_switch_event(world, entity, &SwitchEvent::Toggled { now: update.new });
    }
    true
}

pub(crate) fn set_switch_on(
    world: &mut World,
    entity: Entity,
    on_now: bool,
) -> Option<ValueUpdate<bool>> {
    let old_on = {
        let s = world.get_mut::<Switch>(entity)?;
        if s.on == on_now {
            return Some(ValueUpdate {
                old: on_now,
                new: on_now,
                change: PropertyChange::Unchanged,
            });
        }
        let old = s.on;
        s.on = on_now;
        old
    };

    let target_t = if on_now { Fixed::ONE } else { Fixed::ZERO };
    let cur_t = world
        .get::<SwitchBgT>(entity)
        .map(|t| t.0)
        .unwrap_or_else(|| if old_on { Fixed::ONE } else { Fixed::ZERO });

    if let Some(rect) = world.get::<ComputedRect>(entity).map(|r| r.0) {
        world.insert(
            entity,
            AnimateSwitchBgT(Spring::new(cur_t, target_t, 250, Fixed::ZERO).into()),
        );
        let target_x = if on_now {
            on_thumb_x(&rect)
        } else {
            off_thumb_x(&rect)
        };
        let cur_x = world
            .get::<AnimatedThumbX>(entity)
            .map(|x| x.0)
            .unwrap_or_else(|| {
                if old_on {
                    on_thumb_x(&rect)
                } else {
                    off_thumb_x(&rect)
                }
            });
        world.insert(
            entity,
            AnimateThumbX(Spring::new(cur_x, target_x, 200, Fixed::ZERO).into()),
        );
    } else {
        world.insert(entity, SwitchBgT(target_t));
        world.remove::<AnimateSwitchBgT>(entity);
        world.remove::<AnimateThumbX>(entity);
    }
    Some(ValueUpdate {
        old: old_on,
        new: on_now,
        change: PropertyChange::Visual,
    })
}

fn emit_switch_event(world: &mut World, entity: Entity, event: &SwitchEvent) {
    let cb = world
        .get::<SwitchHandler>(entity)
        .map(|h| h.on_event.clone_out());
    if let Some(cb) = cb {
        cb.call(world, entity, event);
    }
}

fn switch_attach(world: &mut World, entity: Entity) {
    if world.get::<Switch>(entity).is_none() {
        return;
    }
    let on = world.get::<Switch>(entity).map(|s| s.on).unwrap_or(false);
    let initial_t = if on { Fixed::ONE } else { Fixed::ZERO };
    world.insert(entity, SwitchBgT(initial_t));
    // AnimatedThumbX seeded later by switch_init_system once
    // ComputedRect exists.
}

pub fn view() -> View {
    View::new("Switch", 60, switch_render)
        .with_filter::<Switch>()
        .with_attach(switch_attach)
        .with_internal_gesture(switch_handler)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dirty::VisualDirty;

    fn h(_: &mut World, _: Entity, _: &SwitchEvent) -> bool {
        true
    }

    #[test]
    fn build_spawns_switch_with_style_and_handler() {
        let mut world = World::new();
        let e = Switch::build()
            .style(crate::ui::Style::default())
            .on_change(h)
            .spawn(&mut world);
        assert!(world.has::<Switch>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<SwitchHandler>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_handler_omits_it() {
        let mut world = World::new();
        let e = Switch::build().spawn(&mut world);
        assert!(world.has::<Switch>(e));
        assert!(!world.has::<SwitchHandler>(e));
        assert!(!world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn external_value_change_reports_old_new_and_does_not_restart() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Switch::new());
        world.insert(entity, ComputedRect(Rect::new(0, 0, 54, 24)));

        let first = set_switch_on(&mut world, entity, true).unwrap();
        assert!(!first.old);
        assert!(first.new);
        assert_eq!(first.change, PropertyChange::Visual);
        let spring = &mut world.get_mut::<AnimateSwitchBgT>(entity).unwrap().0;
        spring.tick(16);
        let progressed = spring.value();
        let repeated = set_switch_on(&mut world, entity, true).unwrap();
        assert_eq!(repeated.change, PropertyChange::Unchanged);
        assert_eq!(
            world.get::<AnimateSwitchBgT>(entity).unwrap().0.value(),
            progressed
        );
    }

    #[test]
    fn tap_callback_can_remove_switch_after_invalidation() {
        fn remove_switch(world: &mut World, entity: Entity, _: &SwitchEvent) -> bool {
            assert!(world.has::<VisualDirty>(entity));
            world.despawn(entity);
            true
        }

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Switch::new());
        world.insert(
            entity,
            SwitchHandler {
                on_event: BusinessCallback::Fn(remove_switch),
            },
        );
        assert!(switch_handler(
            &mut world,
            entity,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: entity,
            },
        ));
        assert!(!world.is_alive(entity));
    }

    #[test]
    fn on_thumb_x_tracks_track_width() {
        let narrow = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(40),
            h: Fixed::from_int(20),
        };
        let wide = Rect {
            w: Fixed::from_int(120),
            ..narrow
        };
        assert!(
            on_thumb_x(&wide) > on_thumb_x(&narrow),
            "ON knob must move right as the track widens so resize tracks the edge",
        );
        assert_eq!(off_thumb_x(&wide), off_thumb_x(&narrow));
    }

    #[test]
    fn init_seeds_switches_after_their_layout_is_available() {
        let mut world = World::new();
        let ready = world.spawn_empty();
        let waiting = world.spawn_empty();
        world.insert(ready, Switch::new());
        world.insert(waiting, Switch::new());
        world.insert(ready, ComputedRect(Rect::new(0, 0, 54, 24)));

        switch_init_system(&mut world);
        let first = world.get::<AnimatedThumbX>(ready).unwrap().0;
        assert!(world.get::<AnimatedThumbX>(waiting).is_none());

        world.insert(waiting, ComputedRect(Rect::new(0, 0, 72, 24)));
        switch_init_system(&mut world);
        assert_eq!(world.get::<AnimatedThumbX>(ready).unwrap().0, first);
        assert_eq!(world.get::<AnimatedThumbX>(waiting).unwrap().0, first);
    }

    #[test]
    fn closure_handler_captures_and_updates_state() {
        use crate::core::reactive::Signal;

        let mut world = World::new();
        let switched = Signal::new(false);
        let captured = switched.clone();
        let e = world.spawn_empty();
        world.insert(
            e,
            SwitchHandler {
                on_event: BusinessCallback::Closure(alloc::rc::Rc::new(
                    move |_w, _e, ev: &SwitchEvent| {
                        let SwitchEvent::Toggled { now } = ev;
                        captured.set(*now);
                        true
                    },
                )),
            },
        );

        emit_switch_event(&mut world, e, &SwitchEvent::Toggled { now: true });
        assert!(
            switched.get(),
            "closure handler updated the captured Signal"
        );
    }
}
