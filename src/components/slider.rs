use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

pub struct Slider {
    pub value: Fixed,
    pub min: Fixed,
    pub max: Fixed,
    pub track_color: ThemedColor,
    pub fill_color: ThemedColor,
    pub thumb_color: ThemedColor,
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
    if let Some(s) = world.get_mut::<Slider>(entity) {
        s.set_ratio(ratio);
    }
    world.insert(entity, Dirty);
    true
}

fn slider_attach(world: &mut World, entity: Entity) {
    if world.get::<Slider>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: slider_handler,
        },
    );
}

pub fn view() -> View {
    View::new("Slider", 60, slider_render).with_attach(slider_attach)
}
