// View registry — the type-level definition of one widget kind.
//
// `Widget` (in `widget/mod.rs`) is a per-entity marker. `View` is the
// per-kind dispatch entry: render fn + auto-attach fn + priority.
// `App::default_views()` populates the registry with built-ins; user
// code adds custom kinds via `App::register_view`.

use alloc::vec::Vec;
use core::any::TypeId;

use crate::ecs::{Entity, World};
use crate::input::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::types::{Point, Rect, Transform};
use crate::ui::Style;
use crate::ui::theme::WidgetState;

/// Boxed as a struct so adding fields later doesn't change the
/// `ViewRender` fn-pointer signature (which would break user views).
pub struct ViewCtx<'a> {
    pub style: &'a Style,
    pub transform: Transform,
    pub quad: Option<[Point; 4]>,
    pub clip: &'a Rect,
    /// Set true by a view that emits its own background fill so the
    /// generic Style stage skips its bg fill but still emits border.
    pub bg_handled: bool,
    pub state: WidgetState,
    pub(crate) error: Option<RenderError>,
}

impl ViewCtx<'_> {
    /// Submit a draw and retain its first failure for the render walker.
    pub fn draw(&mut self, renderer: &mut dyn Renderer, command: &DrawCommand<'_>, clip: &Rect) {
        if self.error.is_none() {
            self.error = renderer.submit(&DrawRequest::new(command, *clip)).err();
        }
    }

    /// Replay borrowed scene operations and retain the first failure.
    pub fn replay(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_with_scratch(
            renderer,
            ops,
            resolver,
            crate::render::scene::replay::ReplayScratch::new(&mut frames, &mut routes, &mut scopes),
        );
    }

    /// Replay a scene using caller-owned group frames.
    pub fn replay_with_workspace(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        frames: &mut [crate::render::scene::replay::ReplayFrame],
    ) {
        if self.error.is_some() {
            return;
        }
        self.error = crate::render::scene::replay::replay_scene_with_workspace(
            ops, renderer, self.clip, resolver, frames,
        )
        .err()
        .map(Self::replay_error);
    }

    /// Replay a scene with caller-owned storage for isolated groups.
    pub fn replay_with_rgba(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        rgba: &mut [u8],
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_with_scratch(
            renderer,
            ops,
            resolver,
            crate::render::scene::replay::ReplayScratch::new(&mut frames, &mut routes, &mut scopes)
                .with_rgba(rgba),
        );
    }

    /// Replay borrowed scene operations under one shared root transform.
    pub fn replay_transformed_with_rgba(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        transform: Transform,
        rgba: &mut [u8],
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_transformed_with_scratch(
            renderer,
            ops,
            resolver,
            transform,
            crate::render::scene::replay::ReplayScratch::new(&mut frames, &mut routes, &mut scopes)
                .with_rgba(rgba),
        );
    }

    /// Replay a scene using caller-owned group, route, and scope storage.
    pub fn replay_with_scratch(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        scratch: crate::render::scene::replay::ReplayScratch<'_>,
    ) {
        self.replay_transformed_with_scratch(renderer, ops, resolver, Transform::IDENTITY, scratch);
    }

    /// Replay borrowed scene operations under one shared root transform and
    /// caller-owned replay storage.
    pub fn replay_transformed_with_scratch(
        &mut self,
        renderer: &mut dyn Renderer,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        transform: Transform,
        scratch: crate::render::scene::replay::ReplayScratch<'_>,
    ) {
        if self.error.is_some() {
            return;
        }
        self.error = crate::render::scene::replay::replay_scene_with_root(
            ops,
            renderer,
            self.clip,
            resolver,
            scratch,
            crate::render::scene::replay::ReplayFrame::with_transform(transform),
        )
        .err()
        .map(Self::replay_error);
    }

    fn replay_error(error: crate::render::scene::replay::ReplayError) -> RenderError {
        match error {
            crate::render::scene::replay::ReplayError::Render(error) => error,
            crate::render::scene::replay::ReplayError::Bounds(
                crate::render::scene::bbox::BoundsError::ProjectiveGroup,
            ) => {
                RenderError::Unsupported(crate::render::renderer::RenderFeature::ProjectiveGeometry)
            }
            crate::render::scene::replay::ReplayError::Bounds(
                crate::render::scene::bbox::BoundsError::GlyphInk,
            ) => RenderError::Unsupported(crate::render::renderer::RenderFeature::TextInkBounds),
            crate::render::scene::replay::ReplayError::Bounds(
                crate::render::scene::bbox::BoundsError::InsufficientWorkspace { .. },
            ) => RenderError::MissingWorkspace,
            crate::render::scene::replay::ReplayError::GroupOpacityNeedsOffscreen => {
                RenderError::MissingWorkspace
            }
            crate::render::scene::replay::ReplayError::InsufficientWorkspace {
                required,
                available,
            } => RenderError::InsufficientWorkspace {
                required_bytes: required.saturating_mul(core::mem::size_of::<
                    crate::render::scene::replay::ReplayFrame,
                >()),
                capacity_bytes: available.saturating_mul(core::mem::size_of::<
                    crate::render::scene::replay::ReplayFrame,
                >()),
            },
            crate::render::scene::replay::ReplayError::InsufficientScopePlans {
                required,
                available,
            } => RenderError::InsufficientWorkspace {
                required_bytes: required.saturating_mul(core::mem::size_of::<
                    crate::render::scene::replay::ReplayScopePlan,
                >()),
                capacity_bytes: available.saturating_mul(core::mem::size_of::<
                    crate::render::scene::replay::ReplayScopePlan,
                >()),
            },
            crate::render::scene::replay::ReplayError::UnresolvedFont
            | crate::render::scene::replay::ReplayError::UnresolvedTexture => {
                RenderError::InvalidTexture
            }
            _ => RenderError::InvalidGeometry,
        }
    }

    pub(crate) fn record(&mut self, result: Result<(), RenderError>) {
        if self.error.is_none() {
            self.error = result.err();
        }
    }

    /// Active [`crate::ui::Theme`]. Lazy lookup so render fns
    /// that don't need fallback colors pay nothing. `App::new`
    /// guarantees the resource is present.
    pub fn theme<'w>(&self, world: &'w World) -> &'w crate::ui::Theme {
        world
            .resource::<crate::ui::Theme>()
            .expect("App::new must insert Theme; missing means a test fixture skipped App")
    }
}

pub type ViewRender =
    fn(renderer: &mut dyn Renderer, world: &World, entity: Entity, rect: &Rect, ctx: &mut ViewCtx);

pub type ViewAttach = fn(world: &mut World, entity: Entity);

pub type ViewInternalGesture = fn(&mut World, Entity, &GestureEvent) -> bool;

/// Collects subscriptions for one entity and one View registration.
#[doc(hidden)]
pub struct ViewObservationBindings {
    subscriptions: Vec<crate::core::model::ModelSubscription>,
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
        self.subscriptions.push(subscribe(self.invalidation));
    }

    fn reset(&mut self, invalidation: ViewInvalidation, subscription_count: usize) {
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

    fn assert_complete(&self) {
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

pub type ViewObserve = fn(&World, Entity, &mut ViewObservationBindings);

/// Dirty scope used for both filtered-component changes and model notifications.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewInvalidation {
    #[default]
    Visual,
    Layout,
}

pub struct View {
    name: &'static str,
    /// Lower runs earlier. Slot reservation: 0..30 pre-bg,
    /// 30..50 explicit-bg widgets, 50 generic Style, 60..80 content
    /// widgets, 80..100 overlays.
    priority: u8,
    render: ViewRender,
    auto_attach: Option<ViewAttach>,
    systems: &'static [crate::ecs::System],
    /// When set, the walker only invokes `render` on entities that
    /// own a component of this type. `None` runs on every entity
    /// (e.g. the generic Style stage).
    component_filter: Option<fn() -> TypeId>,
    /// Component-internal gesture handler that runs *before* the user
    /// `GestureHandler` channel during `bubble_dispatch`. Returning
    /// `true` consumes the event; returning `false` lets bubble walk
    /// continue (and visit any user-attached `GestureHandler`).
    internal_gesture: Option<ViewInternalGesture>,
    observe: Option<ViewObserve>,
    observation_count: usize,
    invalidation: ViewInvalidation,
    registration_id: Option<u64>,
    component_tracking_ready: bool,
}

impl View {
    pub const fn new(name: &'static str, priority: u8, render: ViewRender) -> Self {
        Self {
            name,
            priority,
            render,
            auto_attach: None,
            systems: &[],
            component_filter: None,
            internal_gesture: None,
            observe: None,
            observation_count: 0,
            invalidation: ViewInvalidation::Visual,
            registration_id: None,
            component_tracking_ready: false,
        }
    }

    pub const fn with_attach(mut self, attach: ViewAttach) -> Self {
        self.auto_attach = Some(attach);
        self
    }

    pub const fn with_systems(mut self, systems: &'static [crate::ecs::System]) -> Self {
        self.systems = systems;
        self
    }

    /// Restrict `render` dispatch to entities owning component `T`.
    pub const fn with_filter<T: crate::ecs::Component>(mut self) -> Self {
        self.component_filter = Some(TypeId::of::<T>);
        self
    }

    /// Register a component-internal gesture handler. Runs before the
    /// user `GestureHandler` channel during `bubble_dispatch`; return
    /// `false` to let the user handler also see the event.
    pub const fn with_internal_gesture(mut self, f: ViewInternalGesture) -> Self {
        self.internal_gesture = Some(f);
        self
    }

    /// Describe the instance sources to bind for each matching component.
    /// Requires `with_filter::<Component>()` on the same View.
    pub const fn with_observation(
        mut self,
        observe: ViewObserve,
        subscription_count: usize,
    ) -> Self {
        self.observe = Some(observe);
        self.observation_count = subscription_count;
        self
    }

    /// Select the dirty scope for this View's component and model dependencies.
    pub const fn with_invalidation(mut self, invalidation: ViewInvalidation) -> Self {
        self.invalidation = invalidation;
        self
    }

    pub(crate) fn observation(&self) -> Option<ViewObserve> {
        self.observe
    }

    #[doc(hidden)]
    pub fn registration_id(&self) -> u64 {
        self.registration_id.expect("View has not been registered")
    }

    /// Marker widget: only contributes systems, no rendering.
    pub const fn systems_only(name: &'static str, systems: &'static [crate::ecs::System]) -> Self {
        fn noop_render(
            _renderer: &mut dyn Renderer,
            _world: &World,
            _entity: Entity,
            _rect: &Rect,
            _ctx: &mut ViewCtx,
        ) {
        }
        Self {
            name,
            priority: 100,
            render: noop_render,
            auto_attach: None,
            systems,
            component_filter: None,
            internal_gesture: None,
            observe: None,
            observation_count: 0,
            invalidation: ViewInvalidation::Visual,
            registration_id: None,
            component_tracking_ready: false,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
    pub(crate) fn priority(&self) -> u8 {
        self.priority
    }
    pub(crate) fn render(&self) -> ViewRender {
        self.render
    }
    pub(crate) fn auto_attach(&self) -> Option<ViewAttach> {
        self.auto_attach
    }
    pub(crate) fn component_filter(&self) -> Option<TypeId> {
        self.component_filter.map(|f| f())
    }
    pub(crate) fn internal_gesture(&self) -> Option<ViewInternalGesture> {
        self.internal_gesture
    }
    pub(crate) fn invalidation(&self) -> ViewInvalidation {
        self.invalidation
    }

    /// Hand each contributed `System` to `sink`. Called by `App` at
    /// view-registration time; `View` owns when and how its systems
    /// surface — callers don't read `self.systems` directly.
    pub(crate) fn install(&self, _world: &mut World, mut sink: impl FnMut(crate::ecs::System)) {
        for s in self.systems {
            sink(s.fresh());
        }
    }
}

#[derive(Default)]
pub struct ViewRegistry {
    views: Vec<View>,
    next_registration: u64,
    bindings: Vec<ViewBinding>,
    owner: Option<crate::ecs::world::WorldId>,
}

struct ViewBinding {
    entity: Entity,
    registration_id: u64,
    active: bool,
    subscriptions: ViewObservationBindings,
}

impl ViewRegistry {
    /// Rebind changed components before layout and paint inspect their state.
    pub(crate) fn reconcile_observations(world: &mut World) {
        world.with_resource_box::<Self, _>(|registry, world| {
            let owner = world.id();
            match registry.owner {
                None => registry.owner = Some(owner),
                Some(registered) => {
                    assert_eq!(registered, owner, "ViewRegistry belongs to a different App")
                }
            }
            for view in &mut registry.views {
                if !view.component_tracking_ready
                    && let Some(type_id) = view.component_filter()
                {
                    world.watch_component_type(type_id);
                }
                view.component_tracking_ready = true;
            }
            world.drain_component_changes(|world, change| registry.apply_change(world, change));
        });
    }

    fn apply_change(&mut self, world: &mut World, change: crate::ecs::world::ComponentChange) {
        for view in &self.views {
            if view.component_filter() != Some(change.type_id) {
                continue;
            }
            if let Some(observe) = view.observation() {
                let registration_id = view.registration_id();
                let slot = self.bindings.iter().position(|binding| {
                    binding.entity == change.entity && binding.registration_id == registration_id
                });
                let slot = slot.or_else(|| {
                    self.bindings.iter().position(|binding| {
                        !binding.active && binding.registration_id == registration_id
                    })
                });
                let slot =
                    slot.or_else(|| self.bindings.iter().position(|binding| !binding.active));
                let index = slot.unwrap_or_else(|| {
                    self.bindings.push(ViewBinding {
                        entity: change.entity,
                        registration_id,
                        active: false,
                        subscriptions: ViewObservationBindings::default(),
                    });
                    self.bindings.len() - 1
                });
                let binding = &mut self.bindings[index];
                binding
                    .subscriptions
                    .reset(view.invalidation(), view.observation_count);
                binding.entity = change.entity;
                binding.registration_id = registration_id;
                binding.active =
                    world.is_alive(change.entity) && world.has_type(change.entity, change.type_id);
                if binding.active {
                    observe(world, change.entity, &mut binding.subscriptions);
                    binding.subscriptions.assert_complete();
                }
            }
            if world.is_alive(change.entity) {
                match view.invalidation() {
                    ViewInvalidation::Visual => world.invalidate_visual(change.entity),
                    ViewInvalidation::Layout => world.invalidate(change.entity),
                }
            }
        }
    }

    /// Pre-populated registry containing every built-in widget kind.
    /// `App::with_default_widgets` iterates this to wire each widget's
    /// `View::systems` into the scheduler before storing the registry
    /// as a `World` resource.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        reg.insert(super::style_view::view());
        reg.insert(crate::ui::widgets::button::view());
        reg.insert(crate::ui::widgets::checkbox::view());
        reg.insert(crate::ui::widgets::progress_bar::view());
        reg.insert(crate::ui::widgets::tabbar::view());
        reg.insert(crate::ui::widgets::text_input::view());
        reg.insert(crate::ui::widgets::image::view());
        reg.insert(crate::ui::widgets::text::view());
        reg.insert(crate::ui::widgets::text::static_glyph_run_view());
        reg.insert(crate::ui::widgets::slider::view());
        reg.insert(crate::ui::widgets::switch::view());
        reg.insert(crate::ui::widgets::tab_pages::view());
        reg.insert(crate::ui::widgets::lazy_list::view());
        reg.insert(crate::ui::widgets::mirror::view());
        reg.insert(crate::ui::widgets::temporal_mix::view());
        reg.insert(crate::ui::widgets::background_blur::view());
        reg.insert(crate::ui::widgets::drop_shadow::view());
        reg.insert(crate::ui::widgets::drop_shadow::glow_view());
        reg.insert(crate::ui::widgets::icon::view());
        reg
    }

    /// Add a view, keeping the internal vec sorted by priority.
    /// Stable on equal priority — insertion order breaks ties.
    pub fn insert(&mut self, mut view: View) {
        if view.observe.is_some() {
            assert!(
                view.component_filter.is_some(),
                "observed View requires a component filter"
            );
        }
        view.registration_id = Some(self.next_registration);
        view.component_tracking_ready = false;
        self.next_registration = self
            .next_registration
            .checked_add(1)
            .expect("View registration identity exhausted");
        let pos = self
            .views
            .iter()
            .position(|v| v.priority() > view.priority())
            .unwrap_or(self.views.len());
        self.views.insert(pos, view);
    }

    pub fn iter(&self) -> impl Iterator<Item = &View> {
        self.views.iter()
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

impl IntoIterator for ViewRegistry {
    type Item = View;
    type IntoIter = alloc::vec::IntoIter<View>;

    fn into_iter(self) -> Self::IntoIter {
        self.views.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelHandle;
    use crate::core::reactive::flush_signal_dirty;
    use crate::ecs::query::system::{ComponentDriver, SystemQuery, Write};
    use crate::test_support::tracked_allocations;
    use crate::types::Fixed;
    use crate::ui::dirty::{Dirty, VisualDirty};
    use core::cell::Cell;

    #[crate::model]
    struct BoundState {
        #[observe]
        value: u32,
    }

    #[crate::model]
    impl BoundState {
        fn set(&mut self, value: u32) {
            self.value = value;
        }
    }

    #[derive(Clone, Copy)]
    struct VisualChange(u8);

    impl VisualChange {
        const REDRAW: Self = Self(1);

        fn contains(self, mask: Self) -> bool {
            self.0 & mask.0 == mask.0
        }
    }

    #[crate::model(change = VisualChange, watch(visual = VisualChange::REDRAW))]
    struct VisualState {
        value: u32,
    }

    #[crate::model]
    impl VisualState {
        fn set(&mut self, value: u32) -> VisualChange {
            self.value = value;
            VisualChange::REDRAW
        }
    }

    #[crate::component]
    struct VisualTile {
        visual: model!(VisualState),
    }

    #[crate::component]
    struct GenericTile<T: Copy + 'static> {
        value: T,
    }

    #[crate::component]
    struct VisualOnlyTile;

    #[crate::component]
    struct LayoutTile;

    #[crate::component]
    struct BoundTile {
        model: model!(BoundState),
    }

    #[crate::component]
    struct OtherBoundTile {
        model: model!(BoundState),
    }

    #[crate::component]
    struct PairTile {
        first: model!(BoundState),
        second: model!(BoundState),
    }

    #[crate::component]
    struct OptionalBoundTile {
        model: model!(optional BoundState),
        visits: Cell<u8>,
        seen: Cell<Option<u32>>,
    }

    fn observe_bound_tile(world: &World, entity: Entity, bindings: &mut ViewObservationBindings) {
        let tile = world.get::<BoundTile>(entity).expect("bound tile");
        bindings.watch(|invalidation| {
            tile.model
                .subscribe_observed(0, world, entity, invalidation)
        });
    }

    fn observe_other_bound_tile(
        world: &World,
        entity: Entity,
        bindings: &mut ViewObservationBindings,
    ) {
        let tile = world
            .get::<OtherBoundTile>(entity)
            .expect("other bound tile");
        bindings.watch(|invalidation| {
            tile.model
                .subscribe_observed(0, world, entity, invalidation)
        });
    }

    #[crate::view(priority = 62)]
    fn paint_bound_tile(
        _component: component!(BoundTile),
        model: model!(_component.model, watch(value)),
        renderer: draw!(),
        rect: rect!(),
        ctx: view!(),
    ) {
        let _ = (renderer, rect);
        ctx.bg_handled = model.value > 0;
    }

    #[crate::view]
    fn paint_pair_tile(
        _component: component!(PairTile),
        first: model!(_component.first, watch(value)),
        second: model!(_component.second, watch(value)),
        ctx: view!(),
    ) {
        ctx.bg_handled = first.value < second.value;
    }

    #[crate::view]
    fn paint_optional_bound_tile(
        component: component!(OptionalBoundTile),
        model: model!(optional component.model, watch(value)),
        ctx: view!(),
    ) {
        component.visits.set(component.visits.get() + 1);
        component.seen.set(model.map(|model| model.value));
        ctx.bg_handled = model.is_some();
    }

    #[crate::view]
    fn paint_visual_tile(
        component: component!(VisualTile),
        _visual: model!(component.visual, watch(visual_revision)),
        ctx: view!(),
    ) {
        ctx.bg_handled = true;
    }

    #[crate::view]
    fn paint_generic_tile<T: Copy + 'static>(component: component!(GenericTile<T>), ctx: view!()) {
        let _ = component.value;
        ctx.bg_handled = true;
    }

    #[crate::view]
    fn invalid_writing_view(component: component!(BoundTile)) {
        component.model.set(7);
    }

    fn options_attach(_: &mut World, _: Entity) {}

    fn options_gesture(_: &mut World, _: Entity, _: &GestureEvent) -> bool {
        false
    }

    fn options_system(_: &mut World) {}

    #[crate::view(
        priority = 42,
        attach = options_attach,
        gesture = options_gesture,
        systems = [crate::ecs::System::new("options", 100, options_system)]
    )]
    fn paint_options(component: component!(GenericTile<u8>), theme: res!(crate::ui::Theme)) {
        let _ = (component.value, theme);
    }

    #[test]
    fn unresolved_text_ink_is_a_capability_error() {
        assert_eq!(
            ViewCtx::replay_error(crate::render::scene::replay::ReplayError::Bounds(
                crate::render::scene::bbox::BoundsError::GlyphInk,
            )),
            RenderError::Unsupported(crate::render::renderer::RenderFeature::TextInkBounds)
        );
    }

    fn dummy_render(
        _renderer: &mut dyn Renderer,
        _world: &World,
        _entity: Entity,
        _rect: &Rect,
        _ctx: &mut ViewCtx,
    ) {
    }

    fn make_view(name: &'static str, priority: u8) -> View {
        View::new(name, priority, dummy_render)
    }

    #[test]
    fn filtered_views_invalidate_without_model_observations() {
        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(make_view("visual", 60).with_filter::<VisualOnlyTile>());
        app.with_widget(
            make_view("layout", 61)
                .with_filter::<LayoutTile>()
                .with_invalidation(ViewInvalidation::Layout),
        );
        let visual = app.world.spawn_empty();
        let layout = app.world.spawn_empty();
        app.world.insert(visual, VisualOnlyTile);
        app.world.insert(layout, LayoutTile);

        ViewRegistry::reconcile_observations(&mut app.world);

        assert!(app.world.has::<VisualDirty>(visual));
        assert!(!app.world.has::<Dirty>(visual));
        assert!(app.world.has::<Dirty>(layout));
        assert!(
            app.world
                .resource::<ViewRegistry>()
                .expect("view registry")
                .bindings
                .is_empty()
        );
    }

    #[test]
    fn install_preserves_system_expect_tag() {
        struct Marker;
        impl crate::ecs::Component for Marker {}
        fn dummy_run(_: &mut World) {}

        const EXPECT: &[crate::__macro_support::SystemAccess] =
            &[crate::__macro_support::SystemAccess::expect_component::<
                Marker,
            >()];
        const SYSTEMS: &[crate::ecs::System] =
            &[unsafe { crate::ecs::System::new("tagged", 100, dummy_run).with_metadata(EXPECT) }];

        let view = View::new("V", 60, dummy_render).with_systems(SYSTEMS);
        let mut world = World::new();
        let mut installed: Vec<crate::ecs::System> = Vec::new();
        view.install(&mut world, |s| installed.push(s));

        assert_eq!(installed.len(), 1);
        assert_eq!(
            installed[0].metadata().len(),
            1,
            "expect must survive install"
        );
    }

    #[test]
    fn observation_registration_identity_survives_priority_sorting() {
        fn observe(_: &World, _: Entity, _: &mut ViewObservationBindings) {}
        struct Marker;
        impl crate::ecs::Component for Marker {}

        let mut registry = ViewRegistry::default();
        registry.insert(
            make_view("late", 80)
                .with_filter::<Marker>()
                .with_observation(observe, 0),
        );
        registry.insert(
            make_view("early", 20)
                .with_filter::<Marker>()
                .with_observation(observe, 0),
        );
        let views: Vec<_> = registry.iter().collect();
        assert_eq!((views[0].name(), views[0].registration_id()), ("early", 1));
        assert_eq!((views[1].name(), views[1].registration_id()), ("late", 0));
        assert!(views.iter().all(|view| view.observation().is_some()));
    }

    #[test]
    fn observed_view_registration_tracks_its_component_type() {
        fn observe(_: &World, _: Entity, _: &mut ViewObservationBindings) {}
        struct Marker;
        impl crate::ecs::Component for Marker {}

        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(
            make_view("observed", 60)
                .with_filter::<Marker>()
                .with_observation(observe, 0),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(entity, Marker);
        ViewRegistry::reconcile_observations(&mut app.world);
        let registry = app.world.resource::<ViewRegistry>().expect("view registry");
        assert!(registry.bindings.iter().any(|binding| {
            binding.active
                && binding.entity == entity
                && binding.registration_id == registry.views[0].registration_id()
        }));
    }

    #[test]
    #[should_panic(expected = "did not create its declared subscription count")]
    fn observed_view_rejects_too_few_subscriptions() {
        fn observe(_: &World, _: Entity, _: &mut ViewObservationBindings) {}

        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(
            make_view("under-subscribed", 60)
                .with_filter::<VisualOnlyTile>()
                .with_observation(observe, 1),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(entity, VisualOnlyTile);
        ViewRegistry::reconcile_observations(&mut app.world);
    }

    #[test]
    #[should_panic(expected = "exceeded its declared subscription count")]
    fn observed_view_rejects_too_many_subscriptions() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("over-subscribed", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 0),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(entity, BoundTile { model });
        ViewRegistry::reconcile_observations(&mut app.world);
    }

    #[test]
    fn later_view_registration_replays_existing_components() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        app.with_widget(
            make_view("first", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        ViewRegistry::reconcile_observations(&mut app.world);

        app.world
            .resource_mut::<ViewRegistry>()
            .expect("view registry")
            .insert(
                make_view("second", 61)
                    .with_filter::<BoundTile>()
                    .with_observation(observe_bound_tile, 1),
            );
        ViewRegistry::reconcile_observations(&mut app.world);

        let registry = app.world.resource::<ViewRegistry>().expect("view registry");
        assert_eq!(
            registry
                .bindings
                .iter()
                .filter(|binding| binding.active && binding.entity == entity)
                .count(),
            2
        );

        app.world.remove::<VisualDirty>(entity);
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    #[should_panic(expected = "ViewRegistry belongs to a different App")]
    fn view_registry_cannot_move_between_apps_with_live_bindings() {
        let mut first = crate::app::App::headless(32, 32);
        first.with_widget(make_view("owned", 60).with_filter::<VisualOnlyTile>());
        let entity = first.world.spawn_empty();
        first.world.insert(entity, VisualOnlyTile);
        ViewRegistry::reconcile_observations(&mut first.world);

        let registry = first
            .world
            .remove_resource::<ViewRegistry>()
            .expect("first registry");
        let mut second = crate::app::App::headless(32, 32);
        second.world.insert_resource(registry);
        ViewRegistry::reconcile_observations(&mut second.world);
    }

    #[test]
    fn layout_view_model_subscription_invalidates_layout() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("layout-bound", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1)
                .with_invalidation(ViewInvalidation::Layout),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<Dirty>(entity);
        app.world.remove::<VisualDirty>(entity);

        model.set(1);
        flush_signal_dirty(&mut app.world);

        assert!(app.world.has::<Dirty>(entity));
        assert!(!app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn replacing_a_component_rebinds_only_its_current_model() {
        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 0 });
        let second = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("bound", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: first.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        first.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        app.world.insert(
            entity,
            BoundTile {
                model: second.clone(),
            },
        );
        second.set(1);
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        first.set(2);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
        second.set(2);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        app.world.remove::<BoundTile>(entity);
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);
        second.set(3);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn mutable_query_rebinds_view_to_the_current_model_without_allocating() {
        type BoundTiles<'world> =
            SystemQuery<'world, ComponentDriver<BoundTile>, Write<BoundTile>, ()>;

        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 0 });
        let second = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("bound-query", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: first.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);

        for replacement in [second.clone(), first.clone()] {
            for tile in BoundTiles::new(&mut app.world) {
                tile.model = replacement.clone();
            }
            ViewRegistry::reconcile_observations(&mut app.world);
        }
        app.world.remove::<VisualDirty>(entity);

        let replacement = second.clone();
        let allocations = tracked_allocations(|| {
            for tile in BoundTiles::new(&mut app.world) {
                tile.model = replacement.clone();
            }
            ViewRegistry::reconcile_observations(&mut app.world);
        });
        assert_eq!(allocations, 0);

        app.world.remove::<VisualDirty>(entity);
        first.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
        second.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn pending_model_notifications_do_not_allocate_during_batch_rebind() {
        const ENTITY_COUNT: usize = 65;
        type BoundTiles<'world> =
            SystemQuery<'world, ComponentDriver<BoundTile>, Write<BoundTile>, ()>;

        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 0 });
        let second = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("bound-batch", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        let mut entities = Vec::with_capacity(ENTITY_COUNT);
        for _ in 0..ENTITY_COUNT {
            let entity = app.world.spawn_empty();
            app.world.insert(
                entity,
                BoundTile {
                    model: first.clone(),
                },
            );
            entities.push(entity);
        }
        ViewRegistry::reconcile_observations(&mut app.world);
        for entity in entities.iter().copied() {
            app.world.remove::<VisualDirty>(entity);
        }

        // Keep one full generation of notifications pending while every View
        // binding moves to another model instance.
        first.set(1);
        let replacement = second.clone();
        let allocations = tracked_allocations(|| {
            for tile in BoundTiles::new(&mut app.world) {
                tile.model = replacement.clone();
            }
            ViewRegistry::reconcile_observations(&mut app.world);
        });

        assert_eq!(allocations, 0);
        flush_signal_dirty(&mut app.world);
    }

    #[test]
    fn removing_one_view_binding_preserves_another_on_the_same_entity() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("first", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        app.with_widget(
            make_view("second", 61)
                .with_filter::<OtherBoundTile>()
                .with_observation(observe_other_bound_tile, 1),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        app.world.insert(
            entity,
            OtherBoundTile {
                model: model.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);

        app.world.remove::<BoundTile>(entity);
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        app.world.remove::<OtherBoundTile>(entity);
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);
        model.set(2);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn existing_components_attach_when_observed_view_is_registered() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        app.with_widget(paint_bound_tile::view());
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn generated_view_borrows_current_model_only_while_painting() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) {}
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        let view = paint_bound_tile::view();
        let style = Style::default();
        let rect = Rect::new(0, 0, 32, 32);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let mut renderer = StubRenderer;
        (view.render())(&mut renderer, &app.world, entity, &rect, &mut ctx);
        assert!(!ctx.bg_handled);

        model.set(1);
        (view.render())(&mut renderer, &app.world, entity, &rect, &mut ctx);
        assert!(ctx.bg_handled);
        model.set(2);
    }

    #[test]
    fn generated_view_reads_two_instances_and_two_aliases() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) {}
        }

        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 1 });
        let second = app.add_model(BoundState { value: 2 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            PairTile {
                first: first.clone(),
                second: second.clone(),
            },
        );
        let view = paint_pair_tile::view();
        let style = Style::default();
        let rect = Rect::new(0, 0, 32, 32);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let mut renderer = StubRenderer;
        (view.render())(&mut renderer, &app.world, entity, &rect, &mut ctx);
        assert!(ctx.bg_handled);

        app.world.insert(
            entity,
            PairTile {
                first: first.clone(),
                second: first,
            },
        );
        (view.render())(&mut renderer, &app.world, entity, &rect, &mut ctx);
        assert!(!ctx.bg_handled);
    }

    #[test]
    fn optional_model_view_runs_with_and_without_an_instance() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) {}
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 7 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            OptionalBoundTile {
                model: None,
                visits: Cell::new(0),
                seen: Cell::new(None),
            },
        );
        let view = paint_optional_bound_tile::view();
        let style = Style::default();
        let rect = Rect::new(0, 0, 32, 32);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };

        (view.render())(&mut StubRenderer, &app.world, entity, &rect, &mut ctx);
        let component = app.world.get::<OptionalBoundTile>(entity).unwrap();
        assert_eq!(component.visits.get(), 1);
        assert_eq!(component.seen.get(), None);
        assert!(!ctx.bg_handled);

        app.world.insert(
            entity,
            OptionalBoundTile {
                model: Some(model),
                visits: Cell::new(1),
                seen: Cell::new(None),
            },
        );
        (view.render())(&mut StubRenderer, &app.world, entity, &rect, &mut ctx);
        let component = app.world.get::<OptionalBoundTile>(entity).unwrap();
        assert_eq!(component.visits.get(), 2);
        assert_eq!(component.seen.get(), Some(7));
        assert!(ctx.bg_handled);
    }

    #[test]
    fn optional_model_observation_rebinds_without_allocating() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        app.with_widget(paint_optional_bound_tile::view());
        let entity = app.world.spawn_empty();

        for bound in [None, Some(model.clone()), None, Some(model.clone())] {
            app.world.insert(
                entity,
                OptionalBoundTile {
                    model: bound,
                    visits: Cell::new(0),
                    seen: Cell::new(None),
                },
            );
            ViewRegistry::reconcile_observations(&mut app.world);
        }
        app.world.remove::<VisualDirty>(entity);

        let allocations = tracked_allocations(|| {
            app.world.insert(
                entity,
                OptionalBoundTile {
                    model: None,
                    visits: Cell::new(0),
                    seen: Cell::new(None),
                },
            );
            ViewRegistry::reconcile_observations(&mut app.world);
        });
        assert_eq!(allocations, 0);

        app.world.remove::<VisualDirty>(entity);
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));

        let allocations = tracked_allocations(|| {
            app.world.insert(
                entity,
                OptionalBoundTile {
                    model: Some(model.clone()),
                    visits: Cell::new(0),
                    seen: Cell::new(None),
                },
            );
            ViewRegistry::reconcile_observations(&mut app.world);
        });
        assert_eq!(allocations, 0);
        app.world.remove::<VisualDirty>(entity);
        model.set(2);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn generated_view_subscribes_to_named_revision() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(VisualState { value: 0 });
        app.with_widget(paint_visual_tile::view());
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            VisualTile {
                visual: model.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(entity);
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn generic_component_view_creates_a_monomorphized_adapter() {
        let view = paint_generic_tile::view::<u8>();
        assert_eq!(
            view.component_filter(),
            Some(TypeId::of::<GenericTile<u8>>())
        );
    }

    #[test]
    fn generated_view_preserves_existing_builder_options() {
        let view = paint_options::view();
        assert_eq!(view.priority(), 42);
        assert!(view.auto_attach().is_some());
        assert!(view.internal_gesture().is_some());
        assert_eq!(view.systems.len(), 1);
    }

    #[test]
    fn generated_view_rejects_model_writes_and_restores_scope_after_panic() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) {}
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        let view = invalid_writing_view::view();
        let style = Style::default();
        let rect = Rect::new(0, 0, 32, 32);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (view.render())(&mut StubRenderer, &app.world, entity, &rect, &mut ctx);
        }));
        assert!(failure.is_err());
        model.set(2);
        assert_eq!(model.value(), 2);
    }

    #[test]
    fn despawned_observed_component_releases_its_subscription() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 });
        app.with_widget(
            make_view("bound", 60)
                .with_filter::<BoundTile>()
                .with_observation(observe_bound_tile, 1),
        );
        let entity = app.world.spawn_empty();
        app.world.insert(
            entity,
            BoundTile {
                model: model.clone(),
            },
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.despawn(entity));
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(
            app.world
                .resource::<ViewRegistry>()
                .unwrap()
                .bindings
                .iter()
                .all(|binding| !binding.active && binding.subscriptions.subscriptions.is_empty())
        );
        model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.is_alive(entity));
    }

    #[test]
    fn insert_keeps_priority_order() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("c", 80));
        reg.insert(make_view("a", 40));
        reg.insert(make_view("b", 50));
        let names: Vec<&str> = reg.iter().map(|v| v.name()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn insert_is_stable_within_same_priority() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("first-50", 50));
        reg.insert(make_view("second-50", 50));
        reg.insert(make_view("third-50", 50));
        let names: Vec<&str> = reg.iter().map(|v| v.name()).collect();
        assert_eq!(names, ["first-50", "second-50", "third-50"]);
    }

    // A view fn must be able to mutate `ViewCtx` while concurrently
    // borrowing components from `&World`. Real renders read multiple
    // components alongside the ctx-mut path; the borrow pattern must
    // compose. Pinning that here so a regression breaks loudly.
    #[test]
    fn render_fn_can_mutate_ctx_while_reading_world() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) {}
        }

        fn flip_bg_when_styled(
            _renderer: &mut dyn Renderer,
            world: &World,
            entity: Entity,
            _rect: &Rect,
            ctx: &mut ViewCtx,
        ) {
            if world.get::<Style>(entity).is_some() {
                ctx.bg_handled = true;
            }
        }

        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, Style::default());

        let style = world.get::<Style>(e).expect("style present");
        let rect = Rect::new(0, 0, 0, 0);
        let mut ctx = ViewCtx {
            style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let mut renderer = StubRenderer;

        flip_bg_when_styled(&mut renderer, &world, e, &rect, &mut ctx);
        assert!(
            ctx.bg_handled,
            "view fn must mutate ctx while reading world"
        );

        // Pin the type aliases — failure here means signature drift
        // breaks user-code views.
        let _: ViewRender = flip_bg_when_styled;
        let _ = Fixed::ZERO;
    }

    #[test]
    fn borrowed_scene_replay_retains_render_failure() {
        struct RejectingRenderer;

        impl Renderer for RejectingRenderer {
            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request).map(|_| ())
            }

            fn flush(&mut self) {}
        }

        let style = Style::default();
        let clip = Rect::new(0, 0, 8, 8);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &clip,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let ops = [crate::render::scene::SceneOp::FillRect {
            area: clip,
            transform: Transform::default(),
            quad: None,
            color: crate::types::Color::rgb(20, 30, 40),
            radius: crate::types::Fixed::ZERO,
            opa: 255,
        }];
        ctx.replay(
            &mut RejectingRenderer,
            &ops,
            &crate::render::scene::resolver::SliceResolver::new(&[], &[]),
        );
        assert_eq!(
            ctx.error,
            Some(RenderError::Unsupported(
                crate::render::renderer::RenderFeature::AffineGeometry
            ))
        );
    }

    #[test]
    fn transformed_scene_replay_composes_one_root_transform() {
        #[derive(Default)]
        struct CapturingRenderer {
            transform: Option<Transform>,
        }

        impl Renderer for CapturingRenderer {
            fn route(
                &self,
                _: &DrawRequest<'_, '_>,
            ) -> Result<crate::render::renderer::RenderRoute, RenderError> {
                Ok(crate::render::renderer::RenderRoute::Native)
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.transform = Some(request.command.transform());
                Ok(())
            }

            fn flush(&mut self) {}
        }

        let style = Style::default();
        let clip = Rect::new(0, 0, 64, 64);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::IDENTITY,
            quad: None,
            clip: &clip,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let ops = [crate::render::scene::SceneOp::FillRect {
            area: Rect::new(0, 0, 8, 8),
            transform: Transform::translate(Fixed::from_int(2), Fixed::from_int(3)),
            quad: None,
            color: crate::types::Color::rgb(255, 255, 255),
            radius: Fixed::ZERO,
            opa: 255,
        }];
        let root = Transform::scale(Fixed::from_int(2), Fixed::from_int(2));
        let mut renderer = CapturingRenderer::default();
        let mut rgba = [];

        ctx.replay_transformed_with_rgba(
            &mut renderer,
            &ops,
            &crate::render::scene::resolver::SliceResolver::new(&[], &[]),
            root,
            &mut rgba,
        );

        assert_eq!(
            renderer.transform,
            Some(root.compose(&Transform::translate(
                Fixed::from_int(2),
                Fixed::from_int(3),
            )))
        );
    }

    #[test]
    fn theme_accessor_returns_world_resource() {
        use crate::ui::Theme;
        use crate::ui::theme::ColorToken;
        let mut world = World::new();
        world.insert_resource(Theme::light());
        let style = Style::default();
        let rect = Rect::new(0, 0, 0, 0);
        let ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let theme = ctx.theme(&world);
        // Theme isn't PartialEq (BTreeMap of extras); compare via resolve.
        assert_eq!(
            theme.resolve(ColorToken::Surface),
            Theme::light().resolve(ColorToken::Surface),
        );
    }

    #[test]
    #[should_panic(expected = "App::new must insert Theme")]
    fn theme_accessor_panics_when_resource_missing() {
        let world = World::new();
        let style = Style::default();
        let rect = Rect::new(0, 0, 0, 0);
        let ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let _ = ctx.theme(&world);
    }
}
