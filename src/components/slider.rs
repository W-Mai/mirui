use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::gesture::GestureEvent;
use crate::types::{Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

#[derive(Clone, Debug)]
pub enum SliderEvent {
    ValueChanged { new: Fixed, old: Fixed },
    DragStarted,
    DragEnded,
}

pub struct SliderHandler {
    pub on_event: fn(&mut World, Entity, &SliderEvent) -> bool,
}

pub struct Slider {
    pub value: Fixed,
    pub min: Fixed,
    pub max: Fixed,
    pub track_color: ThemedColor,
    pub fill_color: ThemedColor,
    pub thumb_color: ThemedColor,
}

impl Default for Slider {
    fn default() -> Self {
        Self::new(Fixed::ZERO, Fixed::ONE)
    }
}

impl Slider {
    pub fn new(min: Fixed, max: Fixed) -> Self {
        Self {
            value: min,
            min,
            max,
            track_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            fill_color: ThemedColor::Token(ColorToken::Primary),
            thumb_color: ThemedColor::Token(ColorToken::OnPrimary),
        }
    }

    pub fn with_track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.track_color = color.into();
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.fill_color = color.into();
        self
    }

    pub fn with_thumb_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.thumb_color = color.into();
        self
    }

    pub fn ratio(&self) -> Fixed {
        let range = self.max - self.min;
        if range <= Fixed::ZERO {
            return Fixed::ZERO;
        }
        (self.value - self.min) / range
    }

    pub fn set_ratio(&mut self, ratio: Fixed) {
        let clamped = ratio.clamp(Fixed::ZERO, Fixed::ONE);
        self.value = self.min + clamped * (self.max - self.min);
    }
}

fn slider_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(s) = world.get::<Slider>(entity) else {
        return;
    };
    let theme = ctx.theme(world);
    let track_color = s.track_color.resolve_in(theme, ctx.state);
    let fill_color = s.fill_color.resolve_in(theme, ctx.state);
    let thumb_color = s.thumb_color.resolve_in(theme, ctx.state);
    let ratio = s.ratio();
    let cap_radius = rect.h / Fixed::from_int(2);

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

    // Fill bar: full-width capsule with the right side cut off via a
    // narrowed clip rect — preserves the rounded right end at any ratio.
    let ratio_w = rect.w * ratio;
    if ratio_w > Fixed::ZERO {
        let ratio_box = Rect {
            x: rect.x,
            y: rect.y,
            w: ratio_w,
            h: rect.h,
        };
        if let Some(fill_clip) = ctx.clip.intersect(&ratio_box) {
            renderer.draw(
                &DrawCommand::Fill {
                    area: *rect,
                    transform: ctx.transform,
                    quad: ctx.quad,
                    color: fill_color,
                    radius: cap_radius,
                    opa: 255,
                },
                &fill_clip,
            );
        }
    }

    let thumb_size = rect.h;
    let thumb_x = rect.x + ratio * (rect.w - thumb_size);
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: thumb_x,
                y: rect.y,
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

pub(crate) fn slider_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragStart { .. } => {
            emit_slider_event(world, entity, &SliderEvent::DragStarted);
            return true;
        }
        GestureEvent::DragEnd { .. } => {
            emit_slider_event(world, entity, &SliderEvent::DragEnded);
            return true;
        }
        GestureEvent::Tap { .. } | GestureEvent::DragMove { .. } => {}
        _ => return false,
    }

    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|r| r.0) else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let local = (x - rect.x).max(Fixed::ZERO);
    let ratio = local / rect.w;

    let (old_value, new_value) = {
        let Some(s) = world.get_mut::<Slider>(entity) else {
            return false;
        };
        let old = s.value;
        s.set_ratio(ratio);
        (old, s.value)
    };
    if old_value != new_value {
        emit_slider_event(
            world,
            entity,
            &SliderEvent::ValueChanged {
                new: new_value,
                old: old_value,
            },
        );
    }
    world.insert(entity, Dirty);
    true
}

fn emit_slider_event(world: &mut World, entity: Entity, event: &SliderEvent) {
    let cb = world.get::<SliderHandler>(entity).map(|h| h.on_event);
    if let Some(f) = cb {
        f(world, entity, event);
    }
}

fn slider_attach(world: &mut World, entity: Entity) {
    let _ = world;
    let _ = entity;
}

pub fn view() -> View {
    View::new("Slider", 60, slider_render)
        .with_filter::<Slider>()
        .with_attach(slider_attach)
        .with_internal_gesture(slider_handler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Mutex;

    static EVENTS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    static LAST_NEW: AtomicI64 = AtomicI64::new(0);
    static LAST_OLD: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn record_handler(_w: &mut World, _e: Entity, ev: &SliderEvent) -> bool {
        let mut v = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
        match ev {
            SliderEvent::ValueChanged { new, old } => {
                LAST_NEW.store(new.to_int() as i64, Ordering::SeqCst);
                LAST_OLD.store(old.to_int() as i64, Ordering::SeqCst);
                v.push("ValueChanged");
            }
            SliderEvent::DragStarted => v.push("DragStarted"),
            SliderEvent::DragEnded => v.push("DragEnded"),
        }
        true
    }

    fn fresh() -> (World, Entity) {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
        world.insert(
            e,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );
        world.insert(
            e,
            SliderHandler {
                on_event: record_handler,
            },
        );
        EVENTS.lock().unwrap_or_else(|x| x.into_inner()).clear();
        LAST_NEW.store(0, Ordering::SeqCst);
        LAST_OLD.store(0, Ordering::SeqCst);
        (world, e)
    }

    fn drain_events() -> Vec<&'static str> {
        EVENTS.lock().unwrap_or_else(|x| x.into_inner()).clone()
    }

    #[test]
    fn tap_emits_value_changed_with_new_old() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mut world, e) = fresh();
        let event = GestureEvent::Tap {
            x: Fixed::from_int(50),
            y: Fixed::ZERO,
            target: e,
        };
        slider_handler(&mut world, e, &event);
        assert_eq!(drain_events(), &["ValueChanged"]);
        assert_eq!(LAST_OLD.load(Ordering::SeqCst), 0);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 50);
    }

    #[test]
    fn drag_move_emits_value_changed() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mut world, e) = fresh();
        let event = GestureEvent::DragMove {
            x: Fixed::from_int(75),
            y: Fixed::ZERO,
            dx: Fixed::ZERO,
            dy: Fixed::ZERO,
            target: e,
        };
        slider_handler(&mut world, e, &event);
        assert_eq!(drain_events(), &["ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 75);
    }

    #[test]
    fn no_value_change_no_emit() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mut world, e) = fresh();
        if let Some(s) = world.get_mut::<Slider>(e) {
            s.value = Fixed::from_int(50);
        }
        let event = GestureEvent::Tap {
            x: Fixed::from_int(50),
            y: Fixed::ZERO,
            target: e,
        };
        slider_handler(&mut world, e, &event);
        assert!(
            drain_events().is_empty(),
            "tapping at the current value must not emit",
        );
    }

    #[test]
    fn drag_start_emits_drag_started() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mut world, e) = fresh();
        let event = GestureEvent::DragStart {
            x: Fixed::from_int(40),
            y: Fixed::ZERO,
            target: e,
        };
        slider_handler(&mut world, e, &event);
        assert_eq!(drain_events(), &["DragStarted"]);
    }

    #[test]
    fn drag_end_emits_drag_ended() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mut world, e) = fresh();
        let event = GestureEvent::DragEnd {
            x: Fixed::from_int(40),
            y: Fixed::ZERO,
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: e,
        };
        slider_handler(&mut world, e, &event);
        assert_eq!(drain_events(), &["DragEnded"]);
    }
}
