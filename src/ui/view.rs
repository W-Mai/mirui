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
    parent_typography: Option<&'frame super::Typography<'frame>>,
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
            parent_typography: None,
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

    /// Resolve the widget's text styling, including inherited declarations.
    pub fn typography(&self) -> super::Typography<'frame> {
        match self.parent_typography {
            Some(parent) => super::Typography::with_parent(self.style, Some(parent)),
            None => super::Typography::local(self.style),
        }
    }

    pub(crate) fn set_parent_typography(
        &mut self,
        parent: Option<&'frame super::Typography<'frame>>,
    ) {
        self.parent_typography = parent;
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

    /// Choose storage for an opaque image within its per-pixel byte limit.
    /// Use the renderer's preference when it fits. An unsupported limit returns an error.
    pub fn opaque_output_format(
        &self,
        max_bytes_per_pixel: usize,
    ) -> Result<crate::render::texture::ColorFormat, RenderError> {
        use crate::render::texture::ColorFormat;

        if let Some(format) = self.renderer.offscreen_format()
            && format.bytes_per_pixel() <= max_bytes_per_pixel
        {
            return Ok(format);
        }
        match max_bytes_per_pixel {
            4.. => Ok(ColorFormat::RGBA8888),
            3 => Ok(ColorFormat::RGB888),
            2 => Ok(ColorFormat::RGB565),
            _ => Err(RenderError::InvalidGeometry),
        }
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
                parent_typography: self.parent_typography,
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

    /// Records the first error encountered while preparing or drawing this View. Subsequent draws skip submission and the render walker receives the failure.
    pub fn fail(&mut self, error: RenderError) {
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

#[derive(Default)]
pub struct ViewRegistry {
    views: Vec<View>,
    input_attach_plan: Option<Rc<Vec<crate::ui::lifecycle::InputAttach>>>,
    prepared_layout_plan: Option<Rc<Vec<super::prepared_layout::RegisteredLayout>>>,
    paint_order: Vec<usize>,
    next_registration: u64,
    bindings: Vec<ViewBinding>,
    typography: super::style::TypographyDeclarations,
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
        if !self.views[0].definition.component_tracking_ready {
            world.watch_component_type(TypeId::of::<Style>());
            world.watch_component_type(TypeId::of::<super::Parent>());
        }
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
        if change.type_id == TypeId::of::<Style>() {
            use crate::ecs::world::ComponentChangeKind;
            match change.kind {
                ComponentChangeKind::Component | ComponentChangeKind::InheritedStyle(_) => {
                    self.typography.update(world, change.entity, false);
                }
                _ => {}
            }
        } else if change.type_id == TypeId::of::<super::Parent>() {
            self.typography.update(world, change.entity, true);
        } else if change.type_id == TypeId::of::<super::Children>() {
            self.typography.update(world, change.entity, false);
        }
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
            crate::ecs::world::ComponentChangeKind::InheritedStyle(fields) => {
                if super::style::metrics_changed(fields)
                    || fields & crate::ecs::world::ComponentChangeKind::LOCAL_LAYOUT != 0
                {
                    ViewInvalidation::Layout
                } else {
                    ViewInvalidation::Visual
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
        reg.insert(crate::input::event::scroll::view());
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
        if let Some(hook) = view.definition.prepared_layout {
            let plan = self
                .prepared_layout_plan
                .get_or_insert_with(|| Rc::new(Vec::new()));
            let plan = Rc::make_mut(plan);
            plan.reserve_exact(1);
            plan.push(super::prepared_layout::RegisteredLayout {
                registration: view.registration_id(),
                hook,
            });
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

    pub(crate) fn prepared_layout_plan(
        &self,
    ) -> Option<Rc<Vec<super::prepared_layout::RegisteredLayout>>> {
        self.prepared_layout_plan.as_ref().map(Rc::clone)
    }

    pub(crate) fn prepared_layout_count(world: &World) -> usize {
        world
            .resource::<Self>()
            .and_then(|registry| registry.prepared_layout_plan.as_deref())
            .map_or(0, |plan| {
                plan.iter()
                    .filter(|entry| (entry.hook.needed)(world))
                    .count()
            })
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
