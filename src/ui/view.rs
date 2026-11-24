//! Widget-kind descriptors and registration.
//!
//! Each [`View`] combines independent paint, attach, input, observation, and system capabilities. Paint priority affects only the paint traversal; per-entity hooks retain registration order, and systems keep their scheduling slots.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::Cell;

use crate::ecs::{Entity, World};
use crate::render::scratch::PlaneLayout;
use crate::types::Rect;
use crate::ui::outputs::OutputDependency;

mod paint;
mod registry;

pub(crate) use paint::FrameRequest;
pub use paint::PaintCtx;
pub use registry::ViewRegistry;

#[cfg(test)]
use crate::{
    render::{
        command::DrawCommand,
        path::Path,
        raster::FillRule,
        renderer::{
            DrawRequest, FallbackRegion, RenderError, RenderFeature, RenderRoute, Renderer,
            TextRunIdentity,
        },
        texture::Texture,
    },
    types::{Point, Transform},
    ui::{
        Style,
        outputs::{relations::OutputRelations, workspace::PaintWorkspace},
        theme::WidgetState,
    },
};
#[cfg(test)]
use core::task::{Context, Poll, Waker};

pub(crate) type ViewRender = fn(world: &World, entity: Entity, ctx: &mut PaintCtx<'_>);

pub(crate) use generated::ViewObservationBindings;

pub(crate) mod generated {
    use super::*;

    pub fn report_query_reads(
        paint: &mut PaintCtx<'_>,
        scope: &crate::ecs::query::system::QueryReadScope,
    ) {
        paint.query_reads_global = scope.is_global();
    }

    pub struct ViewDescriptor {
        pub name: &'static str,
        pub paint: Option<(u8, ViewRender)>,
        pub attach: Option<crate::input::event::view_attach::AttachDefinition>,
        pub input: Option<crate::input::event::view_input::InputFactory>,
        pub observation: Option<(ViewObserve, usize)>,
        pub query_dependencies: &'static [fn() -> TypeId],
        pub systems: &'static [crate::ecs::System],
        pub invalidation: ViewInvalidation,
    }

    pub fn assemble_view<C: crate::ecs::Component>(descriptor: ViewDescriptor) -> View {
        View::from_descriptor(descriptor, Some(TypeId::of::<C>()), C::OUTPUT_DEPENDENCIES)
    }

    /// Collects subscriptions for one entity and one View registration.
    pub struct ViewObservationBindings {
        pub(super) subscriptions: Vec<Option<crate::core::reactive::ViewSubscription>>,
        invalidation: crate::core::model::ModelInvalidation,
        subscription_count: usize,
    }

    impl ViewObservationBindings {
        pub fn watch(
            &mut self,
            subscribe: impl FnOnce(
                crate::core::model::ModelInvalidation,
            ) -> crate::core::model::ModelSubscription,
        ) {
            assert!(
                self.subscriptions.len() < self.subscription_count,
                "View observer exceeded its declared subscription count"
            );
            self.subscriptions
                .push(subscribe(self.invalidation).into_consumer());
        }

        pub(crate) fn watch_signal(
            &mut self,
            signal: Option<&crate::core::reactive::Signal<()>>,
            owner: crate::ecs::world::WorldId,
            entity: Entity,
        ) {
            assert!(
                self.subscriptions.len() < self.subscription_count,
                "View observer exceeded its declared subscription count"
            );
            let visual = matches!(
                self.invalidation,
                crate::core::model::ModelInvalidation::Visual
            );
            self.subscriptions
                .push(signal.map(|signal| signal.observe_view(owner, entity, visual)));
        }

        pub(super) fn reset(&mut self, invalidation: ViewInvalidation, subscription_count: usize) {
            self.subscriptions.clear();
            if self.subscriptions.capacity() < subscription_count {
                self.subscriptions
                    .try_reserve_exact(subscription_count)
                    .expect("View subscription capacity exhausted");
            }
            self.invalidation = match invalidation {
                ViewInvalidation::Visual => crate::core::model::ModelInvalidation::Visual,
                ViewInvalidation::Layout => crate::core::model::ModelInvalidation::Layout,
            };
            self.subscription_count = subscription_count;
        }

        pub(super) fn assert_complete(&self) {
            assert_eq!(
                self.subscriptions.len(),
                self.subscription_count,
                "View observer did not create its declared subscription count"
            );
        }
    }

    impl Default for ViewObservationBindings {
        fn default() -> Self {
            Self {
                subscriptions: Vec::new(),
                invalidation: crate::core::model::ModelInvalidation::Visual,
                subscription_count: 0,
            }
        }
    }
}

pub(crate) type ViewObserve = fn(&World, Entity, &mut ViewObservationBindings);

/// Dirty scope used for component changes, model notifications, and output dependencies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewInvalidation {
    #[default]
    Visual,
    Layout,
}

#[derive(Clone, Copy)]
struct ViewPaint {
    /// Lower runs earlier. Slot reservation: 0..30 pre-bg,
    /// 30..50 explicit-bg widgets, 50 generic Style, 60..80 content
    /// widgets, 80..100 overlays.
    priority: u8,
    render: ViewRender,
    measurement: Option<PaintMeasure>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceGeometry {
    pub plane: PlaneLayout,
    pub layout_rect: Rect,
    pub local_bounds: Rect,
}

#[derive(Clone, Copy)]
pub(crate) struct PaintMeasurement {
    pub coverage: super::dirty::PaintCoverage,
    pub workspace: Option<PlaneLayout>,
}

pub(crate) type PaintMeasure =
    fn(&World, Entity, Rect, &dyn Fn(Entity) -> Option<SourceGeometry>) -> Option<PaintMeasurement>;

/// Widget-kind descriptor with optional painting and independent behavior hooks.
pub struct View<Systems = &'static [crate::ecs::System]> {
    definition: ViewDefinition,
    systems: Systems,
}

struct ViewDefinition {
    name: &'static str,
    paint: Option<ViewPaint>,
    auto_attach: Option<crate::input::event::view_attach::Attachment>,
    reconcile: Option<fn(&mut World, Entity) -> bool>,
    prepared_layout: Option<&'static super::prepared_layout::LayoutHook>,
    retire: Option<fn(&mut World, Entity)>,
    /// When set, the walker only invokes `render` on entities that
    /// own a component of this type. `None` runs on every entity
    /// (e.g. the generic Style stage).
    component_filter: Option<TypeId>,
    query_dependencies: &'static [fn() -> TypeId],
    query_scope: Cell<QueryScope>,
    output_dependencies: &'static [OutputDependency],
    input: Option<crate::input::event::view_input::SharedInputs>,
    observe: Option<ViewObserve>,
    observation_count: usize,
    invalidation: ViewInvalidation,
    registration_id: Option<u64>,
    component_tracking_ready: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryScope {
    Unobserved,
    Local,
    Global,
}

impl View {
    fn from_descriptor(
        descriptor: generated::ViewDescriptor,
        component_filter: Option<TypeId>,
        output_dependencies: &'static [OutputDependency],
    ) -> Self {
        let paint = descriptor.paint.map(|(priority, render)| ViewPaint {
            priority,
            render,
            measurement: None,
        });
        let (observe, observation_count) = match descriptor.observation {
            Some((observe, count)) => (Some(observe), count),
            None => (None, 0),
        };
        Self {
            definition: ViewDefinition {
                name: descriptor.name,
                paint,
                auto_attach: descriptor.attach.map(Into::into),
                reconcile: None,
                prepared_layout: None,
                retire: None,
                component_filter,
                query_dependencies: descriptor.query_dependencies,
                query_scope: Cell::new(QueryScope::Unobserved),
                output_dependencies,
                input: descriptor.input.map(|factory| {
                    Rc::new(core::cell::RefCell::new(
                        crate::input::event::view_input::InputState::Pending(factory),
                    ))
                }),
                observe,
                observation_count,
                invalidation: descriptor.invalidation,
                registration_id: None,
                component_tracking_ready: false,
            },
            systems: descriptor.systems,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(name: &'static str, priority: u8, render: ViewRender) -> Self {
        let mut view = Self::systems_only(name, &[]);
        view.definition.paint = Some(ViewPaint {
            priority,
            render,
            measurement: None,
        });
        view
    }

    #[cfg(test)]
    pub(crate) fn systems_only(name: &'static str, systems: &'static [crate::ecs::System]) -> Self {
        Self::from_descriptor(
            generated::ViewDescriptor {
                name,
                paint: None,
                attach: None,
                systems,
                query_dependencies: &[],
                input: None,
                observation: None,
                invalidation: ViewInvalidation::Visual,
            },
            None,
            &[],
        )
    }
}

impl<Systems> View<Systems> {
    pub(crate) fn with_prepared_layout<P: super::prepared_layout::LayoutParticipant + Default>(
        mut self,
    ) -> Self {
        self.definition.prepared_layout =
            Some(&const { super::prepared_layout::LayoutHook::new::<P>() });
        self
    }

    pub(crate) fn with_reconcile(mut self, reconcile: fn(&mut World, Entity) -> bool) -> Self {
        self.definition.reconcile = Some(reconcile);
        self
    }

    pub(crate) fn reconcile(&self) -> Option<fn(&mut World, Entity) -> bool> {
        self.definition.reconcile
    }

    /// Release attached state when its component generation ends. The entity may be dead or already carry the replacement component; the hook must not remove that replacement.
    pub(crate) fn with_retire(mut self, retire: fn(&mut World, Entity)) -> Self {
        self.definition.retire = Some(retire);
        self
    }

    pub(crate) fn retire(&self) -> Option<fn(&mut World, Entity)> {
        self.definition.retire
    }

    #[cfg(test)]
    pub(crate) fn with_attach(mut self, attach: fn(&mut crate::ui::UiScope<'_>)) -> Self {
        self.definition.auto_attach = Some(
            crate::input::event::view_attach::Attachment::Composition(attach),
        );
        self
    }

    pub(crate) const fn with_measurement(mut self, measure: PaintMeasure) -> Self {
        if let Some(paint) = &mut self.definition.paint {
            paint.measurement = Some(measure);
        }
        self
    }

    pub(crate) fn measurement(&self) -> Option<PaintMeasure> {
        self.definition
            .paint
            .as_ref()
            .and_then(|paint| paint.measurement)
    }

    pub(crate) fn measurement_reusable(&self) -> bool {
        self.definition.component_filter.is_some()
            && self.definition.component_tracking_ready
            && self.definition.query_scope.get() == QueryScope::Local
    }

    /// Replace the systems installed with this widget kind.
    ///
    /// Accepts owned system arrays or borrowed system templates. Systems belong to the App scheduler, not to individual widget entities; hiding or removing an entity does not uninstall them.
    pub fn with_systems<NewSystems>(self, systems: NewSystems) -> View<NewSystems>
    where
        NewSystems: IntoIterator,
        NewSystems::Item: Into<crate::ecs::System>,
    {
        View {
            definition: self.definition,
            systems,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_filter<T: crate::ecs::Component>(mut self) -> Self {
        self.definition.component_filter = Some(TypeId::of::<T>());
        self.definition.output_dependencies = T::OUTPUT_DEPENDENCIES;
        self
    }

    #[cfg(test)]
    pub(crate) const fn with_query_dependencies(
        mut self,
        dependencies: &'static [fn() -> TypeId],
    ) -> Self {
        self.definition.query_dependencies = dependencies;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_inputs<I: crate::input::event::view_input::ViewInputs>(
        mut self,
        world: &mut World,
        inputs: I,
    ) -> Self {
        self.definition.input = Some(Rc::new(core::cell::RefCell::new(
            crate::input::event::view_input::InputState::Ready(
                crate::input::event::view_input::prepare_inputs(world, inputs).unwrap(),
            ),
        )));
        self
    }

    pub(crate) const fn with_observation(
        mut self,
        observe: ViewObserve,
        subscription_count: usize,
    ) -> Self {
        self.definition.observe = Some(observe);
        self.definition.observation_count = subscription_count;
        self
    }

    /// Select the dirty scope for this View's component and model dependencies.
    pub const fn with_invalidation(mut self, invalidation: ViewInvalidation) -> Self {
        self.definition.invalidation = invalidation;
        self
    }

    pub(crate) fn observation(&self) -> Option<ViewObserve> {
        self.definition.observe
    }

    #[doc(hidden)]
    pub fn registration_id(&self) -> u64 {
        self.definition
            .registration_id
            .expect("View has not been registered")
    }

    pub fn name(&self) -> &'static str {
        self.definition.name
    }
    pub(crate) fn priority(&self) -> Option<u8> {
        self.definition.paint.map(|paint| paint.priority)
    }
    pub(crate) fn render(&self) -> Option<ViewRender> {
        self.definition.paint.map(|paint| paint.render)
    }

    pub(crate) fn paint(&self, world: &World, entity: Entity, ctx: &mut PaintCtx<'_>) {
        let previous = self.definition.query_scope.replace(QueryScope::Global);
        ctx.query_reads_global = true;
        let render = self
            .render()
            .expect("paint order contains a non-paint View");
        // Generated and raw callbacks share this model write boundary.
        crate::core::model::with_model_read_only(|| render(world, entity, ctx));
        if previous != QueryScope::Global && !ctx.query_reads_global && ctx.error().is_none() {
            self.definition.query_scope.set(QueryScope::Local);
        }
    }
    pub(crate) fn auto_attach(&self) -> Option<&crate::input::event::view_attach::Attachment> {
        self.definition.auto_attach.as_ref()
    }
    pub(crate) fn component_filter(&self) -> Option<TypeId> {
        self.definition.component_filter
    }
    pub(crate) fn inputs(&self) -> Option<&crate::input::event::view_input::SharedInputs> {
        self.definition.input.as_ref()
    }
    pub(crate) fn invalidation(&self) -> ViewInvalidation {
        self.definition.invalidation
    }

    pub(crate) fn install(self, mut sink: impl FnMut(crate::ecs::System)) -> View
    where
        Systems: IntoIterator,
        Systems::Item: Into<crate::ecs::System>,
    {
        for system in self.systems {
            sink(system.into());
        }
        View {
            definition: self.definition,
            systems: &[],
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui/view/clip_tests.rs"]
mod clip_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/view/pending_tests.rs"]
mod pending_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/view/query_tests.rs"]
mod query_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/view/tests.rs"]
mod tests;
