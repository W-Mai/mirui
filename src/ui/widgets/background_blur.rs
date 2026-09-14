use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::Renderer;
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::types::{Fixed, Point, Rect, Transform};
use crate::ui::view::{View, ViewCtx};

pub struct BackgroundBlur {
    pub radius: Fixed,
    pub spread: Fixed,
}

impl BackgroundBlur {
    pub fn new(radius: impl Into<Fixed>) -> Self {
        Self {
            radius: radius.into(),
            spread: Fixed::ZERO,
        }
    }

    pub fn with_spread(mut self, spread: impl Into<Fixed>) -> Self {
        self.spread = spread.into();
        self
    }
}

fn background_blur_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(bg) = world.get::<BackgroundBlur>(entity) else {
        return;
    };
    if bg.radius <= Fixed::ZERO {
        return;
    }

    let base_rect = ctx.transform.apply_rect_bbox(*rect);
    let alpha = alpha_for_radius(bg.radius);

    let padding = bg.radius + bg.spread;
    let sample_rect = if padding > Fixed::ZERO {
        base_rect.inflate(padding)
    } else {
        base_rect
    };

    renderer.prepare_readback(&sample_rect);

    let mut tmp = match renderer.sample_target_region(&sample_rect) {
        Ok(Some(texture)) => texture,
        Ok(None) => return,
        Err(error) => {
            ctx.record(Err(error));
            return;
        }
    };
    {
        let r = crate::types::Rect::new(
            crate::types::Fixed::ZERO,
            crate::types::Fixed::ZERO,
            crate::types::Fixed::from_int(tmp.width as i32),
            crate::types::Fixed::from_int(tmp.height as i32),
        );
        iir_blur_inplace(&mut tmp, alpha, r);
    }

    // sample_rect already has ctx.transform baked in; passing it again would double-translate.
    ctx.draw(
        renderer,
        &DrawCommand::Blit {
            pos: Point::new(sample_rect.x, sample_rect.y),
            size: Point::new(sample_rect.w, sample_rect.h),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &tmp,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("BackgroundBlur", 60, background_blur_render).with_filter::<BackgroundBlur>()
}
