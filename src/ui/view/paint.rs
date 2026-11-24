use core::task::{Context, Poll, Waker};

use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::renderer::{
    DrawRequest, FallbackRegion, RenderError, RenderFeature, RenderRoute, Renderer, TextRunIdentity,
};
use crate::render::scratch::PlaneLayout;
use crate::render::texture::{Texture, TexturePixels};
use crate::types::{Point, Rect, Transform};
use crate::ui::outputs::workspace::PaintWorkspace;
use crate::ui::theme::WidgetState;
use crate::ui::{Style, Typography};

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
    parent_typography: Option<&'frame Typography<'frame>>,
    rect: &'frame Rect,
    clip: &'frame Rect,
    transform: Transform,
    quad: Option<[Point; 4]>,
    state: WidgetState,
    background_handled: bool,
    error: Option<RenderError>,
    pub(super) workspace: Option<&'frame PaintWorkspace>,
    pub(super) layout_offset: Point,
    frame_request: FrameRequest,
    pub(super) query_reads_global: bool,
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
    pub fn typography(&self) -> Typography<'frame> {
        match self.parent_typography {
            Some(parent) => Typography::with_parent(self.style, Some(parent)),
            None => Typography::local(self.style),
        }
    }

    pub(crate) fn set_parent_typography(&mut self, parent: Option<&'frame Typography<'frame>>) {
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

    pub(super) fn replay_error(error: crate::render::scene::replay::ReplayError) -> RenderError {
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
