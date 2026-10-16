use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::types::{Fixed, Point, Transform};

#[crate::component]
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

#[crate::view(component = BackgroundBlur, name = "BackgroundBlur", priority = 60)]
fn background_blur_render(bg: component!(), paint: paint!()) {
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
        match tmp.pixels_mut() {
            Ok(mut pixels) => iir_blur_inplace(&mut pixels, alpha, r),
            Err(error) => {
                paint.fail(error);
                return;
            }
        }
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

pub use background_blur_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::Rect;
    use crate::ui::Style;
    use crate::ui::theme::WidgetState;
    use crate::ui::view::PaintCtx;

    struct FailingReadback;

    impl Renderer for FailingReadback {
        type TargetState = ();

        fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            panic!("readback preparation failure must stop the draw")
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }

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

        view().render().unwrap()(&world, entity, &mut paint);

        assert_eq!(paint.error(), Some(RenderError::BackendFailure));
    }

    #[test]
    fn typed_view_samples_transformed_bounds_and_blits_without_double_transform() {
        use crate::ui::view::ViewInvalidation;
        use core::cell::{Cell, RefCell};

        struct PreparedReadback {
            bounds: Rect,
            texture: RefCell<Option<Texture<'static>>>,
            prepared: Cell<bool>,
            draws: usize,
        }

        impl Renderer for PreparedReadback {
            type TargetState = ();

            fn prepare_readback(&mut self, region: &Rect) -> Result<(), RenderError> {
                assert_eq!(*region, self.bounds);
                assert!(!self.prepared.replace(true));
                Ok(())
            }

            fn sample_target_region(
                &self,
                region: &Rect,
            ) -> Result<Option<Texture<'static>>, RenderError> {
                assert!(self.prepared.get());
                assert_eq!(*region, self.bounds);
                Ok(self.texture.borrow_mut().take())
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                let DrawCommand::Blit {
                    pos,
                    size,
                    transform,
                    texture,
                    ..
                } = request.command
                else {
                    panic!("background blur submits one sampled texture");
                };
                assert_eq!(*pos, Point::new(self.bounds.x, self.bounds.y));
                assert_eq!(*size, Point::new(self.bounds.w, self.bounds.h));
                assert_eq!(*transform, Transform::IDENTITY);
                assert_eq!(texture.width, 38);
                assert_eq!(texture.height, 38);
                assert_eq!(request.clip, Rect::new(0, 0, 100, 100));
                self.draws += 1;
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let descriptor = view();
        assert_eq!(
            descriptor.component_filter(),
            Some(core::any::TypeId::of::<BackgroundBlur>())
        );
        assert_eq!(descriptor.priority(), Some(60));
        assert_eq!(descriptor.invalidation(), ViewInvalidation::Visual);
        assert!(descriptor.auto_attach().is_none());
        assert!(descriptor.inputs().is_none());
        let render = descriptor.render().unwrap();
        let mut world = World::new();
        let entity = world.spawn(BackgroundBlur::new(4).with_spread(3));
        let style = Style::default();
        let rect = Rect::new(4, 4, 12, 12);
        let clip = Rect::new(0, 0, 100, 100);
        let transform = Transform::translate(Fixed::from_int(10), Fixed::from_int(20))
            .compose(&Transform::scale(Fixed::from_int(2), Fixed::from_int(2)));

        for _ in 0..2 {
            let mut renderer = PreparedReadback {
                bounds: Rect::new(11, 21, 38, 38),
                texture: RefCell::new(Some(
                    Texture::owned(38, 38, ColorFormat::RGBA8888).expect("valid pixel storage"),
                )),
                prepared: Cell::new(false),
                draws: 0,
            };
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &clip,
                transform,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| render(&world, entity, &mut paint)),
                0
            );
            assert_eq!(paint.error(), None);
            assert_eq!(renderer.draws, 1);
            assert!(renderer.texture.borrow().is_none());
        }
    }
}
