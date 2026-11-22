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
#[path = "../../tests/unit/viewport/overlay.rs"]
mod tests;
