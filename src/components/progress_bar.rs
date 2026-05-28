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

pub struct ProgressBar {
    pub value: f32, // 0.0 ~ 1.0
    pub track_color: ThemedColor,
    pub fill_color: ThemedColor,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            value: 0.0,
            track_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            fill_color: ThemedColor::Token(ColorToken::Primary),
        }
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.track_color = color.into();
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.fill_color = color.into();
        self
    }
}

fn progress_bar_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(pb) = world.get::<ProgressBar>(entity) else {
        return;
    };
    let theme = ctx.theme(world);
    let track_color = pb.track_color.resolve_in(theme, ctx.state);
    let fill_color = pb.fill_color.resolve_in(theme, ctx.state);
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: track_color,
            radius: ctx.style.border_radius,
            opa: 255,
        },
        ctx.clip,
    );
    let fill_w = Fixed::from_f32(rect.w.to_f32() * pb.value.clamp(0.0, 1.0));
    if fill_w > Fixed::ZERO {
        renderer.draw(
            &DrawCommand::Fill {
                area: Rect {
                    x: rect.x,
                    y: rect.y,
                    w: fill_w,
                    h: rect.h,
                },
                transform: ctx.transform,
                quad: ctx.quad,
                color: fill_color,
                radius: ctx.style.border_radius,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

/// Map pointer x onto the bar's ComputedRect to drive `value` in
/// [0, 1]. Both Tap and DragMove route here so dragging produces a
/// continuous update.
fn progress_bar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|c| c.0) else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let ratio = ((x - rect.x).to_f32() / rect.w.to_f32()).clamp(0.0, 1.0);
    if let Some(pb) = world.get_mut::<ProgressBar>(entity) {
        pb.value = ratio;
    }
    world.insert(entity, Dirty);
    true
}

fn progress_bar_attach(world: &mut World, entity: Entity) {
    if world.get::<ProgressBar>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: progress_bar_handler,
        },
    );
}

pub fn view() -> View {
    View::new("ProgressBar", 60, progress_bar_render)
        .with_filter::<ProgressBar>()
        .with_attach(progress_bar_attach)
}
