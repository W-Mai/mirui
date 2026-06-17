//! Frosted-glass blur of the framebuffer pixels behind this widget.
//! Must come after the content it blurs in children-array order;
//! skips silently when the renderer can't sample its own target.

use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::types::{Fixed, Point, Rect};
use crate::ui::view::{View, ViewCtx};

pub struct BackgroundBlur {
    /// Pixels of Gaussian-equivalent blur radius. `Fixed` (not `u8`)
    /// so an animation system (e.g. `mirui_macros::animate!`) can
    /// drive it through fractional values without stepping each
    /// integer.
    pub radius: Fixed,
}

impl BackgroundBlur {
    pub fn new(radius: impl Into<Fixed>) -> Self {
        Self {
            radius: radius.into(),
        }
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

    // Animated transforms must sample the on-screen rect, not the
    // layout rect, or the blur source freezes at the original spot.
    let sample_rect = ctx.transform.apply_rect_bbox(*rect);
    let alpha = alpha_for_radius(bg.radius);

    // Identity / translate-only with no 3D quad can blur in place,
    // skipping the alloc + sample-copy + blit-back round-trip. Rotated
    // or projected widgets still need the intermediate sample so the
    // blur runs on an axis-aligned source.
    use crate::types::TransformClass;
    let class = ctx.transform.classify();
    if matches!(class, TransformClass::Identity | TransformClass::Translate) && ctx.quad.is_none() {
        renderer.modify_target_region(&sample_rect, &mut |tex| {
            iir_blur_inplace(tex, alpha);
        });
        return;
    }

    let Some(mut tmp) = renderer.sample_target_region(&sample_rect) else {
        return;
    };
    iir_blur_inplace(&mut tmp, alpha);

    renderer.draw(
        &DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: ctx.transform,
            quad: ctx.quad,
            texture: &tmp,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("BackgroundBlur", 60, background_blur_render).with_filter::<BackgroundBlur>()
}
