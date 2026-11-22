use crate::raster::{Canvas, FrameOverlay, OverlayFrame};
use alloc::rc::Rc;
use core::{
    cell::{Ref, RefCell},
    marker::PhantomData,
};
use mirui::{
    __macro_support::{BindingRole, OwnedEffects, prepare_widget_effect, with_owned_effects},
    ecs::{Entity, World},
    render::RenderError,
    ui::Prop,
};

#[cfg(test)]
use crate::allocations;

/// Composites a screen-space layer into the attached scene viewport.
#[mirui::component]
pub struct ViewportOverlay {
    source: Rc<dyn OverlaySource>,
    effects: Option<OwnedEffects>,
}

impl ViewportOverlay {
    /// Builds an overlay with static or observed inputs.
    pub fn new<T, P>(input: impl Into<Prop<T>>, painter: P) -> Self
    where
        T: Clone + PartialEq + 'static,
        P: FrameOverlay<T> + 'static,
    {
        let input = input.into();
        let value = input.__mirui_is_static().then(|| input.get());
        Self {
            source: Rc::new(Source {
                input,
                value: RefCell::new(value),
                painter: RefCell::new(Painter(painter, PhantomData)),
            }),
            effects: None,
        }
    }

    pub(super) fn attach(world: &mut World, entity: Entity) {
        let Some(overlay) = world.get::<Self>(entity) else {
            return;
        };
        if overlay.effects.is_some() || !overlay.source.is_dynamic() {
            return;
        }
        let source = overlay.source.clone();
        let (_, effects) = with_owned_effects(|| {
            prepare_widget_effect(entity, BindingRole::Display, move || {
                if source.refresh() {
                    mirui::ui::property::apply::<super::SourceChanged>(entity, ());
                }
            })
            .install(world);
        });
        world
            .get_mut::<Self>(entity)
            .expect("attached overlay")
            .effects = Some(effects);
    }

    pub(super) fn prepare(
        &self,
        frame: OverlayFrame,
    ) -> Result<Option<Ref<'_, dyn FrameOverlay>>, RenderError> {
        self.source.prepare(frame)
    }
}

trait OverlaySource {
    fn is_dynamic(&self) -> bool;
    fn refresh(&self) -> bool;
    fn prepare(
        &self,
        frame: OverlayFrame,
    ) -> Result<Option<Ref<'_, dyn FrameOverlay>>, RenderError>;
}

struct Source<T, P> {
    input: Prop<T>,
    value: RefCell<Option<T>>,
    painter: RefCell<Painter<T, P>>,
}

impl<T: Clone + PartialEq + 'static, P: FrameOverlay<T> + 'static> OverlaySource for Source<T, P> {
    fn is_dynamic(&self) -> bool {
        !self.input.__mirui_is_static()
    }

    fn refresh(&self) -> bool {
        let next = self.input.get();
        let mut value = self.value.borrow_mut();
        if value.as_ref() == Some(&next) {
            return false;
        }
        *value = Some(next);
        true
    }

    fn prepare(
        &self,
        frame: OverlayFrame,
    ) -> Result<Option<Ref<'_, dyn FrameOverlay>>, RenderError> {
        if self.value.borrow().is_none() {
            self.refresh();
        }
        let active = self.painter.borrow_mut().0.prepare(
            frame,
            self.value
                .borrow()
                .as_ref()
                .expect("prepared overlay input"),
        )?;
        Ok(active.then(|| {
            Ref::map(self.painter.borrow(), |painter| {
                painter as &dyn FrameOverlay
            })
        }))
    }
}

struct Painter<T, P>(P, PhantomData<fn() -> T>);

impl<T, P: FrameOverlay<T>> FrameOverlay for Painter<T, P> {
    fn covers(&self, rect: [i32; 4], scratch: &mut [u8]) -> bool {
        self.0.covers(rect, scratch)
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        self.0.paint(canvas);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{CameraWorkspace, Frame, RollMapping};
    use mirui::{
        render::texture::ColorFormat,
        types::{Color, Fixed},
    };

    struct Panel(u32);

    impl FrameOverlay<u32> for Panel {
        fn prepare(&mut self, _: OverlayFrame, color: &u32) -> Result<bool, RenderError> {
            self.0 = *color;
            Ok(true)
        }

        fn covers(&self, _: [i32; 4], _: &mut [u8]) -> bool {
            true
        }

        fn paint(&self, canvas: &mut Canvas<'_>) {
            let [left, top, right, bottom] = canvas.clip;
            for y in top..bottom {
                for x in left..right {
                    canvas.point(x, y, self.0);
                }
            }
        }
    }

    #[test]
    fn attached_overlay_borrows_the_compact_target_without_allocating() {
        let size = [128, 128];
        let mut workspace = CameraWorkspace::new(size, Fixed::ZERO, true).unwrap();
        let mut output = Frame::with_format(size[0], size[1], ColorFormat::RGB565).unwrap();
        let mapping = RollMapping::new(size, Fixed::ZERO, None);
        let overlay = ViewportOverlay::new(0xff00_00ff, Panel(0));
        let storage = workspace.bytes() + output.bytes();

        let allocations = allocations::tracked_allocations(|| {
            let overlay = overlay
                .prepare(OverlayFrame { size, camera: None })
                .unwrap()
                .unwrap();
            workspace
                .render(
                    &mut output,
                    &mapping,
                    Some(&*overlay),
                    |_| panic!("covered"),
                    |_| Ok(()),
                )
                .unwrap();
        });
        assert_eq!(allocations, 0);
        assert_eq!(workspace.bytes() + output.bytes(), storage);
        let pixels = output.texture.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(126, 126), Color::rgb(255, 0, 0));
    }
}
