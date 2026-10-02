use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::types::{Fixed, Point, Transform};
use crate::ui::view::{PaintCtx, View};

pub struct BackgroundBlur {
    pub radius: Fixed,
    pub spread: Fixed,
}

impl crate::ecs::Component for BackgroundBlur {}

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

fn background_blur_render(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let Some(bg) = world.get::<BackgroundBlur>(entity) else {
        return;
    };
    if bg.radius <= Fixed::ZERO {
        return;
    }

    let base_rect = paint.transform().apply_rect_bbox(paint.rect());
    let alpha = alpha_for_radius(bg.radius);

    let padding = bg.radius + bg.spread;
    let sample_rect = if padding > Fixed::ZERO {
        base_rect.inflate(padding)
    } else {
        base_rect
    };

    let mut tmp = match paint.sample_target_region(sample_rect) {
        Some(texture) => texture,
        None => return,
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

    // sample_rect already has the paint transform baked in; applying it again would double-translate.
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(sample_rect.x, sample_rect.y),
        size: Point::new(sample_rect.w, sample_rect.h),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &tmp,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    });
}

pub fn view() -> View {
    View::new("BackgroundBlur", 60, background_blur_render).with_filter::<BackgroundBlur>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::types::Rect;
    use crate::ui::Style;
    use crate::ui::theme::WidgetState;

    struct FailingReadback;

    impl Renderer for FailingReadback {
        fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            panic!("readback preparation failure must stop the draw")
        }

        fn flush(&mut self) {}

        fn prepare_readback(&mut self, _: &Rect) -> Result<(), RenderError> {
            Err(RenderError::BackendFailure)
        }

        fn sample_target_region(
            &self,
            _: &Rect,
        ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
            panic!("failed preparation must stop before target sampling")
        }
    }

    #[test]
    fn readback_preparation_failure_stops_before_sampling() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, BackgroundBlur::new(4));
        let style = Style::default();
        let clip = Rect::new(0, 0, 32, 32);
        let rect = Rect::new(4, 4, 12, 12);
        let mut renderer = FailingReadback;
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &clip,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );

        background_blur_render(&world, entity, &mut paint);

        assert_eq!(paint.error(), Some(RenderError::BackendFailure));
    }
}
