//! Widget-kind descriptors and registration.
//!
//! Each [`View`] combines independent paint, attach, input, observation, and system capabilities. Paint priority affects only the paint traversal; per-entity hooks retain registration order, and systems keep their scheduling slots.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::Cell;
use core::task::{Context, Poll, Waker};

use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::renderer::{
    DrawRequest, FallbackRegion, RenderError, RenderFeature, RenderRoute, Renderer, TextRunIdentity,
};
use crate::render::scratch::PlaneLayout;
use crate::render::texture::{Texture, TexturePixels};
use crate::types::{Point, Rect, Transform};
use crate::ui::Style;
use crate::ui::outputs::workspace::PaintWorkspace;
use crate::ui::outputs::{OutputDependency, relations::OutputRelations};
use crate::ui::theme::WidgetState;

#[cfg(test)]
mod clip_tests;
#[cfg(test)]
mod pending_tests;
#[cfg(test)]
mod query_tests;

#[derive(Clone, Copy, Default)]
pub(crate) enum FrameRequest {
    #[default]
    None,
    Next,
    IfOutputChanged,
}

/// Drawing context supplied to one View paint invocation.
pub struct PaintCtx<'frame> {
    renderer: &'frame mut dyn Renderer,
    style: &'frame Style,
    rect: &'frame Rect,
    clip: &'frame Rect,
    transform: Transform,
    quad: Option<[Point; 4]>,
    state: WidgetState,
    background_handled: bool,
    error: Option<RenderError>,
    workspace: Option<&'frame PaintWorkspace>,
    layout_offset: Point,
    frame_request: FrameRequest,
    query_reads_global: bool,
    waker: &'frame Waker,
    resource_pending: bool,
}

impl<'frame> PaintCtx<'frame> {
    pub(crate) fn new(
        renderer: &'frame mut dyn Renderer,
        style: &'frame Style,
        rect: &'frame Rect,
        clip: &'frame Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        state: WidgetState,
    ) -> Self {
        Self {
            renderer,
            style,
            rect,
            clip,
            transform,
            quad,
            state,
            background_handled: false,
            error: None,
            workspace: None,
            layout_offset: Point::new(0, 0),
            frame_request: FrameRequest::None,
            query_reads_global: true,
            waker: Waker::noop(),
            resource_pending: false,
        }
    }

    pub fn style(&self) -> &Style {
        self.style
    }

    pub(crate) fn set_waker(&mut self, waker: &'frame Waker) {
        self.waker = waker;
    }

    pub(crate) fn resource_pending(&self) -> bool {
        self.resource_pending
    }

    /// Request one more paint after this output or main-target drawing batch succeeds. Repeated requests for the same entity coalesce; failed drawing discards its requests. State and dependency changes already invalidate their Views without this call.
    pub fn request_frame(&mut self) {
        self.frame_request = FrameRequest::Next;
    }

    pub(crate) fn request_frame_on_change(&mut self) {
        if matches!(self.frame_request, FrameRequest::None) {
            self.frame_request = FrameRequest::IfOutputChanged;
        }
    }

    pub(crate) fn frame_request(&self) -> FrameRequest {
        self.frame_request
    }

    pub(crate) fn set_workspace(&mut self, workspace: &'frame PaintWorkspace) {
        self.workspace = Some(workspace);
    }

    pub(crate) fn set_layout_offset(&mut self, offset: Point) {
        self.layout_offset = offset;
    }

    pub(crate) fn map_layout_rect(&self, rect: Rect) -> Rect {
        Rect {
            x: rect.x - self.layout_offset.x,
            y: rect.y - self.layout_offset.y,
            ..rect
        }
    }

    pub(crate) fn with_workspace(
        &mut self,
        layout: PlaneLayout,
        body: impl FnOnce(&mut Texture<'_>, &mut Self),
    ) -> bool {
        let Some(workspace) = self.workspace else {
            return false;
        };
        workspace.with_texture(layout, |texture| body(texture, self))
    }

    /// Borrow source pixels and retain any read failure for the render walker. Pending registers a resource wait and fails this paint attempt without publishing partial pixels. The pixels borrow the texture, not this paint context.
    pub fn read_pixels<'texture>(
        &mut self,
        texture: &'texture Texture<'_>,
    ) -> Option<TexturePixels<'texture>> {
        if self.error.is_some() {
            return None;
        }
        match texture.poll_read_pixels(&mut Context::from_waker(self.waker)) {
            Poll::Ready(Ok(pixels)) => Some(pixels),
            Poll::Ready(Err(error)) => {
                self.fail(error);
                None
            }
            Poll::Pending => {
                self.resource_pending = true;
                self.fail(RenderError::ResourcePending);
                None
            }
        }
    }

    pub fn rect(&self) -> Rect {
        *self.rect
    }

    /// The inherited clip in the current render target's logical coordinates.
    pub fn clip(&self) -> Rect {
        *self.clip
    }

    pub fn transform(&self) -> Transform {
        self.transform
    }

    pub fn quad(&self) -> Option<[Point; 4]> {
        self.quad
    }

    pub fn state(&self) -> WidgetState {
        self.state
    }

    pub fn output_scale(&self) -> crate::types::Fixed {
        self.renderer.output_scale()
    }

    pub fn background_handled(&self) -> bool {
        self.background_handled
    }

    pub fn set_background_handled(&mut self, handled: bool) {
        self.background_handled = handled;
    }

    /// Submit a draw and retain its first failure for the render walker.
    /// Path clip commands must use [`Self::with_clip_path`] so cleanup cannot
    /// be skipped after a body draw fails.
    pub fn draw(&mut self, command: &DrawCommand<'_>) {
        self.draw_clipped(command, *self.clip);
    }

    /// Submit a draw with an explicit clip in render-target logical coordinates and retain its first failure. The command's transform does not transform this clip.
    pub fn draw_clipped(&mut self, command: &DrawCommand<'_>, clip: Rect) {
        if self.poll_draw_clipped(command, clip).is_pending() {
            self.fail(RenderError::ResourcePending);
        }
    }

    /// Prepare and submit a command. Pending leaves this command unsubmitted and permits drawing a fallback; source readiness invalidates the View. A real error is retained even when the caller ignores the result.
    pub fn poll_draw(&mut self, command: &DrawCommand<'_>) -> Poll<Result<(), RenderError>> {
        self.poll_draw_clipped(command, *self.clip)
    }

    fn poll_draw_clipped(
        &mut self,
        command: &DrawCommand<'_>,
        clip: Rect,
    ) -> Poll<Result<(), RenderError>> {
        if let Some(error) = self.error {
            return Poll::Ready(Err(error));
        }
        if matches!(command, DrawCommand::PushClip { .. } | DrawCommand::PopClip) {
            self.fail(RenderError::InvalidScope);
            return Poll::Ready(Err(RenderError::InvalidScope));
        }
        if let Some(clip) = self.clip.intersect(&clip) {
            let request = DrawRequest::new(command, clip);
            let result = self.renderer.poll_draw_request(
                request,
                None,
                &mut Context::from_waker(self.waker),
            );
            match result {
                Poll::Pending => self.resource_pending = true,
                Poll::Ready(result) => self.retain_error(result),
            }
            return result;
        }
        Poll::Ready(Ok(()))
    }

    /// Draw inside a path clip, using the renderer's software scope when native path clipping is unavailable. The callback borrows the scope's drawing context and executes at most once.
    ///
    /// The body is skipped when entering the clip fails. If drawing the body fails, the matching pop still runs and the first failure is retained.
    pub fn with_clip_path(
        &mut self,
        path: &Path,
        transform: Transform,
        fill_rule: FillRule,
        body: impl FnOnce(&mut PaintCtx<'_>),
    ) {
        if self.error.is_some() {
            return;
        }

        let clip = *self.clip;
        let push = DrawCommand::PushClip {
            path,
            transform,
            fill_rule,
        };
        let region = match self.renderer.route(&DrawRequest::new(&push, clip)) {
            Ok(RenderRoute::Native) => {
                self.draw_clip_scope(&push, body);
                return;
            }
            Ok(RenderRoute::ExactFallback(region)) => Ok(region),
            Err(RenderError::Unsupported(RenderFeature::PathClip)) => {
                self.renderer.plan_scope(&clip)
            }
            Err(error) => Err(error),
        };
        match region {
            Ok(region) => self.draw_software_clip_scope(region, &push, body),
            Err(error) => self.fail(error),
        }
    }

    fn draw_clip_scope(&mut self, push: &DrawCommand<'_>, body: impl FnOnce(&mut PaintCtx<'_>)) {
        let clip = *self.clip;
        let result = self.renderer.draw_request(&DrawRequest::new(push, clip));
        self.retain_error(result);
        if self.error.is_some() {
            return;
        }

        body(self);

        let result = self
            .renderer
            .draw_request(&DrawRequest::new(&DrawCommand::PopClip, clip));
        self.retain_error(result);
    }

    fn draw_software_clip_scope(
        &mut self,
        region: FallbackRegion,
        push: &DrawCommand<'_>,
        body: impl FnOnce(&mut PaintCtx<'_>),
    ) {
        let mut body = Some(body);
        let mut error = None;
        let result = self.renderer.render_scope(region, &mut |renderer| {
            let body = body.take().ok_or(RenderError::InvalidScope)?;
            let mut paint = PaintCtx {
                renderer,
                style: self.style,
                rect: self.rect,
                clip: self.clip,
                transform: self.transform,
                quad: self.quad,
                state: self.state,
                background_handled: self.background_handled,
                error: None,
                workspace: self.workspace,
                layout_offset: self.layout_offset,
                frame_request: self.frame_request,
                query_reads_global: self.query_reads_global,
                waker: self.waker,
                resource_pending: self.resource_pending,
            };
            paint.draw_clip_scope(push, body);
            self.background_handled = paint.background_handled;
            self.frame_request = paint.frame_request;
            self.query_reads_global = paint.query_reads_global;
            self.resource_pending = paint.resource_pending;
            error = paint.error;
            error.map_or(Ok(()), Err)
        });
        if let Some(error) = error {
            self.fail(error);
        }
        self.retain_error(result.map(|_| ()));
    }

    /// Replay borrowed scene operations and retain the first failure.
    pub fn replay(
        &mut self,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_with_scratch(
            ops,
            resolver,
            crate::render::scene::replay::ReplayScratch::new(&mut frames, &mut routes, &mut scopes),
        );
    }

    /// Replay a scene using caller-owned group frames.
    pub fn replay_with_workspace(
        &mut self,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        frames: &mut [crate::render::scene::replay::ReplayFrame],
    ) {
        if self.error.is_some() {
            return;
        }
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_with_scratch(
            ops,
            resolver,
            crate::render::scene::replay::ReplayScratch::new(frames, &mut routes, &mut scopes),
        );
    }

    /// Replay a scene with caller-owned storage for isolated groups.
    pub fn replay_with_rgba(
        &mut self,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        rgba: &mut [u8],
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_with_scratch(
            ops,
            resolver,
            crate::render::scene::replay::ReplayScratch::new(&mut frames, &mut routes, &mut scopes)
                .with_rgba(rgba),
        );
    }

    /// Replay borrowed scene operations under one shared root transform.
    pub fn replay_transformed_with_rgba(
        &mut self,
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        transform: Transform,
        rgba: &mut [u8],
    ) {
        let mut frames = [crate::render::scene::replay::ReplayFrame::EMPTY; 8];
        let mut routes = [crate::render::scene::replay::ReplayPlan::EMPTY; 8];
        let mut scopes = [crate::render::scene::replay::ReplayScopePlan::EMPTY; 8];
        self.replay_transformed_with_scratch(
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
        ops: &[crate::render::scene::SceneOp],
        resolver: &dyn crate::render::scene::replay::SceneResolver,
        scratch: crate::render::scene::replay::ReplayScratch<'_>,
    ) {
        self.replay_transformed_with_scratch(ops, resolver, Transform::IDENTITY, scratch);
    }

    /// Replay borrowed scene operations under one shared root transform and
    /// caller-owned replay storage.
    pub fn replay_transformed_with_scratch(
        &mut self,
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
            self.renderer,
            self.clip,
            resolver,
            scratch.with_waker(self.waker),
            crate::render::scene::replay::ReplayFrame::with_transform(transform),
        )
        .err()
        .map(Self::replay_error);
        self.resource_pending |= self.error == Some(RenderError::ResourcePending);
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

    pub(crate) fn sample_target_region(&mut self, rect: Rect) -> Option<Texture<'static>> {
        if self.error.is_some() {
            return None;
        }

        let result = self.renderer.prepare_readback(&rect);
        self.retain_error(result);
        if self.error.is_some() {
            return None;
        }

        match self.renderer.sample_target_region(&rect) {
            Ok(texture) => texture,
            Err(error) => {
                self.fail(error);
                None
            }
        }
    }

    pub(crate) fn draw_text_run(
        &mut self,
        command: &DrawCommand<'_>,
        clip: Rect,
        identity: TextRunIdentity,
    ) {
        if self.error.is_some() {
            return;
        }
        let Some(clip) = self.clip.intersect(&clip) else {
            return;
        };
        let request = DrawRequest::new(command, clip);
        let result = self.renderer.poll_draw_request(
            request,
            Some(identity),
            &mut Context::from_waker(self.waker),
        );
        match result {
            Poll::Pending => {
                self.resource_pending = true;
                self.fail(RenderError::ResourcePending);
            }
            Poll::Ready(result) => self.retain_error(result),
        }
    }

    pub(crate) fn fail(&mut self, error: RenderError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn error(&self) -> Option<RenderError> {
        self.error
    }

    fn retain_error(&mut self, result: Result<(), RenderError>) {
        if let Err(error) = result {
            self.fail(error);
        }
    }
}

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

#[derive(Default)]
pub struct ViewRegistry {
    views: Vec<View>,
    input_attach_plan: Option<Rc<Vec<crate::ui::lifecycle::InputAttach>>>,
    paint_order: Vec<usize>,
    next_registration: u64,
    bindings: Vec<ViewBinding>,
    owner: Option<crate::ecs::world::WorldId>,
    output_registration: Option<u64>,
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
        super::lifecycle::prepare(world);
        let registered = world.with_resource_box::<Self, _>(|registry, world| {
            let owner = world.id();
            match registry.owner {
                None => registry.owner = Some(owner),
                Some(registered) => {
                    assert_eq!(registered, owner, "ViewRegistry belongs to a different App")
                }
            }
            if world.resource::<OutputRelations>().is_none()
                && registry
                    .views
                    .iter()
                    .any(|view| !view.definition.output_dependencies.is_empty())
            {
                world.insert_resource(OutputRelations::default());
            }
            let tracking_changed = registry.prepare_tracking(world);
            if world.resource::<OutputRelations>().is_some() {
                world.with_resource_box::<OutputRelations, _>(|relations, world| {
                    let activated = relations.activate(world, &mut registry.output_registration);
                    if activated {
                        super::render_system::output_plan::invalidate_measurements(world);
                        for view in &registry.views {
                            if !view.definition.output_dependencies.is_empty() {
                                world.watch_component_type(
                                    view.component_filter().expect("output component filter"),
                                );
                            }
                        }
                    }
                    if tracking_changed || activated {
                        for view in &registry.views {
                            for dependency in view.definition.output_dependencies {
                                relations.register(&dependency.descriptor);
                            }
                        }
                    }
                    world.drain_component_changes(|world, change| {
                        registry.apply_change(world, change, Some(relations));
                    });
                    registry.invalidate_pending_queries(world);
                    relations.reconcile(world);
                });
            } else {
                world.drain_component_changes(|world, change| {
                    registry.apply_change(world, change, None);
                });
                registry.invalidate_pending_queries(world);
            }
        });
        if registered.is_none() {
            super::render_system::output_plan::invalidate_measurements(world);
            world.with_resource_box::<OutputRelations, _>(|relations, world| {
                relations.clear(world);
                relations.reconcile(world);
            });
        }
    }

    pub(crate) fn invalidate_queries(world: &mut World) {
        Self::reconcile_observations(world);
    }

    fn prepare_tracking(&mut self, world: &mut World) -> bool {
        // Views are appended and prepared in registration order; the ready prefix is contiguous.
        if self
            .views
            .last()
            .is_none_or(|view| view.definition.component_tracking_ready)
        {
            return false;
        }
        super::render_system::output_plan::invalidate_measurements(world);
        for view in self
            .views
            .iter_mut()
            .filter(|view| !view.definition.component_tracking_ready)
        {
            for dependency in view.definition.output_dependencies {
                (dependency.descriptor.install)(world);
            }
            if let Some(type_id) = view.component_filter() {
                world.watch_component_type(type_id);
            }
            for dependency in view.definition.query_dependencies {
                world.watch_query_dependency_type(dependency());
                if view.component_filter() != Some(dependency()) {
                    world.watch_component_type(dependency());
                }
            }
            if !view.definition.query_dependencies.is_empty() {
                Self::invalidate_instances(world, view);
            }
            view.definition.component_tracking_ready = true;
        }
        true
    }

    /// Invalidate readers of a published component without re-running attachment or output preparation.
    pub(crate) fn component_published<T: crate::ecs::Component>(world: &mut World, entity: Entity) {
        let changed = TypeId::of::<T>();
        world.with_resource_box::<Self, _>(|registry, world| {
            for view in &registry.views {
                let owns_component = view.component_filter() == Some(changed);
                if !owns_component
                    && !view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|kind| kind() == changed)
                {
                    continue;
                }
                if !owns_component && view.definition.query_scope.get() != QueryScope::Local {
                    Self::invalidate_instances(world, view);
                } else if owns_component
                    || view
                        .component_filter()
                        .is_some_and(|kind| world.has_type(entity, kind))
                {
                    match view.invalidation() {
                        ViewInvalidation::Visual => world.invalidate_visual(entity),
                        ViewInvalidation::Layout => world.invalidate(entity),
                    }
                }
            }
        });
    }

    /// Mark consumers of newly published results without invalidating prepared producers.
    pub(crate) fn propagate_output_publications(world: &mut World) {
        world.with_resource_box::<OutputRelations, _>(|relations, world| {
            relations.propagate_publications(world);
        });
    }

    fn invalidate_pending_queries(&self, world: &mut World) {
        world.drain_query_dependency_changes(|world, changed, cause| {
            for view in &self.views {
                if (cause == crate::ecs::world::QueryDependencyCause::Source
                    || view.definition.query_scope.get() != QueryScope::Local)
                    && view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|dependency| dependency() == changed)
                {
                    Self::invalidate_instances(world, view);
                }
            }
        });
    }

    fn invalidate_instances(world: &mut World, view: &View) {
        if let Some(type_id) = view.component_filter() {
            world.for_each_stable_type(type_id, |world, entity| match view.invalidation() {
                ViewInvalidation::Visual => world.invalidate_visual(entity),
                ViewInvalidation::Layout => world.invalidate(entity),
            });
        }
    }

    fn apply_change(
        &mut self,
        world: &mut World,
        change: crate::ecs::world::ComponentChange,
        mut relations: Option<&mut OutputRelations>,
    ) {
        for view in &self.views {
            if view.component_filter() != Some(change.type_id) {
                if view.definition.query_scope.get() == QueryScope::Local
                    && view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|kind| kind() == change.type_id)
                    && world.is_alive(change.entity)
                    && view
                        .component_filter()
                        .is_some_and(|kind| world.has_type(change.entity, kind))
                {
                    match view.invalidation() {
                        ViewInvalidation::Visual => world.invalidate_visual(change.entity),
                        ViewInvalidation::Layout => world.invalidate(change.entity),
                    }
                }
                continue;
            }
            let invalidation = Self::invalidation_for_change(view, change);
            if let Some(relations) = relations.as_deref_mut() {
                if world.is_alive(change.entity) && world.has_type(change.entity, change.type_id) {
                    for (slot, dependency) in view.definition.output_dependencies.iter().enumerate()
                    {
                        relations.bind(
                            world,
                            change.entity,
                            view.registration_id(),
                            slot,
                            dependency,
                            invalidation,
                        );
                    }
                } else {
                    relations.remove(change.entity, view.registration_id());
                }
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
                    .reset(invalidation, view.definition.observation_count);
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
                match invalidation {
                    ViewInvalidation::Visual => world.invalidate_visual(change.entity),
                    ViewInvalidation::Layout => world.invalidate(change.entity),
                }
            }
        }
    }

    fn invalidation_for_change(
        view: &View,
        change: crate::ecs::world::ComponentChange,
    ) -> ViewInvalidation {
        if view.component_filter() != Some(TypeId::of::<Style>()) {
            return view.invalidation();
        }
        match change.kind {
            crate::ecs::world::ComponentChangeKind::Visual => ViewInvalidation::Visual,
            crate::ecs::world::ComponentChangeKind::Layout => ViewInvalidation::Layout,
            crate::ecs::world::ComponentChangeKind::Transform => ViewInvalidation::Visual,
            crate::ecs::world::ComponentChangeKind::Component => view.invalidation(),
        }
    }

    /// Pre-populated registry containing every built-in widget kind.
    /// `App::with_default_widgets` iterates this to wire each widget's
    /// `View::systems` into the scheduler before storing the registry
    /// as a `World` resource.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        reg.insert(super::children_view::view());
        reg.insert(super::style_view::view());
        reg.insert(crate::ui::widgets::button::view());
        reg.insert(crate::ui::widgets::checkbox::view());
        reg.insert(crate::ui::widgets::progress_bar::view());
        reg.insert(crate::ui::widgets::tabbar::view());
        reg.insert(crate::ui::widgets::text_input::view());
        reg.insert(crate::ui::widgets::text_editing::view());
        reg.insert(crate::ui::widgets::image::view());
        reg.insert(crate::ui::widgets::text::view());
        reg.insert(crate::ui::widgets::text::static_glyph_run_view());
        reg.insert(crate::ui::widgets::slider::view());
        reg.insert(crate::ui::widgets::switch::view());
        reg.insert(crate::ui::widgets::tab_pages::view());
        reg.insert(crate::ui::widgets::lazy_list::view());
        reg.insert(crate::ui::widgets::follow_bounds::view());
        reg.insert(crate::ui::widgets::mirror::view());
        reg.insert(crate::ui::widgets::temporal_mix::view());
        reg.insert(crate::ui::widgets::background_blur::view());
        reg.insert(crate::ui::widgets::drop_shadow::view());
        reg.insert(crate::ui::widgets::drop_shadow::glow_view());
        reg.insert(crate::ui::widgets::icon::view());
        reg
    }

    /// Add a View descriptor in registration order and index its optional
    /// paint hook independently by paint priority.
    pub fn insert(&mut self, mut view: View) {
        assert!(
            view.definition.query_dependencies.is_empty() || view.component_filter().is_some(),
            "queried View requires a component filter"
        );
        if view.observation().is_some() {
            assert!(
                view.component_filter().is_some(),
                "observed View requires a component filter"
            );
        }
        view.definition.registration_id = Some(self.next_registration);
        view.definition.component_tracking_ready = false;
        self.next_registration = self
            .next_registration
            .checked_add(1)
            .expect("View registration identity exhausted");
        let index = self.views.len();
        let paint_priority = view.priority();
        if let Some(hook) = crate::ui::lifecycle::InputAttach::for_view(&view) {
            let plan = self
                .input_attach_plan
                .get_or_insert_with(|| Rc::new(Vec::new()));
            Rc::make_mut(plan).push(hook);
        }
        self.views.push(view);
        if let Some(priority) = paint_priority {
            let pos = self
                .paint_order
                .iter()
                .position(|&index| self.views[index].priority().is_some_and(|p| p > priority))
                .unwrap_or(self.paint_order.len());
            self.paint_order.insert(pos, index);
        }
    }

    /// Iterate all descriptors in registration order, including those without paint.
    pub fn iter(&self) -> impl Iterator<Item = &View> {
        self.views.iter()
    }

    pub(crate) fn input_attach_plan(&self) -> Option<Rc<Vec<crate::ui::lifecycle::InputAttach>>> {
        self.input_attach_plan.as_ref().map(Rc::clone)
    }

    pub(crate) fn input_bindings(
        &self,
    ) -> impl Iterator<
        Item = (
            &Option<TypeId>,
            &crate::input::event::view_input::SharedInputs,
        ),
    > {
        self.input_attach_plan
            .as_deref()
            .map_or(&[][..], |plan| plan.as_slice())
            .iter()
            .filter_map(crate::ui::lifecycle::InputAttach::inputs)
    }

    pub(crate) fn paint_iter(&self) -> impl ExactSizeIterator<Item = &View> {
        self.paint_order.iter().map(|&index| &self.views[index])
    }

    pub(crate) fn paint_at(&self, position: usize) -> Option<&View> {
        self.paint_order
            .get(position)
            .map(|&index| &self.views[index])
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

    #[test]
    fn input_plan_retains_registration_order_and_shared_state() {
        fn check(registry: &ViewRegistry) {
            let mut expected = registry.iter().filter_map(|view| {
                view.inputs()
                    .map(|inputs| (view.component_filter(), inputs))
            });
            for (filter, inputs) in registry.input_bindings() {
                let (expected_filter, expected_inputs) = expected.next().unwrap();
                assert_eq!(*filter, expected_filter);
                assert!(Rc::ptr_eq(inputs, expected_inputs));
            }
            assert!(expected.next().is_none());
        }
        let mut registry = ViewRegistry::with_builtins();
        check(&registry);
        assert_eq!(tracked_allocations(|| check(&registry)), 0);
        let retained_plan = registry.input_attach_plan().unwrap();
        let retained_len = retained_plan.len();
        registry.insert(crate::ui::widgets::button::view());
        check(&registry);
        assert_eq!(retained_plan.len(), retained_len);
        assert_eq!(
            registry.input_attach_plan().unwrap().len(),
            retained_len + 1
        );
        check(&ViewRegistry::default());
    }
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
    struct BehaviorOnlyTile;

    struct BehaviorTicks(u32);

    #[crate::system(order = Normal, expect = [BehaviorOnlyTile])]
    fn tick_behavior(ticks: res!(mut BehaviorTicks)) {
        ticks.0 += 1;
    }

    #[crate::view(component = BehaviorOnlyTile, name = "BehaviorOnly", systems = [tick_behavior::system()])]
    fn behavior_only_view() {}

    #[test]
    fn empty_view_installs_its_system_only_when_registered_without_paint() {
        let mut app = crate::app::App::headless(32, 32);
        app.world.insert_resource(BehaviorTicks(0));
        let node = app.world.spawn_empty();
        app.world.insert(node, BehaviorOnlyTile);
        app.systems.run_all(&mut app.world).unwrap();
        assert_eq!(app.world.resource::<BehaviorTicks>().unwrap().0, 0);
        assert!(
            !app.systems
                .iter()
                .any(|system| system.name == "tick_behavior")
        );

        let view = behavior_only_view::view();
        assert_eq!(view.name(), "BehaviorOnly");
        assert_eq!(
            view.component_filter().unwrap(),
            TypeId::of::<BehaviorOnlyTile>()
        );
        assert!(view.render().is_none());
        assert_eq!(view.priority(), None);
        app.with_widget(view);
        let registry = app.world.resource::<ViewRegistry>().unwrap();
        assert!(registry.iter().any(|view| view.name() == "BehaviorOnly"));
        assert!(
            !registry
                .paint_iter()
                .any(|view| view.name() == "BehaviorOnly")
        );
        assert_eq!(
            app.systems
                .iter()
                .filter(|system| system.name == "tick_behavior")
                .count(),
            1
        );
        for expected in [1, 2] {
            assert_eq!(
                tracked_allocations(|| app.systems.run_all(&mut app.world).unwrap()),
                0
            );
            assert_eq!(app.world.resource::<BehaviorTicks>().unwrap().0, expected);
        }
        app.world.remove::<BehaviorOnlyTile>(node);
        assert_eq!(
            tracked_allocations(|| app.systems.run_all(&mut app.world).unwrap()),
            0
        );
        assert_eq!(app.world.resource::<BehaviorTicks>().unwrap().0, 2);
    }

    #[test]
    fn builtin_behavior_views_preserve_system_slots_and_expect_without_paint() {
        use crate::ecs::SystemSlot;
        for (view, name, component, slot) in [
            (
                crate::ui::widgets::tab_pages::view(),
                "TabPages",
                TypeId::of::<crate::ui::widgets::tab_pages::TabContent>(),
                SystemSlot::TabPages,
            ),
            (
                crate::ui::widgets::lazy_list::view(),
                "LazyList",
                TypeId::of::<crate::ui::widgets::lazy_list::LazyList>(),
                SystemSlot::LazyList,
            ),
        ] {
            assert_eq!(view.name(), name);
            assert_eq!(view.component_filter().unwrap(), component);
            assert!(view.render().is_none());
            assert_eq!(view.priority(), None);
            let mut app = crate::app::App::headless(32, 32);
            assert!(
                !app.systems
                    .iter()
                    .any(|system| system.priority() == slot.priority())
            );
            app.with_widget(view);
            let mut matching = app
                .systems
                .iter()
                .filter(|system| system.priority() == slot.priority());
            let system = matching.next().unwrap();
            assert!(!system.metadata().is_empty());
            assert!(matching.next().is_none());
        }
    }

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

    struct OptionalPaintResource(u32);

    #[crate::component]
    struct ResourceTile {
        seen: Cell<Option<u32>>,
        visits: Cell<u32>,
    }

    #[crate::view(component = ResourceTile)]
    fn paint_resource_tile(
        tile: component!(),
        value: res!(optional self::OptionalPaintResource),
        theme: res!(crate::ui::Theme),
        paint: paint!(),
    ) {
        tile.seen.set(value.as_deref().map(|value| value.0));
        tile.visits.set(tile.visits.get() + 1);
        let _ = theme;
        paint.set_background_handled(value.is_some());
    }

    #[test]
    fn optional_resource_paint_refetches_borrowed_values_without_allocating() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }
        let mut world = World::new();
        world.insert_resource(crate::ui::Theme::default());
        let entity = world.spawn_empty();
        world.insert(
            entity,
            ResourceTile {
                seen: Cell::new(None),
                visits: Cell::new(0),
            },
        );
        let render = paint_resource_tile::view().render().unwrap();
        let style = Style::default();
        let rect = Rect::new(0, 0, 16, 16);
        let mut renderer = StubRenderer;
        let mut visits = 0;
        for value in [None, Some(7), Some(11), None] {
            if let Some(value) = value {
                world.insert_resource(OptionalPaintResource(value));
            } else {
                world.remove_resource::<OptionalPaintResource>();
            }
            for _ in 0..2 {
                let mut paint = PaintCtx::new(
                    &mut renderer,
                    &style,
                    &rect,
                    &rect,
                    Transform::IDENTITY,
                    None,
                    WidgetState::Enabled,
                );
                assert_eq!(
                    tracked_allocations(|| render(&world, entity, &mut paint)),
                    0
                );
                visits += 1;
                let tile = world.get::<ResourceTile>(entity).unwrap();
                assert_eq!(tile.seen.get(), value);
                assert_eq!(tile.visits.get(), visits);
                assert_eq!(paint.background_handled(), value.is_some());
                assert_eq!(paint.error(), None);
            }
        }
        world.remove_resource::<crate::ui::Theme>();
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render(&world, entity, &mut paint);
            }))
            .is_err()
        );
        assert_eq!(
            world.get::<ResourceTile>(entity).unwrap().visits.get(),
            visits
        );
    }

    #[cfg(feature = "gallery")]
    #[test]
    fn gallery_optional_resource_views_preserve_allocation_free_descriptors() {
        use crate::gallery::demos::{
            butterfly, clip_path, fill_rules, shapes, stroke_styles, vector_mandala,
        };
        for (factory, component, name) in [
            (
                shapes::shapes_view as fn() -> View,
                TypeId::of::<shapes::Shapes>(),
                "Shapes",
            ),
            (
                butterfly::butterfly_view,
                TypeId::of::<butterfly::Butterfly>(),
                "Butterfly",
            ),
            (
                vector_mandala::vector_mandala_view,
                TypeId::of::<vector_mandala::VectorMandala>(),
                "VectorMandala",
            ),
            (
                fill_rules::fill_rules_view,
                TypeId::of::<fill_rules::FillRules>(),
                "FillRules",
            ),
            (
                clip_path::clip_path_view,
                TypeId::of::<clip_path::ClipPath>(),
                "ClipPath",
            ),
            (
                stroke_styles::stroke_styles_view,
                TypeId::of::<stroke_styles::StrokeStyles>(),
                "StrokeStyles",
            ),
        ] {
            for _ in 0..2 {
                let mut view = None;
                assert_eq!(tracked_allocations(|| view = Some(factory())), 0);
                let view = view.unwrap();
                assert_eq!(view.name(), name);
                assert_eq!(view.component_filter(), Some(component));
                assert_eq!(view.priority(), Some(60));
                assert_eq!(view.invalidation(), ViewInvalidation::Visual);
                assert!(view.render().is_some());
            }
        }
    }

    #[cfg(feature = "gallery")]
    #[test]
    fn gallery_paint_refetches_theme_and_optional_clock_without_allocating() {
        use crate::ecs::MonoClock;
        use crate::gallery::demos::{butterfly, clip_path, fill_rules, shapes, stroke_styles};
        use crate::render::canvas::Paint;
        use crate::types::Color;
        use crate::ui::{ColorToken, Theme};

        #[derive(Default)]
        struct Trace {
            color: Option<Color>,
            motion: Option<(Transform, Point)>,
            calls: usize,
        }
        impl Renderer for Trace {
            type TargetState = ();

            fn route(
                &self,
                request: &DrawRequest<'_, '_>,
            ) -> Result<crate::render::RenderRoute, RenderError> {
                request.validate()?;
                Ok(crate::render::RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                assert_eq!(request.clip, Rect::new(13, 17, 460, 340));
                match *request.command {
                    DrawCommand::Arc { color, .. } | DrawCommand::Fill { color, .. } => {
                        self.color.get_or_insert(color);
                    }
                    DrawCommand::Line { transform, p2, .. } => {
                        self.motion.get_or_insert((transform, p2));
                    }
                    DrawCommand::FillPath {
                        transform,
                        paint: Paint::Color(color),
                        ..
                    }
                    | DrawCommand::StrokePath {
                        transform,
                        paint: Paint::Color(color),
                        ..
                    } => {
                        self.color.get_or_insert((*color).into());
                        self.motion.get_or_insert((
                            transform,
                            Point {
                                x: Fixed::ZERO,
                                y: Fixed::ZERO,
                            },
                        ));
                    }
                    DrawCommand::PushClip { .. } | DrawCommand::PopClip => {}
                    _ => panic!("unexpected gallery paint request"),
                }
                self.calls += 1;
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut world = World::new();
        let shapes_node = world.spawn_empty();
        let butterfly_node = world.spawn_empty();
        let fill_rules_node = world.spawn_empty();
        let clip_path_node = world.spawn_empty();
        let stroke_styles_node = world.spawn_empty();
        world.insert(shapes_node, shapes::Shapes::default());
        world.insert(butterfly_node, butterfly::Butterfly::default());
        world.insert(fill_rules_node, fill_rules::FillRules);
        world.insert(clip_path_node, clip_path::ClipPath);
        world.insert(stroke_styles_node, stroke_styles::StrokeStyles);
        let fixtures = [
            (
                shapes::shapes_view().render().unwrap(),
                shapes_node,
                ColorToken::Primary,
                true,
            ),
            (
                butterfly::butterfly_view().render().unwrap(),
                butterfly_node,
                ColorToken::Primary,
                true,
            ),
            (
                fill_rules::fill_rules_view().render().unwrap(),
                fill_rules_node,
                ColorToken::Secondary,
                false,
            ),
            (
                clip_path::clip_path_view().render().unwrap(),
                clip_path_node,
                ColorToken::SurfaceVariant,
                false,
            ),
            (
                stroke_styles::stroke_styles_view().render().unwrap(),
                stroke_styles_node,
                ColorToken::Primary,
                false,
            ),
        ];
        let style = Style::default();
        let rect = Rect::new(7, 11, 480, 360);
        let clip = Rect::new(13, 17, 460, 340);
        let transform = Transform {
            m00: Fixed::from_f32(1.25),
            m11: Fixed::from_f32(0.75),
            tx: Fixed::from_int(9),
            ty: Fixed::from_int(13),
            ..Transform::IDENTITY
        };
        let mut initial = [None; 5];
        let mut previous = [None; 5];
        for stage in 0..4 {
            let mut current = [None; 5];
            let theme = match stage {
                1 => Theme::light(),
                _ => Theme::default(),
            };
            let colors = fixtures.map(|(_, _, token, _)| theme.resolve(token));
            world.insert_resource(theme);
            if stage == 1 || stage == 2 {
                world.insert_resource(MonoClock::new(if stage == 1 {
                    || 1_337_000_000
                } else {
                    || 2_000_000_000
                }));
            } else {
                world.remove_resource::<MonoClock>();
            }
            for (i, (render, entity, _, animated)) in fixtures.into_iter().enumerate() {
                for _ in 0..2 {
                    let mut trace = Trace::default();
                    let mut paint = PaintCtx::new(
                        &mut trace,
                        &style,
                        &rect,
                        &clip,
                        transform,
                        None,
                        WidgetState::Enabled,
                    );
                    assert_eq!(
                        tracked_allocations(|| render(&world, entity, &mut paint)),
                        0
                    );
                    assert_eq!(paint.error(), None);
                    assert!(trace.calls > 0);
                    assert_eq!(trace.color, Some(colors[i]));
                    if stage == 0 {
                        initial[i] = trace.motion;
                    }
                    if animated && (stage == 1 || stage == 2) {
                        assert_ne!(trace.motion, previous[i]);
                    }
                    if stage == 3 {
                        assert_eq!(trace.motion, initial[i]);
                    }
                    current[i] = trace.motion;
                }
            }
            previous = current;
        }
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

    #[crate::view(component = BoundTile, priority = 62)]
    fn paint_bound_tile(model: model!(component.model, watch(value)), paint: paint!()) {
        paint.set_background_handled(model.value > 0);
    }

    #[crate::view(component = PairTile)]
    fn paint_pair_tile(
        first: model!(component.first, watch(value)),
        second: model!(component.second, watch(value)),
        paint: paint!(),
    ) {
        paint.set_background_handled(first.value < second.value);
    }

    #[crate::view(component = OptionalBoundTile)]
    fn paint_optional_bound_tile(
        component_data: component!(),
        model: model!(optional component.model, watch(value)),
        paint: paint!(),
    ) {
        component_data.visits.set(component_data.visits.get() + 1);
        component_data.seen.set(model.map(|model| model.value));
        paint.set_background_handled(model.is_some());
    }

    #[crate::view(component = VisualTile)]
    fn paint_visual_tile(
        _visual: model!(component.visual, watch(visual_revision)),
        paint: paint!(),
    ) {
        paint.set_background_handled(true);
    }

    #[crate::view(component = GenericTile<T>)]
    fn paint_generic_tile<T: Copy + 'static>(component: component!(), paint: paint!()) {
        let _ = component.value;
        paint.set_background_handled(true);
    }

    #[crate::view(component = BoundTile)]
    fn invalid_writing_view(component: component!()) {
        component.model.set(7);
    }

    #[crate::event]
    fn options_attach() {}

    #[crate::event]
    fn options_gesture(_tap: event!(crate::input::event::gesture::Tap)) -> bool {
        false
    }

    fn options_system(_: &mut World) -> Result<(), crate::ecs::CommandError> {
        Ok(())
    }

    #[crate::view(
        component = GenericTile<u8>,
        priority = 42,
        attach = options_attach::callback(),
        input = options_gesture::callback(),
        systems = [crate::ecs::System::new("options", 100, options_system)]
    )]
    fn paint_options(component: component!(), theme: res!(crate::ui::Theme)) {
        let _ = (component.value, theme);
    }

    #[test]
    fn unresolved_text_ink_is_a_capability_error() {
        assert_eq!(
            PaintCtx::replay_error(crate::render::scene::replay::ReplayError::Bounds(
                crate::render::scene::bbox::BoundsError::GlyphInk,
            )),
            RenderError::Unsupported(crate::render::renderer::RenderFeature::TextInkBounds)
        );
    }

    fn dummy_render(_world: &World, _entity: Entity, _ctx: &mut PaintCtx<'_>) {}

    fn make_view(name: &'static str, priority: u8) -> View {
        View::new(name, priority, dummy_render)
    }

    mod output_contracts {
        use super::*;
        use crate::render::texture::ColorFormat;
        use crate::ui::offscreen::{BufferKey, OffscreenRender, WidgetTexture};
        use crate::ui::outputs::Outputs;
        use crate::ui::widgets::WidgetTransform;
        use crate::ui::{Hidden, Parent};

        struct Sources {
            first: Entity,
            second: Option<Entity>,
        }

        impl crate::ecs::Component for Sources {
            const OUTPUT_DEPENDENCIES: &'static [OutputDependency] = &[
                OutputDependency::current::<WidgetTexture>(|world, owner| {
                    Some(world.get::<Self>(owner)?.first)
                }),
                OutputDependency::current::<WidgetTexture>(|world, owner| {
                    world.get::<Self>(owner)?.second
                }),
            ];
        }

        struct OtherSource(Entity);

        impl crate::ecs::Component for OtherSource {
            const OUTPUT_DEPENDENCIES: &'static [OutputDependency] =
                &[OutputDependency::current::<WidgetTexture>(
                    |world, owner| Some(world.get::<Self>(owner)?.0),
                )];
        }

        struct RetainedSource(Entity);

        impl crate::ecs::Component for RetainedSource {
            const OUTPUT_DEPENDENCIES: &'static [OutputDependency] = &[
                OutputDependency::current::<WidgetTexture>(|world, owner| {
                    Some(world.get::<Self>(owner)?.0)
                }),
                OutputDependency::previous::<WidgetTexture>(),
            ];
        }

        struct OwnerHistory;

        impl crate::ecs::Component for OwnerHistory {
            const OUTPUT_DEPENDENCIES: &'static [OutputDependency] =
                &[OutputDependency::previous::<WidgetTexture>()];
        }

        struct OtherOwnerHistory;

        impl crate::ecs::Component for OtherOwnerHistory {
            const OUTPUT_DEPENDENCIES: &'static [OutputDependency] =
                &[OutputDependency::previous::<WidgetTexture>()];
        }

        fn registry<C: crate::ecs::Component>() -> ViewRegistry {
            let mut registry = ViewRegistry::default();
            registry.insert(make_view("output-reader", 60).with_filter::<C>());
            registry
        }

        fn requested(world: &World, entity: Entity) -> bool {
            world
                .resource::<Outputs<WidgetTexture>>()
                .is_some_and(|outputs| outputs.is_requested(entity))
        }

        fn clear_dirty(world: &mut World, entities: &[Entity]) {
            for &entity in entities {
                world.remove::<Dirty>(entity);
                world.remove::<VisualDirty>(entity);
            }
        }

        fn publish(world: &World, entity: Entity, generation: u32) {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            let _batch = outputs.begin_batch();
            let write = outputs
                .prepare_test_texture(BufferKey {
                    entity,
                    w: 2,
                    h: 2,
                    format: ColorFormat::RGBA8888,
                    generation,
                    layout_rect: Rect::new(0, 0, 2, 2),
                    local_bounds: Rect::new(0, 0, 2, 2),
                    scale: Fixed::ONE,
                })
                .expect("output write");
            assert!(write.publish());
        }

        #[test]
        fn output_declarations_are_cold_until_a_matching_view_is_registered() {
            let mut world = World::default();
            let source = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(
                owner,
                Sources {
                    first: source,
                    second: None,
                },
            );
            let mut views = ViewRegistry::default();
            views.insert(make_view("unrelated", 60).with_filter::<VisualOnlyTile>());
            world.insert_resource(views);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(world.resource::<Outputs<WidgetTexture>>().is_none());
            assert!(world.resource::<OutputRelations>().is_none());

            world
                .resource_mut::<ViewRegistry>()
                .unwrap()
                .insert(View::systems_only("late-reader", &[]).with_filter::<Sources>());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(requested(&world, source));
            assert!(!world.has::<OffscreenRender>(source));
        }

        #[test]
        fn late_output_kinds_install_once_and_reconcile_each_pass() {
            #[derive(Default)]
            struct Calls {
                installed: [usize; 2],
                reconciled: [Cell<usize>; 2],
            }
            struct Probe<const N: usize>;
            impl<const N: usize> crate::ui::outputs::private::OutputKind for Probe<N> {
                fn install(world: &mut World) {
                    world.resource_mut::<Calls>().unwrap().installed[N] += 1;
                }
                fn reconcile(world: &World) {
                    let calls = &world.resource::<Calls>().unwrap().reconciled[N];
                    calls.set(calls.get() + 1);
                }
            }
            impl<const N: usize> crate::ui::outputs::Output for Probe<N> {
                type Storage = ();
                type Record = ();
                type Read<'a> = ();
                fn read<'a>(_: core::cell::Ref<'a, ()>, _: ()) -> Option<()> {
                    Some(())
                }
            }
            impl<const N: usize> crate::ecs::Component for Probe<N> {
                const OUTPUT_DEPENDENCIES: &'static [OutputDependency] =
                    &[OutputDependency::current::<Self>(|_, _| None)];
            }
            let mut world = World::default();
            world.insert_resource(Calls::default());
            world.insert_resource(registry::<Probe<0>>());
            ViewRegistry::reconcile_observations(&mut world);
            ViewRegistry::reconcile_observations(&mut world);
            world
                .resource_mut::<ViewRegistry>()
                .unwrap()
                .insert(make_view("late output", 0).with_filter::<Probe<1>>());
            ViewRegistry::reconcile_observations(&mut world);
            ViewRegistry::reconcile_observations(&mut world);
            let calls = world.resource::<Calls>().unwrap();
            assert_eq!(calls.installed, [1, 1]);
            assert_eq!(calls.reconciled.each_ref().map(Cell::get), [4, 2]);
            world.remove_resource::<OutputRelations>();
            ViewRegistry::reconcile_observations(&mut world);
            let calls = world.resource::<Calls>().unwrap();
            assert_eq!(calls.installed, [1, 1]);
            assert_eq!(calls.reconciled.each_ref().map(Cell::get), [5, 3]);
        }

        #[test]
        fn output_registration_survives_relation_storage_replacement() {
            let mut world = World::default();
            let source = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(owner, OtherSource(source));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(requested(&world, source));

            for _ in 0..3 {
                world.remove_resource::<OutputRelations>();
                ViewRegistry::reconcile_observations(&mut world);
                assert!(requested(&world, source));
                world.remove::<OtherSource>(owner);
                ViewRegistry::reconcile_observations(&mut world);
                assert!(!requested(&world, source));
                world.insert(owner, OtherSource(source));
                ViewRegistry::reconcile_observations(&mut world);
                assert!(requested(&world, source));
            }
            assert_eq!(
                tracked_allocations(|| ViewRegistry::reconcile_observations(&mut world)),
                0,
            );
        }

        #[test]
        fn output_publications_use_the_registered_views_invalidation_policy() {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(1024));
            let source = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(owner, OtherSource(source));
            let mut views = ViewRegistry::default();
            views.insert(
                make_view("layout-output-reader", 60)
                    .with_filter::<OtherSource>()
                    .with_invalidation(ViewInvalidation::Layout),
            );
            world.insert_resource(views);
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[source, owner]);
            publish(&world, source, 0);
            ViewRegistry::propagate_output_publications(&mut world);
            assert!(world.has::<Dirty>(owner));
            assert!(!world.has::<Dirty>(source));
            assert!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(source)
                    .is_some()
            );
        }

        #[test]
        fn independent_slots_components_and_registrations_share_only_source_storage() {
            let mut world = World::default();
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let third = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(
                owner,
                Sources {
                    first,
                    second: Some(second),
                },
            );
            world.insert(owner, OtherSource(third));
            let transform = Transform::translate(Fixed::from_int(7), Fixed::from_int(11));
            world.insert(owner, WidgetTransform(transform));
            let explicit = OffscreenRender::default().with_opacity(73);
            world.insert(first, explicit);
            let mut views = registry::<Sources>();
            views.insert(make_view("another-registration", 61).with_filter::<Sources>());
            views.insert(make_view("another-component", 62).with_filter::<OtherSource>());
            world.insert_resource(views);
            ViewRegistry::reconcile_observations(&mut world);
            for source in [first, second, third] {
                assert!(requested(&world, source));
            }
            assert_eq!(world.get::<WidgetTransform>(owner).unwrap().0, transform);
            assert_eq!(world.get::<OffscreenRender>(first).unwrap().opacity, 73);

            world.remove::<Sources>(owner);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, first));
            assert!(!requested(&world, second));
            assert!(requested(&world, third));
            assert_eq!(world.get::<OffscreenRender>(first).unwrap().opacity, 73);
            assert_eq!(world.get::<WidgetTransform>(owner).unwrap().0, transform);
        }

        #[test]
        fn final_source_swaps_do_not_transiently_release_shared_outputs() {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(1024));
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let left = world.spawn_empty();
            let right = world.spawn_empty();
            world.insert(left, OtherSource(first));
            world.insert(right, OtherSource(second));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[first, second, left, right]);
            publish(&world, first, 0);
            publish(&world, second, 0);

            world.get_mut::<OtherSource>(left).unwrap().0 = second;
            world.get_mut::<OtherSource>(right).unwrap().0 = first;
            ViewRegistry::reconcile_observations(&mut world);
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.get(first).is_some());
            assert!(outputs.get(second).is_some());
        }

        #[test]
        fn source_subtree_changes_visibility_and_generation_invalidate_consumers() {
            let mut world = World::default();
            let source = world.spawn_empty();
            let child = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(child, Parent(source));
            world.insert(owner, OtherSource(source));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[source, child, owner]);
            world.invalidate_visual(child);
            ViewRegistry::invalidate_queries(&mut world);
            assert!(world.has::<VisualDirty>(owner));

            clear_dirty(&mut world, &[source, child, owner]);
            world.insert(source, Hidden);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(world.remove::<VisualDirty>(owner).is_some());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!world.has::<VisualDirty>(owner));

            world.remove::<Hidden>(source);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(world.remove::<VisualDirty>(owner).is_some());
            assert!(world.despawn(source));
            let replacement = world.spawn_empty();
            assert_eq!(replacement.id, source.id);
            assert_ne!(replacement.generation, source.generation);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(world.has::<VisualDirty>(owner));
            assert!(!requested(&world, source));
            assert!(!requested(&world, replacement));
        }

        #[test]
        fn source_identity_changes_reset_only_the_consumers_own_history() {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(1024));
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(owner, RetainedSource(first));
            world.insert_resource(registry::<RetainedSource>());
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[first, second, owner]);
            publish(&world, owner, 0);
            publish(&world, owner, 1);
            assert!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .previous(owner)
                    .is_some()
            );

            world.invalidate_visual(first);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .previous(owner)
                    .is_some()
            );
            world.get_mut::<RetainedSource>(owner).unwrap().0 = second;
            ViewRegistry::reconcile_observations(&mut world);
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.previous(owner).is_none());
            assert!(outputs.state(owner).unwrap().retained);
            assert!(!requested(&world, first));
            assert!(requested(&world, second));
        }

        #[test]
        fn publication_invalidates_current_consumers_only_once() {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(1024));
            let source = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(owner, OtherSource(source));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[source, owner]);
            publish(&world, source, 0);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(world.remove::<VisualDirty>(owner).is_some());
            world
                .resource::<crate::ui::outputs::relations::OutputRelations>()
                .unwrap()
                .acknowledge(&world, &[owner]);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!world.has::<VisualDirty>(owner));
        }

        #[test]
        fn history_requests_share_storage_without_creating_self_invalidation() {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(1024));
            let owner = world.spawn_empty();
            world.insert(owner, OwnerHistory);
            world.insert(owner, OtherOwnerHistory);
            let mut views = registry::<OwnerHistory>();
            views.insert(make_view("other-history", 61).with_filter::<OtherOwnerHistory>());
            world.insert_resource(views);
            ViewRegistry::reconcile_observations(&mut world);
            clear_dirty(&mut world, &[owner]);
            publish(&world, owner, 0);
            publish(&world, owner, 1);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!world.has::<VisualDirty>(owner));

            world.remove::<OwnerHistory>(owner);
            ViewRegistry::reconcile_observations(&mut world);
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.state(owner).unwrap().retained);
            assert!(outputs.previous(owner).is_some());
            world.remove::<OtherOwnerHistory>(owner);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, owner));
        }

        #[test]
        fn replacing_removing_and_reusing_registries_reconciles_old_demands() {
            let mut world = World::default();
            let first = world.spawn_empty();
            let second = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(
                owner,
                Sources {
                    first,
                    second: None,
                },
            );
            world.insert(owner, OtherSource(second));
            world.insert_resource(registry::<Sources>());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(requested(&world, first));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, first));
            assert!(requested(&world, second));

            let views = world.remove_resource::<ViewRegistry>().unwrap();
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, second));
            world.insert_resource(views);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(requested(&world, second));
            clear_dirty(&mut world, &[first, second, owner]);
            world.insert_resource(ViewRegistry::default());
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, second));
            assert!(world.has::<VisualDirty>(owner));

            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            world.despawn(owner);
            ViewRegistry::reconcile_observations(&mut world);
            assert!(!requested(&world, second));
        }

        #[test]
        fn output_rebinding_reads_final_fields_without_steady_allocations() {
            let mut world = World::default();
            let first = world.spawn_empty();
            let intermediate = world.spawn_empty();
            let final_source = world.spawn_empty();
            let owner = world.spawn_empty();
            world.insert(owner, OtherSource(first));
            world.insert_resource(registry::<OtherSource>());
            ViewRegistry::reconcile_observations(&mut world);
            for source in [intermediate, final_source, first] {
                world.get_mut::<OtherSource>(owner).unwrap().0 = source;
                ViewRegistry::reconcile_observations(&mut world);
            }
            clear_dirty(&mut world, &[first, intermediate, final_source, owner]);
            let allocations = tracked_allocations(|| {
                world.get_mut::<OtherSource>(owner).unwrap().0 = intermediate;
                world.get_mut::<OtherSource>(owner).unwrap().0 = final_source;
                ViewRegistry::reconcile_observations(&mut world);
            });
            assert_eq!(allocations, 0);
            assert!(!requested(&world, first));
            assert!(!requested(&world, intermediate));
            assert!(requested(&world, final_source));
        }
    }

    #[test]
    fn builtin_typed_views_preserve_descriptors_and_borrowed_paint_execution() {
        struct CountingRenderer(usize);
        impl Renderer for CountingRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                self.0 += 1;
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        fn check<T: crate::ecs::Component>(
            component: T,
            view: View,
            name: &'static str,
            priority: u8,
            gesture: bool,
            draws: usize,
        ) {
            assert_eq!(view.name(), name);
            assert_eq!(view.priority(), Some(priority));
            assert_eq!(view.component_filter(), Some(TypeId::of::<T>()));
            assert_eq!(view.inputs().is_some(), gesture);
            assert!(view.auto_attach().is_none());
            assert_eq!(view.invalidation(), ViewInvalidation::Visual);
            let mut world = World::new();
            world.insert_resource(crate::ui::Theme::default());
            let node = world.spawn_empty();
            world.insert(node, component);
            let render = view.render().unwrap();
            let style = Style::default();
            let rect = Rect::new(0, 0, 40, 20);
            let mut renderer = CountingRenderer(0);
            for _ in 0..2 {
                let mut paint = PaintCtx::new(
                    &mut renderer,
                    &style,
                    &rect,
                    &rect,
                    Transform::IDENTITY,
                    None,
                    WidgetState::Hovered,
                );
                assert_eq!(tracked_allocations(|| render(&world, node, &mut paint)), 0);
                assert_eq!(paint.error(), None);
            }
            assert_eq!(renderer.0, 2 * draws);
        }

        use crate::ui::widgets::{Checkbox, Icon, ProgressBar, TabBar};
        check(
            Checkbox::new(),
            crate::ui::widgets::checkbox::view(),
            "Checkbox",
            40,
            true,
            1,
        );
        let mut progress = ProgressBar::new();
        progress.value = 0.5;
        check(
            progress,
            crate::ui::widgets::progress_bar::view(),
            "ProgressBar",
            60,
            true,
            2,
        );
        check(
            TabBar::new(4),
            crate::ui::widgets::tabbar::view(),
            "TabBar",
            60,
            true,
            1,
        );
        check(
            Icon::new(Path::from_static(&[])),
            crate::ui::widgets::icon::view(),
            "Icon",
            70,
            false,
            1,
        );
    }

    #[test]
    fn children_changes_invalidate_only_the_modified_parent_without_paint() {
        use crate::ui::Children;

        let mut app = crate::app::App::headless(32, 32);
        assert!(
            !app.world
                .resource::<ViewRegistry>()
                .unwrap()
                .iter()
                .any(|view| view.name() == "Children")
        );
        app.with_default_widgets();
        let registry = app.world.resource::<ViewRegistry>().unwrap();
        let descriptor = registry
            .iter()
            .find(|view| view.name() == "Children")
            .unwrap();
        assert_eq!(
            descriptor.component_filter(),
            Some(TypeId::of::<Children>())
        );
        assert_eq!(descriptor.invalidation(), ViewInvalidation::Layout);
        assert!(descriptor.render().is_none());
        assert_eq!(descriptor.priority(), None);
        assert!(!registry.paint_iter().any(|view| view.name() == "Children"));

        let parent = app.world.spawn_empty();
        let sibling = app.world.spawn_empty();
        let child = app.world.spawn_empty();
        app.world.insert(parent, Children(Vec::new()));
        app.world.insert(sibling, Children(Vec::new()));
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<Dirty>(parent);
        app.world.remove::<Dirty>(sibling);

        for operation in 0..3 {
            match operation {
                0 => app.world.insert(parent, Children(alloc::vec![child])),
                1 => app.world.get_mut::<Children>(parent).unwrap().0.clear(),
                _ => {
                    app.world.remove::<Children>(parent).unwrap();
                }
            }
            ViewRegistry::reconcile_observations(&mut app.world);
            assert!(app.world.remove::<Dirty>(parent).is_some());
            for entity in [parent, sibling, child] {
                assert!(!app.world.has::<Dirty>(entity));
                assert!(!app.world.has::<VisualDirty>(entity));
            }
            ViewRegistry::reconcile_observations(&mut app.world);
            assert!(!app.world.has::<Dirty>(parent));
        }
    }

    #[test]
    fn children_reorder_layout_settles_without_repeated_invalidation() {
        use crate::types::{Dimension, Viewport};
        use crate::ui::{Children, ComputedRect, Style};

        let mut app = crate::app::App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let make_style = || {
            let mut style = Style::default();
            style.layout.width = Dimension::px(16);
            style.layout.height = Dimension::px(8);
            style
        };
        let (first, second) = app.compose(root, |tree| {
            (tree.spawn(make_style()), tree.spawn(make_style()))
        });
        app.render_dirty().unwrap();
        let first_slot = app.world.get::<ComputedRect>(first).unwrap().0;
        let second_slot = app.world.get::<ComputedRect>(second).unwrap().0;
        assert_ne!(first_slot, second_slot);

        app.world.get_mut::<Children>(root).unwrap().0.swap(0, 1);
        app.render_dirty().unwrap();
        assert_eq!(app.world.get::<ComputedRect>(second).unwrap().0, first_slot);
        assert_eq!(app.world.get::<ComputedRect>(first).unwrap().0, second_slot);

        for _ in 0..2 {
            let plan = crate::ui::render_system::collect_dirty_regions(
                &mut app.world,
                root,
                &Viewport::new(32, 32, Fixed::ONE),
            )
            .unwrap();
            assert!(plan.is_empty());
            for entity in [root, first, second] {
                assert!(!app.world.has::<Dirty>(entity));
                assert!(!app.world.has::<VisualDirty>(entity));
            }
        }
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
    fn query_dependencies_invalidate_only_matching_views_with_their_declared_policy() {
        #[crate::component]
        struct Source(u32);

        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(
            make_view("query-visual", 60)
                .with_filter::<VisualOnlyTile>()
                .with_query_dependencies(&[TypeId::of::<Source>]),
        );
        app.with_widget(
            make_view("query-layout", 60)
                .with_filter::<LayoutTile>()
                .with_query_dependencies(&[TypeId::of::<Source>])
                .with_invalidation(ViewInvalidation::Layout),
        );
        let visual = app.world.spawn_empty();
        let layout = app.world.spawn_empty();
        let unrelated = app.world.spawn_empty();
        let source = app.world.spawn_empty();
        app.world.insert(visual, VisualOnlyTile);
        app.world.insert(layout, LayoutTile);
        app.world.insert(source, Source(0));
        ViewRegistry::reconcile_observations(&mut app.world);
        app.world.remove::<VisualDirty>(visual);
        app.world.remove::<Dirty>(layout);

        for _ in 0..2 {
            assert_eq!(
                tracked_allocations(|| {
                    app.world.get_mut::<Source>(source).unwrap().0 += 1;
                    ViewRegistry::invalidate_queries(&mut app.world);
                }),
                0
            );
            assert!(app.world.remove::<VisualDirty>(visual).is_some());
            assert!(!app.world.has::<Dirty>(visual));
            assert!(app.world.remove::<Dirty>(layout).is_some());
            assert!(!app.world.has::<Dirty>(unrelated));
            assert!(!app.world.has::<VisualDirty>(unrelated));
            ViewRegistry::invalidate_queries(&mut app.world);
            assert!(!app.world.has::<VisualDirty>(visual));
            assert!(!app.world.has::<Dirty>(layout));
        }
    }

    #[test]
    fn late_query_registration_invalidates_existing_instances_without_observation_slots() {
        #[crate::component]
        struct Source;

        let mut app = crate::app::App::headless(32, 32);
        let entity = app.world.spawn_empty();
        app.world.insert(entity, VisualOnlyTile);
        app.with_widget(
            make_view("late-query", 60)
                .with_filter::<VisualOnlyTile>()
                .with_query_dependencies(&[TypeId::of::<Source>]),
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());
        assert!(
            app.world
                .resource::<ViewRegistry>()
                .unwrap()
                .bindings
                .is_empty()
        );
        ViewRegistry::reconcile_observations(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
        let source = app.world.spawn_empty();
        app.world.insert(source, Source);
        ViewRegistry::invalidate_queries(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn query_invalidation_rejects_a_registry_transferred_to_another_world() {
        let mut first = crate::app::App::headless(32, 32);
        let mut second = crate::app::App::headless(32, 32);
        first.with_widget(make_view("owned", 60));
        ViewRegistry::reconcile_observations(&mut first.world);
        let registry = first.world.remove_resource::<ViewRegistry>().unwrap();
        second.world.insert_resource(registry);
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ViewRegistry::invalidate_queries(&mut second.world);
        }));
        assert!(rejected.is_err());
        assert!(second.world.resource::<ViewRegistry>().is_some());
    }

    #[test]
    fn install_preserves_system_expect_tag() {
        struct Marker;
        impl crate::ecs::Component for Marker {}
        fn dummy_run(_: &mut World) -> Result<(), crate::ecs::CommandError> {
            Ok(())
        }

        const EXPECT: &[crate::__macro_support::SystemAccess] =
            &[crate::__macro_support::SystemAccess::expect_component::<
                Marker,
            >()];
        const SYSTEMS: &[crate::ecs::System] =
            &[unsafe { crate::ecs::System::new("tagged", 100, dummy_run).with_metadata(EXPECT) }];

        let view = View::new("V", 60, dummy_render).with_systems(SYSTEMS);
        let mut installed: Vec<crate::ecs::System> = Vec::new();
        let view = view.install(|s| installed.push(s));

        assert!(view.systems.is_empty());
        assert_eq!(installed.len(), 1);
        assert_eq!(
            installed[0].metadata().len(),
            1,
            "expect must survive install"
        );
    }

    #[test]
    fn system_sources_move_owned_values_and_copy_templates_without_allocating() {
        let mut template = tick_behavior::system();
        template.last_us = 2;
        template.total_us = 9;
        template.call_count = 4;
        let expected_metadata = template.metadata().as_ptr();
        let mut copied = None;
        assert_eq!(
            tracked_allocations(|| {
                let view = behavior_only_view::view()
                    .with_invalidation(ViewInvalidation::Layout)
                    .with_systems(core::slice::from_ref(&template))
                    .install(|system| copied = Some(system));
                assert!(view.systems.is_empty());
                assert!(view.render().is_none());
                assert_eq!(view.invalidation(), ViewInvalidation::Layout);
                assert_eq!(
                    view.component_filter(),
                    Some(TypeId::of::<BehaviorOnlyTile>())
                );
            }),
            0
        );
        let copied = copied.unwrap();
        assert_eq!(
            (copied.last_us, copied.total_us, copied.call_count),
            (0, 0, 0)
        );
        assert_eq!(copied.metadata().as_ptr(), expected_metadata);
        assert_eq!(copied.priority(), template.priority());

        let mut moved = None;
        assert_eq!(
            tracked_allocations(|| {
                let view = behavior_only_view::view()
                    .with_systems([template])
                    .with_invalidation(ViewInvalidation::Layout)
                    .install(|system| moved = Some(system));
                assert!(view.systems.is_empty());
                assert!(view.render().is_none());
                assert_eq!(view.invalidation(), ViewInvalidation::Layout);
            }),
            0
        );
        let moved = moved.unwrap();
        assert_eq!((moved.last_us, moved.total_us, moved.call_count), (2, 9, 4));
        assert_eq!(moved.metadata().as_ptr(), expected_metadata);
    }

    #[test]
    fn bound_source_replacement_and_installation_transfer_callback_ownership() {
        let mut app = crate::app::App::headless(8, 8);
        let model = app.add_model(BoundState { value: 0 }).finish();
        let retained = Rc::new(Cell::new(0));
        let make_system = || {
            let capture = retained.clone();
            crate::ecs::System::bound("retained", 100, &model, move |_| {
                capture.set(capture.get() + 1);
                Ok(())
            })
        };
        let replaced = behavior_only_view::view().with_systems([make_system()]);
        assert_eq!(Rc::strong_count(&retained), 2);
        let empty = replaced.with_systems([] as [crate::ecs::System; 0]);
        assert_eq!(Rc::strong_count(&retained), 1);
        drop(empty);

        let uninstalled = behavior_only_view::view().with_systems([make_system()]);
        assert_eq!(Rc::strong_count(&retained), 2);
        drop(uninstalled);
        assert_eq!(Rc::strong_count(&retained), 1);

        app.with_widget(behavior_only_view::view().with_systems([make_system()]));
        let registry = app.world.resource::<ViewRegistry>().unwrap();
        assert!(registry.iter().all(|view| view.systems.is_empty()));
        assert_eq!(registry.iter().count(), 1);
        assert_eq!(Rc::strong_count(&retained), 2);
        app.systems.run_all(&mut app.world).unwrap();
        assert_eq!(retained.get(), 1);
        drop(app);
        assert_eq!(Rc::strong_count(&retained), 1);
    }

    #[test]
    fn builtin_templates_are_consumed_only_when_installed_in_an_app() {
        let registry = ViewRegistry::with_builtins();
        let count: usize = registry.iter().map(|view| view.systems.len()).sum();
        assert!(count > 0);
        let mut app = crate::app::App::headless(8, 8);
        let existing = app.systems.iter().count();
        app.with_widgets(registry);
        assert_eq!(app.systems.iter().count(), existing + count);
        assert!(
            app.world
                .resource::<ViewRegistry>()
                .unwrap()
                .iter()
                .all(|view| view.systems.is_empty())
        );
    }

    #[test]
    fn registration_order_is_independent_from_paint_priority() {
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
        assert_eq!((views[0].name(), views[0].registration_id()), ("late", 0));
        assert_eq!((views[1].name(), views[1].registration_id()), ("early", 1));
        assert!(views.iter().all(|view| view.observation().is_some()));

        let paint: Vec<_> = registry.paint_iter().map(View::name).collect();
        assert_eq!(paint, ["early", "late"]);
    }

    #[test]
    fn systems_only_view_is_not_indexed_for_paint() {
        let mut registry = ViewRegistry::default();
        registry.insert(make_view("paint", 60));
        registry.insert(View::systems_only("behavior", &[]));

        assert_eq!(
            registry.iter().map(View::name).collect::<Vec<_>>(),
            ["paint", "behavior"]
        );
        assert_eq!(
            registry.paint_iter().map(View::name).collect::<Vec<_>>(),
            ["paint"]
        );
    }

    #[test]
    fn niche_descriptors_keep_attach_without_paint_hooks() {
        use crate::ui::widgets::Text;

        crate::ui!(compose StaticNiche {
            @@body
        });
        crate::ui!(compose TypedNiche(label: &'static str) {
            @@body {
                Text(text: label)
            }
        });

        let mut registry = ViewRegistry::default();
        registry.insert(make_view("paint", 60));
        for (factory, name, component) in [
            (
                StaticNiche::__view as fn() -> View,
                "StaticNiche",
                TypeId::of::<StaticNiche>(),
            ),
            (TypedNiche::__view, "TypedNiche", TypeId::of::<TypedNiche>()),
        ] {
            let mut descriptor = None;
            assert_eq!(tracked_allocations(|| descriptor = Some(factory())), 0);
            let descriptor = descriptor.unwrap();
            assert_eq!(descriptor.name(), name);
            assert_eq!(descriptor.component_filter(), Some(component));
            assert!(descriptor.auto_attach().is_some());
            assert!(descriptor.render().is_none());
            assert_eq!(descriptor.priority(), None);
            registry.insert(descriptor);
        }
        assert_eq!(
            registry.iter().map(View::name).collect::<Vec<_>>(),
            ["paint", "StaticNiche", "TypedNiche"],
        );
        assert_eq!(
            registry.paint_iter().map(View::name).collect::<Vec<_>>(),
            ["paint"],
        );
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
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let first = app.add_model(BoundState { value: 0 }).finish();
        let second = app.add_model(BoundState { value: 0 }).finish();
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
        let first = app.add_model(BoundState { value: 0 }).finish();
        let second = app.add_model(BoundState { value: 0 }).finish();
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
    fn raw_mutable_access_rebinds_view_to_final_model_without_allocating() {
        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 0 }).finish();
        let intermediate = app.add_model(BoundState { value: 0 }).finish();
        let final_model = app.add_model(BoundState { value: 0 }).finish();
        app.with_widget(
            make_view("bound-mutable-access", 60)
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

        for replacement in [intermediate.clone(), final_model.clone(), first.clone()] {
            app.world.get_mut::<BoundTile>(entity).unwrap().model = replacement;
            ViewRegistry::reconcile_observations(&mut app.world);
        }
        app.world.remove::<VisualDirty>(entity);

        let allocations = tracked_allocations(|| {
            app.world.get_mut::<BoundTile>(entity).unwrap().model = intermediate.clone();
            app.world.get_mut::<BoundTile>(entity).unwrap().model = final_model.clone();
            ViewRegistry::reconcile_observations(&mut app.world);
        });
        assert_eq!(allocations, 0);
        assert!(app.world.remove::<VisualDirty>(entity).is_some());

        first.set(1);
        intermediate.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(entity));
        final_model.set(1);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(entity));
    }

    #[test]
    fn pending_model_notifications_do_not_allocate_during_batch_rebind() {
        const ENTITY_COUNT: usize = 65;
        type BoundTiles<'world> =
            SystemQuery<'world, ComponentDriver<BoundTile>, Write<BoundTile>, ()>;

        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 0 }).finish();
        let second = app.add_model(BoundState { value: 0 }).finish();
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
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let model = app.add_model(BoundState { value: 0 }).finish();
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
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let mut renderer = StubRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );
        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        assert!(!ctx.background_handled());

        model.set(1);
        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        assert!(ctx.background_handled());
        model.set(2);
    }

    #[test]
    fn generated_view_reads_two_instances_and_two_aliases() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(32, 32);
        let first = app.add_model(BoundState { value: 1 }).finish();
        let second = app.add_model(BoundState { value: 2 }).finish();
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
        let mut renderer = StubRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );
        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        assert!(ctx.background_handled());

        app.world.insert(
            entity,
            PairTile {
                first: first.clone(),
                second: first,
            },
        );
        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        assert!(!ctx.background_handled());
    }

    #[test]
    fn optional_model_view_runs_with_and_without_an_instance() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 7 }).finish();
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
        let mut renderer = StubRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );

        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        let component = app.world.get::<OptionalBoundTile>(entity).unwrap();
        assert_eq!(component.visits.get(), 1);
        assert_eq!(component.seen.get(), None);
        assert!(!ctx.background_handled());

        app.world.insert(
            entity,
            OptionalBoundTile {
                model: Some(model),
                visits: Cell::new(1),
                seen: Cell::new(None),
            },
        );
        (view.render().expect("paint view"))(&app.world, entity, &mut ctx);
        let component = app.world.get::<OptionalBoundTile>(entity).unwrap();
        assert_eq!(component.visits.get(), 2);
        assert_eq!(component.seen.get(), Some(7));
        assert!(ctx.background_handled());
    }

    #[test]
    fn optional_model_observation_rebinds_without_allocating() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let model = app.add_model(VisualState { value: 0 }).finish();
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
        let other = paint_generic_tile::view::<u16>();
        assert_eq!(
            other.component_filter(),
            Some(TypeId::of::<GenericTile<u16>>())
        );
        assert_ne!(view.component_filter(), other.component_filter());
        let systems: [crate::ecs::System; 0] = [];
        let view = view.with_systems(systems);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    view.component_filter(),
                    Some(TypeId::of::<GenericTile<u8>>())
                );
            }),
            0
        );
        assert_eq!(
            View::systems_only("unfiltered", &[]).component_filter(),
            None
        );
    }

    #[test]
    fn generated_view_preserves_existing_builder_options() {
        let view = paint_options::view();
        assert_eq!(view.priority(), Some(42));
        assert!(view.auto_attach().is_some());
        assert!(view.inputs().is_some());
        assert_eq!(view.systems.len(), 1);
    }

    #[test]
    fn generated_view_rejects_model_writes_and_restores_scope_after_panic() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 }).finish();
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
        let mut renderer = StubRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view.paint(&app.world, entity, &mut ctx);
        }));
        assert!(failure.is_err());
        assert_eq!(model.value(), 0);
        model.set(2);
        assert_eq!(model.value(), 2);
    }

    #[test]
    fn despawned_observed_component_releases_its_subscription() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(BoundState { value: 0 }).finish();
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
    fn paint_index_keeps_priority_order() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("c", 80));
        reg.insert(make_view("a", 40));
        reg.insert(make_view("b", 50));
        let names: Vec<&str> = reg.paint_iter().map(View::name).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn paint_index_is_stable_within_same_priority() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("first-50", 50));
        reg.insert(make_view("second-50", 50));
        reg.insert(make_view("third-50", 50));
        let names: Vec<&str> = reg.paint_iter().map(View::name).collect();
        assert_eq!(names, ["first-50", "second-50", "third-50"]);
    }

    // A view fn must be able to mutate `PaintCtx` while concurrently
    // borrowing components from `&World`. Real renders read multiple
    // components alongside the ctx-mut path; the borrow pattern must
    // compose. Pinning that here so a regression breaks loudly.
    #[test]
    fn render_fn_can_mutate_ctx_while_reading_world() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        fn flip_bg_when_styled(world: &World, entity: Entity, ctx: &mut PaintCtx<'_>) {
            if world.get::<Style>(entity).is_some() {
                ctx.set_background_handled(true);
            }
        }

        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, Style::default());

        let style = world.get::<Style>(e).expect("style present");
        let rect = Rect::new(0, 0, 0, 0);
        let mut renderer = StubRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            style,
            &rect,
            &rect,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );

        flip_bg_when_styled(&world, e, &mut ctx);
        assert!(
            ctx.background_handled(),
            "view fn must mutate ctx while reading world"
        );

        // Pin the type aliases — failure here means signature drift
        // breaks user-code views.
        let _: ViewRender = flip_bg_when_styled;
        let _ = Fixed::ZERO;
    }

    #[test]
    fn clip_scope_restores_the_stack_and_retains_the_first_failure() {
        #[derive(Default)]
        struct ClipRenderer {
            pushed: bool,
            pops: usize,
            fills: usize,
        }

        impl Renderer for ClipRenderer {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                match request.command {
                    DrawCommand::PushClip { .. } => {
                        self.pushed = true;
                        Ok(())
                    }
                    DrawCommand::Fill { .. } => {
                        self.fills += 1;
                        Err(RenderError::BackendFailure)
                    }
                    DrawCommand::PopClip => {
                        self.pushed = false;
                        self.pops += 1;
                        Err(RenderError::InvalidGeometry)
                    }
                    _ => Ok(()),
                }
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let style = Style::default();
        let rect = Rect::new(0, 0, 8, 8);
        let path = Path::from_static(&[]);
        let mut renderer = ClipRenderer::default();
        let allocations = tracked_allocations(|| {
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            paint.with_clip_path(&path, Transform::IDENTITY, FillRule::EvenOdd, |paint| {
                paint.draw(&DrawCommand::Fill {
                    area: rect,
                    transform: Transform::IDENTITY,
                    quad: None,
                    color: crate::types::Color::rgb(0, 0, 0),
                    radius: Fixed::ZERO,
                    opa: 255,
                });
            });
            assert_eq!(paint.error(), Some(RenderError::BackendFailure));
        });

        assert_eq!(allocations, 0);
        assert!(!renderer.pushed);
        assert_eq!(renderer.pops, 1);
        assert_eq!(renderer.fills, 1);
    }

    #[test]
    fn raw_clip_commands_are_rejected_by_the_paint_context() {
        #[derive(Default)]
        struct CountingRenderer {
            submissions: usize,
        }

        impl Renderer for CountingRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                self.submissions += 1;
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let style = Style::default();
        let rect = Rect::new(0, 0, 8, 8);
        let path = Path::from_static(&[]);
        let push = DrawCommand::PushClip {
            path: &path,
            transform: Transform::IDENTITY,
            fill_rule: FillRule::EvenOdd,
        };
        let pop = DrawCommand::PopClip;

        for command in [&push, &pop] {
            let mut renderer = CountingRenderer::default();
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            paint.draw(command);
            assert_eq!(paint.error(), Some(RenderError::InvalidScope));
            assert_eq!(renderer.submissions, 0);
        }
    }

    #[test]
    fn source_pixels_borrow_texture_and_latch_errors_before_draw() {
        struct DrawCounter(usize);
        impl Renderer for DrawCounter {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                self.0 += 1;
                Ok(())
            }
            fn flush(&mut self) -> Result<(), RenderError> {
                Ok(())
            }
        }
        let bytes = [0u8; 16];
        let source =
            Texture::from_ref(&bytes, 2, 2, crate::render::texture::ColorFormat::RGBA8888).unwrap();
        let mut invalid =
            Texture::from_ref(&bytes, 2, 2, crate::render::texture::ColorFormat::RGBA8888).unwrap();
        invalid.stride = 32;
        let style = Style::default();
        let rect = Rect::new(0, 0, 2, 2);
        let command = DrawCommand::Fill {
            area: rect,
            transform: Transform::IDENTITY,
            quad: None,
            color: crate::types::Color::rgb(10, 20, 30),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let mut renderer = DrawCounter(0);
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert_eq!(
            tracked_allocations(|| {
                let pixels = paint.read_pixels(&source).unwrap();
                paint.draw(&command);
                assert_eq!(pixels.bytes().as_ptr(), bytes.as_ptr());
                assert_eq!(paint.error(), None);
                assert!(paint.read_pixels(&invalid).is_none());
                assert_eq!(paint.error(), Some(RenderError::InvalidTexture));
                paint.fail(RenderError::BackendFailure);
                assert!(paint.read_pixels(&source).is_none());
                paint.draw(&command);
                assert_eq!(paint.error(), Some(RenderError::InvalidTexture));
            }),
            0
        );
        assert_eq!(renderer.0, 1);
    }

    #[test]
    fn readback_is_prepared_and_latches_failures() {
        #[derive(Clone, Copy)]
        enum Outcome {
            PrepareFailure,
            SampleFailure,
            Empty,
        }

        struct ReadbackRenderer {
            outcome: Outcome,
            samples: Cell<usize>,
        }

        impl Renderer for ReadbackRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }

            fn prepare_readback(&mut self, _: &Rect) -> Result<(), RenderError> {
                match self.outcome {
                    Outcome::PrepareFailure => Err(RenderError::BackendFailure),
                    Outcome::SampleFailure | Outcome::Empty => Ok(()),
                }
            }

            fn sample_target_region(
                &self,
                _: &Rect,
            ) -> Result<Option<Texture<'static>>, RenderError> {
                self.samples.set(self.samples.get() + 1);
                match self.outcome {
                    Outcome::PrepareFailure | Outcome::SampleFailure => {
                        Err(RenderError::InvalidTexture)
                    }
                    Outcome::Empty => Ok(None),
                }
            }
        }

        let style = Style::default();
        let rect = Rect::new(0, 0, 8, 8);
        for (outcome, error, samples) in [
            (
                Outcome::PrepareFailure,
                Some(RenderError::BackendFailure),
                0,
            ),
            (Outcome::SampleFailure, Some(RenderError::InvalidTexture), 1),
            (Outcome::Empty, None, 1),
        ] {
            let mut renderer = ReadbackRenderer {
                outcome,
                samples: Cell::new(0),
            };
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );

            assert!(paint.sample_target_region(rect).is_none());
            assert_eq!(paint.error(), error);
            assert_eq!(renderer.samples.get(), samples);
        }
    }

    #[test]
    fn identified_text_submission_latches_failure_and_honors_the_context_clip() {
        #[derive(Default)]
        struct TextRenderer {
            draws: usize,
            text_runs: usize,
        }

        impl Renderer for TextRenderer {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                self.draws += 1;
                Ok(())
            }

            fn submit_text_run(
                &mut self,
                _: &::mirui::render::PreparedDraw<'_>,
                _: TextRunIdentity,
            ) -> Result<(), RenderError> {
                self.text_runs += 1;
                Err(RenderError::TextLayout)
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let style = Style::default();
        let rect = Rect::new(0, 0, 8, 8);
        let command = DrawCommand::Fill {
            area: rect,
            transform: Transform::IDENTITY,
            quad: None,
            color: crate::types::Color::rgb(0, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let identity = TextRunIdentity {
            entity: Entity {
                id: 1,
                generation: 0,
            },
            run_index: 0,
        };
        let mut renderer = TextRenderer::default();
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );

        paint.draw_text_run(&command, Rect::new(20, 20, 4, 4), identity);
        assert_eq!(paint.error(), None);
        paint.draw_text_run(&command, rect, identity);
        paint.draw(&command);
        assert_eq!(paint.error(), Some(RenderError::TextLayout));
        assert_eq!(renderer.text_runs, 1);
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn borrowed_scene_replay_retains_render_failure() {
        struct RejectingRenderer;

        impl Renderer for RejectingRenderer {
            type TargetState = ();

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request).map(|_| ())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let style = Style::default();
        let clip = Rect::new(0, 0, 8, 8);
        let mut renderer = RejectingRenderer;
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &clip,
            &clip,
            Transform::default(),
            None,
            WidgetState::Enabled,
        );
        let ops = [crate::render::scene::SceneOp::FillRect {
            area: clip,
            transform: Transform::default(),
            quad: None,
            color: crate::types::Color::rgb(20, 30, 40),
            radius: crate::types::Fixed::ZERO,
            opa: 255,
        }];
        ctx.replay(
            &ops,
            &crate::render::scene::resolver::SliceResolver::new(&[], &[]),
        );
        assert_eq!(
            ctx.error(),
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
            type TargetState = ();

            fn route(
                &self,
                _: &DrawRequest<'_, '_>,
            ) -> Result<crate::render::renderer::RenderRoute, RenderError> {
                Ok(crate::render::renderer::RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.transform = Some(request.command.transform());
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let style = Style::default();
        let clip = Rect::new(0, 0, 64, 64);
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
        let mut ctx = PaintCtx::new(
            &mut renderer,
            &style,
            &clip,
            &clip,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );

        ctx.replay_transformed_with_rgba(
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
}
