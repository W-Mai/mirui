use crate::ecs::Entity;
use crate::types::{Fixed, PhysicalRect, Rect, Transform3D, Viewport};

use super::PreparedDraw;
use super::command::{CompositeMode, DrawCommand};
use super::scratch::{PlaneLayout, PlaneRequirements};
use super::target::{BackendTarget, TargetPlan, TargetUpdate};
use super::texture::{ColorFormat, Texture};
use core::task::{Context, Poll, Waker};

#[cfg(test)]
#[path = "../../tests/unit/render/physical_clip_tests.rs"]
mod physical_clip_tests;

#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/render/draw_dispatch_tests.rs"]
mod draw_dispatch_tests;

/// Stable identity of one run in a bounded `Text` widget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextRunIdentity {
    pub entity: Entity,
    pub run_index: usize,
}

pub(crate) fn constrain_physical_clip(clip: Rect, boundary: Option<PhysicalRect>) -> Rect {
    match boundary {
        Some(boundary) => clip
            .intersect(&Rect::new(
                boundary.x(),
                boundary.y(),
                boundary.width(),
                boundary.height(),
            ))
            .unwrap_or(Rect::new(0, 0, 0, 0)),
        None => clip,
    }
}

/// One draw under its effective clip and shared projective transform.
#[derive(Clone, Copy)]
pub struct DrawRequest<'cmd, 'data> {
    pub command: &'cmd DrawCommand<'data>,
    pub clip: Rect,
    pub projective: Transform3D,
    /// Additional target-local integer write boundary. Geometry and sampling remain unchanged.
    pub physical_clip: Option<PhysicalRect>,
}

impl<'cmd, 'data> DrawRequest<'cmd, 'data> {
    pub const fn new(command: &'cmd DrawCommand<'data>, clip: Rect) -> Self {
        Self {
            command,
            clip,
            projective: Transform3D::IDENTITY,
            physical_clip: None,
        }
    }

    pub const fn with_projective(mut self, projective: Transform3D) -> Self {
        self.projective = projective;
        self
    }

    pub const fn with_physical_clip(mut self, clip: PhysicalRect) -> Self {
        self.physical_clip = Some(clip);
        self
    }

    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32")
    ))]
    pub(crate) fn clip_route(&self, route: RenderRoute) -> Result<RenderRoute, RenderError> {
        let (Some(clip), RenderRoute::ExactFallback(region)) = (self.physical_clip, route) else {
            return Ok(route);
        };
        let Some(rect) = region.rect().intersection(clip) else {
            return Ok(RenderRoute::ExactFallback(FallbackRegion::EMPTY));
        };
        Ok(RenderRoute::ExactFallback(FallbackRegion::new(
            rect,
            region.stride_bytes(),
        )?))
    }

    pub(crate) fn validate_write_boundary(&self, viewport: Viewport) -> Result<(), RenderError> {
        if let (Some(clip), DrawCommand::ApplyBlur { alpha, region }) =
            (self.physical_clip, self.command)
            && *alpha > Fixed::ZERO
            && *alpha < Fixed::ONE
            && let Some(area) = viewport.physical_rect(*region)
            && area.intersection(clip) != Some(area)
        {
            return Err(RenderError::Unsupported(RenderFeature::Blur));
        }
        Ok(())
    }

    /// Validate command data shared by all renderer routes.
    pub fn validate(&self) -> Result<(), RenderError> {
        self.validate_projection()?;
        self.validate_texture()
    }

    pub(crate) fn validate_texture(&self) -> Result<(), RenderError> {
        if let DrawCommand::Blit { size, texture, .. } = self.command {
            texture.storage_layout()?;
            if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
                return Err(RenderError::InvalidGeometry);
            }
        }
        Ok(())
    }

    pub(crate) fn validate_projection(&self) -> Result<(), RenderError> {
        if matches!(
            self.command,
            DrawCommand::Fill { quad: Some(_), .. }
                | DrawCommand::Border { quad: Some(_), .. }
                | DrawCommand::Blit { quad: Some(_), .. }
        ) && !self.projective.is_identity()
        {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        Ok(())
    }
}

/// How a backend preserves the requested drawing semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderRoute {
    Native,
    ExactFallback(FallbackRegion),
}

/// A clipped physical RGBA region prepared for an exact fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FallbackRegion {
    rect: PhysicalRect,
    stride_bytes: usize,
}

impl FallbackRegion {
    pub const EMPTY: Self = Self {
        rect: PhysicalRect::EMPTY,
        stride_bytes: 0,
    };

    pub(crate) fn new(rect: PhysicalRect, stride_bytes: usize) -> Result<Self, RenderError> {
        PlaneLayout::new(
            rect.width(),
            rect.height(),
            ColorFormat::RGBA8888,
            stride_bytes,
            PlaneRequirements::CPU,
        )
        .map_err(|_| RenderError::InvalidGeometry)?;
        Ok(Self { rect, stride_bytes })
    }

    #[cfg(test)]
    pub(crate) fn from_parts(x: u16, y: u16, width: u16, height: u16, stride_bytes: usize) -> Self {
        Self::new(
            PhysicalRect::new(x, y, width, height).expect("test region fits"),
            stride_bytes,
        )
        .expect("test fallback layout is valid")
    }

    pub(crate) fn rgba8(
        rect: PhysicalRect,
        requirements: PlaneRequirements,
    ) -> Result<Self, RenderError> {
        if rect.is_empty() {
            return Ok(Self::EMPTY);
        }
        let layout = PlaneLayout::aligned(
            rect.width(),
            rect.height(),
            ColorFormat::RGBA8888,
            requirements,
        )
        .map_err(|_| RenderError::ResourceLimit(RenderResource::Target))?;
        Ok(Self {
            rect,
            stride_bytes: layout.stride_bytes(),
        })
    }

    pub(crate) fn from_logical_bounds(
        bounds: Rect,
        viewport: Viewport,
        capacity_bytes: Option<usize>,
    ) -> Result<Self, RenderError> {
        Self::from_logical_bounds_with(bounds, viewport, capacity_bytes, PlaneRequirements::CPU)
    }

    pub(crate) fn from_logical_bounds_with(
        bounds: Rect,
        viewport: Viewport,
        capacity_bytes: Option<usize>,
        requirements: PlaneRequirements,
    ) -> Result<Self, RenderError> {
        let rect = viewport
            .physical_rect(bounds)
            .unwrap_or(PhysicalRect::EMPTY);
        let region = Self::rgba8(rect, requirements)?;
        if region.required_bytes() == 0 {
            return Ok(region);
        }
        let capacity_bytes = capacity_bytes.ok_or(RenderError::MissingWorkspace)?;
        if region.required_bytes() > capacity_bytes {
            return Err(RenderError::InsufficientWorkspace {
                required_bytes: region.required_bytes(),
                capacity_bytes,
            });
        }
        Ok(region)
    }

    pub const fn rect(self) -> PhysicalRect {
        self.rect
    }

    pub const fn x(self) -> u16 {
        self.rect.x()
    }

    pub const fn y(self) -> u16 {
        self.rect.y()
    }

    pub const fn width(self) -> u16 {
        self.rect.width()
    }

    pub const fn height(self) -> u16 {
        self.rect.height()
    }

    pub const fn stride_bytes(self) -> usize {
        self.stride_bytes
    }

    pub const fn required_bytes(self) -> usize {
        self.stride_bytes * self.rect.height() as usize
    }

    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn plane_layout(
        self,
        requirements: PlaneRequirements,
    ) -> Result<PlaneLayout, RenderError> {
        PlaneLayout::new(
            self.width(),
            self.height(),
            ColorFormat::RGBA8888,
            self.stride_bytes,
            requirements,
        )
        .map_err(|_| RenderError::InvalidGeometry)
    }
}

impl RenderRoute {
    #[cfg(any(
        feature = "wgpu",
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn target_readback(
        region: Option<PhysicalRect>,
        capacity_bytes: Option<usize>,
        requirements: PlaneRequirements,
    ) -> Result<Self, RenderError> {
        let Some(region) = region else {
            return Ok(Self::ExactFallback(FallbackRegion::rgba8(
                PhysicalRect::EMPTY,
                requirements,
            )?));
        };
        let plan = FallbackRegion::rgba8(region, requirements)?;
        let capacity_bytes = capacity_bytes.ok_or(RenderError::MissingWorkspace)?;
        if plan.required_bytes() > capacity_bytes {
            return Err(RenderError::InsufficientWorkspace {
                required_bytes: plan.required_bytes(),
                capacity_bytes,
            });
        }
        Ok(Self::ExactFallback(plan))
    }
}

pub(crate) struct RegionRenderer<'a> {
    inner: &'a mut dyn Renderer,
    origin_x: Fixed,
    origin_y: Fixed,
    physical_origin: (u16, u16),
}

impl RegionRenderer<'_> {
    pub(crate) fn new(inner: &mut dyn Renderer, physical_origin: (u16, u16)) -> RegionRenderer<'_> {
        let scale = inner.output_scale();
        RegionRenderer {
            inner,
            origin_x: Fixed::from(physical_origin.0) / scale,
            origin_y: Fixed::from(physical_origin.1) / scale,
            physical_origin,
        }
    }

    fn clip(&self, clip: Rect) -> Rect {
        Rect {
            x: clip.x - self.origin_x,
            y: clip.y - self.origin_y,
            w: clip.w,
            h: clip.h,
        }
    }

    fn local_command<'data>(&self, request: &DrawRequest<'_, 'data>) -> Option<DrawCommand<'data>> {
        let translate = |point: crate::types::Point| crate::types::Point {
            x: point.x - self.origin_x,
            y: point.y - self.origin_y,
        };
        let mut command = *request.command;
        match &mut command {
            DrawCommand::Fill {
                quad: Some(quad), ..
            }
            | DrawCommand::Border {
                quad: Some(quad), ..
            }
            | DrawCommand::Blit {
                quad: Some(quad), ..
            } => {
                *quad = quad.map(translate);
                return Some(command);
            }
            _ => {}
        }
        if !request.projective.is_identity() {
            return None;
        }
        match &mut command {
            DrawCommand::Fill { transform, .. }
            | DrawCommand::Border { transform, .. }
            | DrawCommand::Blit { transform, .. }
            | DrawCommand::GlyphRun { transform, .. }
            | DrawCommand::PosedGlyphRun { transform, .. }
            | DrawCommand::Line { transform, .. }
            | DrawCommand::Arc { transform, .. }
            | DrawCommand::FillPath { transform, .. }
            | DrawCommand::StrokePath { transform, .. }
            | DrawCommand::PushClip { transform, .. } => {
                *transform = crate::types::Transform::translate(-self.origin_x, -self.origin_y)
                    .compose(transform);
            }
            DrawCommand::ApplyBlur { region, .. } => *region = self.clip(*region),
            DrawCommand::PopClip => {}
        }
        Some(command)
    }

    fn request<'cmd, 'data>(&self, request: &DrawRequest<'cmd, 'data>) -> DrawRequest<'cmd, 'data> {
        DrawRequest {
            clip: self.clip(request.clip),
            projective: Transform3D::translate(-self.origin_x, -self.origin_y)
                .compose(&request.projective),
            physical_clip: self.local_physical_clip(request.physical_clip),
            ..*request
        }
    }

    fn local_physical_clip(&self, clip: Option<PhysicalRect>) -> Option<PhysicalRect> {
        clip.map(|clip| {
            let (x, y) = self.physical_origin;
            let left = clip.x().saturating_sub(x);
            let top = clip.y().saturating_sub(y);
            let right = clip.right().saturating_sub(x);
            let bottom = clip.bottom().saturating_sub(y);
            PhysicalRect::new(left, top, right - left, bottom - top).unwrap_or(PhysicalRect::EMPTY)
        })
    }

    fn local_request<'cmd, 'data>(
        &self,
        command: &'cmd DrawCommand<'data>,
        request: &DrawRequest<'_, '_>,
    ) -> DrawRequest<'cmd, 'data> {
        DrawRequest {
            command,
            clip: self.clip(request.clip),
            projective: request.projective,
            physical_clip: self.local_physical_clip(request.physical_clip),
        }
    }
}

impl Renderer for RegionRenderer<'_> {
    type TargetState = ();

    fn poll_draw_request(
        &mut self,
        request: DrawRequest<'_, '_>,
        text_run: Option<TextRunIdentity>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), RenderError>> {
        if let Some(command) = self.local_command(&request) {
            return self.inner.poll_draw_request(
                self.local_request(&command, &request),
                text_run,
                cx,
            );
        }
        self.inner
            .poll_draw_request(self.request(&request), text_run, cx)
    }

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        self.inner.plan_target(layout)
    }

    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        self.inner.can_reuse_target(target, plan)
    }

    fn create_target(
        &mut self,
        plan: &TargetPlan,
    ) -> Result<alloc::boxed::Box<dyn BackendTarget>, RenderError> {
        self.inner.create_target(plan)
    }

    fn with_target(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        update: TargetUpdate,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        self.inner
            .with_target(target, viewport, update, workspace, body)
    }

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        if let Some(command) = self.local_command(request) {
            return self.inner.route(&self.local_request(&command, request));
        }
        self.inner.route(&self.request(request))
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        self.inner.submit(draw)
    }

    fn submit_text_run(
        &mut self,
        draw: &::mirui::render::PreparedDraw<'_>,
        identity: TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.inner.submit_text_run(draw, identity)
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        if let Some(command) = self.local_command(&request) {
            return self
                .inner
                .prepare(self.local_request(&command, &request), route, cx);
        }
        self.inner.prepare(self.request(&request), route, cx)
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        self.inner.flush()
    }

    fn output_scale(&self) -> Fixed {
        self.inner.output_scale()
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        let local = self.inner.plan_scope(&self.clip(*bounds))?;
        let scale = self.output_scale();
        if scale <= Fixed::ZERO {
            return Err(RenderError::InvalidGeometry);
        }
        let origin_x = i32::from(self.physical_origin.0);
        let origin_y = i32::from(self.physical_origin.1);
        let x = i32::from(local.x())
            .checked_add(origin_x)
            .ok_or(RenderError::InvalidGeometry)?;
        let y = i32::from(local.y())
            .checked_add(origin_y)
            .ok_or(RenderError::InvalidGeometry)?;
        let rect = PhysicalRect::new(
            u16::try_from(x).map_err(|_| RenderError::InvalidGeometry)?,
            u16::try_from(y).map_err(|_| RenderError::InvalidGeometry)?,
            local.width(),
            local.height(),
        )
        .ok_or(RenderError::InvalidGeometry)?;
        FallbackRegion::new(rect, local.stride_bytes())
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        draw: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        self.inner.modify_target_region(&self.clip(*src), draw)
    }
}

/// Semantic operation that a backend cannot execute exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderFeature {
    AffineGeometry,
    ProjectiveGeometry,
    PathClip,
    Mask,
    PathStroke,
    FillRule,
    GradientPaint,
    TextInkBounds,
    StrokeStyle,
    Blur,
    RoundedFill,
    RoundedBlit,
    BlitOpacity,
    Composite(CompositeMode),
    TextureFormat(ColorFormat),
    Readback,
    Scope,
    ScrollBlit,
}

/// Bounded backend resource exhausted while preparing a draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderResource {
    Geometry,
    Uniforms,
    Texture,
    Target,
}

/// Failure to preserve a requested draw without approximation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// Source preparation has not completed; no ready draw was submitted.
    ResourcePending,
    Unsupported(RenderFeature),
    /// A bounded label has no valid initial value; query the App for the widget and cause.
    TextContent,
    /// Text preparation failed before drawing; query the App for the widget and cause.
    TextLayout,
    /// App-wide text reservation failed before any text entity entered layout.
    TextPreparation,
    /// A renderer scope command was submitted outside its structured API.
    InvalidScope,
    /// The source device operation must execute on its creating thread.
    WrongThread,
    InvalidGeometry,
    InvalidTexture,
    MissingWorkspace,
    InsufficientWorkspace {
        required_bytes: usize,
        capacity_bytes: usize,
    },
    ResourceLimit(RenderResource),
    /// No drawable is currently available. A later frame may retry without losing pending changes.
    SurfaceUnavailable,
    BackendFailure,
}

#[cfg(any(all(feature = "web-canvas", target_arch = "wasm32"), test))]
pub(crate) fn copy_packed_rgba8(
    bytes: &[u8],
    clipped: PhysicalRect,
    source_origin: (i32, i32),
    dst: &mut Texture<'_>,
) -> Result<(), RenderError> {
    if dst.format != ColorFormat::RGBA8888 || !dst.valid_storage() {
        return Err(RenderError::InvalidTexture);
    }
    let width = usize::from(clipped.width());
    let height = usize::from(clipped.height());
    let row_bytes = width.checked_mul(4).ok_or(RenderError::InvalidGeometry)?;
    let required = row_bytes
        .checked_mul(height)
        .ok_or(RenderError::InvalidGeometry)?;
    if bytes.len() < required {
        return Err(RenderError::BackendFailure);
    }
    with_rgba8_readback(clipped, source_origin, dst, |area, pixels| {
        let copy_bytes = usize::from(area.width()) * ColorFormat::RGBA8888.bytes_per_pixel();
        let dst_stride = pixels.stride();
        let dst_bytes = pixels.bytes_mut();
        for row in 0..usize::from(area.height()) {
            let src_start = row * row_bytes;
            let dst_start = row * dst_stride;
            dst_bytes[dst_start..dst_start + copy_bytes]
                .copy_from_slice(&bytes[src_start..src_start + copy_bytes]);
        }
        Ok(())
    })
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "wgpu", not(target_arch = "wasm32")),
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
pub(crate) fn with_rgba8_readback(
    clipped: PhysicalRect,
    source_origin: (i32, i32),
    dst: &mut Texture<'_>,
    read: impl FnOnce(
        PhysicalRect,
        &mut super::texture::TexturePixelsMut<'_>,
    ) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    if dst.format != ColorFormat::RGBA8888 || !dst.valid_storage() {
        return Err(RenderError::InvalidTexture);
    }
    let offset = |position: u16, origin: i32| {
        i32::from(position)
            .checked_sub(origin)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(RenderError::InvalidGeometry)
    };
    let x = offset(clipped.x(), source_origin.0)?;
    let y = offset(clipped.y(), source_origin.1)?;
    let width = usize::from(clipped.width()).min(usize::from(dst.width).saturating_sub(x));
    let height = usize::from(clipped.height()).min(usize::from(dst.height).saturating_sub(y));
    if width == 0 || height == 0 {
        return Ok(());
    }
    let area = PhysicalRect::new(clipped.x(), clipped.y(), width as u16, height as u16)
        .ok_or(RenderError::InvalidGeometry)?;
    let mut pixels = dst.pixels_mut()?;
    let stride = pixels.stride();
    let layout = PlaneLayout::new(
        area.width(),
        area.height(),
        ColorFormat::RGBA8888,
        stride,
        PlaneRequirements::CPU,
    )
    .map_err(|_| RenderError::InvalidTexture)?;
    let start = y * stride + x * ColorFormat::RGBA8888.bytes_per_pixel();
    let bytes = &mut pixels.bytes_mut()[start..start + layout.minimum_bytes()];
    let mut region = Texture::new(bytes, area.width(), area.height(), ColorFormat::RGBA8888)?
        .with_stride(stride)?;
    let mut pixels = region.pixels_mut()?;
    read(area, &mut pixels)
}

/// Failure to execute a draw command under a non-affine homography.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectiveDrawError {
    /// The renderer or command variant has no exact projective path.
    Unsupported,
    /// No bounded software target was supplied for an exact fallback.
    MissingFallbackStorage,
    /// The supplied software target cannot hold the clipped output region.
    InsufficientFallbackStorage {
        required_bytes: usize,
        capacity_bytes: usize,
    },
    /// The effective transform is singular or crosses the near plane.
    InvalidProjection,
    /// The target rejected a readback, upload, or composite operation.
    BackendFailure,
}

impl From<ProjectiveDrawError> for RenderError {
    fn from(error: ProjectiveDrawError) -> Self {
        match error {
            ProjectiveDrawError::Unsupported => {
                Self::Unsupported(RenderFeature::ProjectiveGeometry)
            }
            ProjectiveDrawError::MissingFallbackStorage => Self::MissingWorkspace,
            ProjectiveDrawError::InsufficientFallbackStorage {
                required_bytes,
                capacity_bytes,
            } => Self::InsufficientWorkspace {
                required_bytes,
                capacity_bytes,
            },
            ProjectiveDrawError::InvalidProjection => Self::InvalidGeometry,
            ProjectiveDrawError::BackendFailure => Self::BackendFailure,
        }
    }
}

pub trait Renderer: super::target::__private::TargetScopeDispatch {
    /// Owned state restored after binding an independent native target. Use `()` when native targets are unsupported.
    type TargetState
    where
        Self: Sized;

    /// Plan native target storage without allocating or modifying device state. `None` selects software output production.
    fn plan_target(&self, _layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        Ok(None)
    }

    /// Check writable storage compatibility without binding, allocating, reading or submitting work.
    ///
    /// A different device, stale allocation or incompatible plan returns `false`; device failures and active-target conflicts return errors. Completed contents are not required for storage reuse. Binding must repeat the storage checks and reject incomplete contents for `TargetUpdate::Incremental`; a mismatch must not silently rebuild an incremental target.
    fn can_reuse_target(
        &self,
        _target: &dyn BackendTarget,
        _plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        Ok(false)
    }

    /// Allocate an unpublished target according to a storage plan.
    fn create_target(
        &mut self,
        _plan: &TargetPlan,
    ) -> Result<alloc::boxed::Box<dyn BackendTarget>, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Scope))
    }

    /// Bind a native target and return owned restoration state, releasing device and workspace borrows before returning.
    ///
    /// Reject foreign devices, stale resources and active-target feedback before writing. `Rebuild` initializes the entire target to zero once; `Incremental` requires successfully completed contents. A failed bind must restore the parent state or latch device failure; any partially modified contents become invalid.
    fn bind_target(
        &mut self,
        _target: &mut dyn BackendTarget,
        _viewport: Viewport,
        _update: TargetUpdate,
        _workspace: &mut [u8],
    ) -> Result<Self::TargetState, RenderError>
    where
        Self: Sized,
    {
        Err(RenderError::Unsupported(RenderFeature::Scope))
    }

    /// Restore parent state after drawing. `completed` means the callback and its flush succeeded, not that the output was published.
    ///
    /// On failure, discard or safely finish pending child work before restoring the parent. Accept child contents only when `completed` is true and restoration succeeds; otherwise invalidate them until rebuilt. Restore the parent without initializing it again. Latch any restoration failure so later submissions cannot use unknown device state. This method must not panic during unwind cleanup.
    fn restore_target(
        &mut self,
        _state: Self::TargetState,
        _completed: bool,
    ) -> Result<(), RenderError>
    where
        Self: Sized,
    {
        Err(RenderError::Unsupported(RenderFeature::Scope))
    }

    /// Execute one callback on an independent target while preserving this renderer's complete routing.
    ///
    /// Drawing uses the supplied target-local viewport. Parent region offsets and projective wrappers apply only to parent drawing, not to production of an independent image.
    ///
    /// Flush the parent, bind with the requested content policy, execute once, flush successful child work and restore. Rebuilding an empty body still clears the target; intermediate flushes and nested returns do not clear it again. Restoration runs on callback errors and unwinding; the earliest error is returned. Failed writes are not rolled back. Output publication belongs to the caller. Abort does not guarantee cleanup.
    fn with_target(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        update: TargetUpdate,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        self.dispatch_target_scope(target, viewport, update, workspace, body)
    }

    /// Classify exact execution before changing the target. Unknown backends
    /// conservatively reject requests until they define their own route.
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        let feature = if request.projective.is_identity() {
            RenderFeature::AffineGeometry
        } else {
            RenderFeature::ProjectiveGeometry
        };
        Err(RenderError::Unsupported(feature))
    }

    /// Prepare source storage using the route for this request, without modifying the target.
    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        PreparedDraw::poll_cpu(request, route, self.output_scale(), cx)
    }

    /// Execute a prepared command without performing source I/O.
    fn submit(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError>;

    /// Route, prepare and submit one request, preserving its task waker and optional text-run identity.
    ///
    /// Pending preparation never submits. Coordinate wrappers can normalize the request once before forwarding; independent preparation and scene pre-routing remain available through `route`, `prepare` and `submit`.
    fn poll_draw_request(
        &mut self,
        request: DrawRequest<'_, '_>,
        text_run: Option<TextRunIdentity>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), RenderError>> {
        let route = self.route(&request)?;
        match &self.prepare(request, route, cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(*error)),
            Poll::Ready(Ok(draw)) => Poll::Ready(match text_run {
                Some(identity) => self.submit_text_run(draw, identity),
                None => self.submit(draw),
            }),
        }
    }

    /// Prepare and submit an immediately available request. Pending sources return `ResourcePending`.
    ///
    /// Use `poll_draw_request` with a task context when source readiness must wake the caller.
    fn draw_request(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(result) = self.poll_draw_request(*request, None, &mut cx) else {
            return Err(RenderError::ResourcePending);
        };
        result
    }

    /// Prepare and submit an immediately available bounded text run.
    fn draw_text_run(
        &mut self,
        request: &DrawRequest<'_, '_>,
        identity: TextRunIdentity,
    ) -> Result<(), RenderError> {
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(result) = self.poll_draw_request(*request, Some(identity), &mut cx) else {
            return Err(RenderError::ResourcePending);
        };
        result
    }

    /// Submit a bounded text run with its stable widget and run identity.
    /// Other renderers use the ordinary draw path.
    fn submit_text_run(
        &mut self,
        draw: &::mirui::render::PreparedDraw<'_>,
        _identity: TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.submit(draw)
    }

    /// Submit pending drawing work and report any submission failure. Dropping a renderer does not submit its pending work.
    fn flush(&mut self) -> Result<(), RenderError>;

    fn output_scale(&self) -> Fixed {
        Fixed::ONE
    }

    /// Whether this backend hosts [`crate::ui::OffscreenRender`] output.
    /// Production selects a native target when its plan and complete working set fit; otherwise it uses software pixels. Both results use `DrawCommand::Blit`.
    /// Returning `false` makes the render walker inline-render the subtree.
    fn supports_offscreen(&self) -> bool {
        false
    }

    /// Format the offscreen buffer should be allocated in. `None`
    /// means the backend does not host offscreen rendering and the
    /// walker should not call [`Self::supports_offscreen`].
    fn offscreen_format(&self) -> Option<ColorFormat> {
        None
    }

    /// Copy a logical-pixel rect from the current target into `dst` as
    /// straight-alpha color bytes.
    /// `dst` is sized in physical pixels by the caller. Pixels outside
    /// the target are left unchanged. A backend that returns `Some`
    /// from [`Self::offscreen_format`] must override this. Readback
    /// failure must be reported before the caller reuses `dst`.
    fn read_target_region(
        &self,
        _src: &Rect,
        _dst: &mut crate::render::texture::Texture,
    ) -> Result<(), RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    /// Logical-pixel `src` in, physical-resolution straight-alpha texture
    /// out. An empty target intersection returns `Ok(None)`; readback failure
    /// is distinct.
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    /// Hand the closure a mutable `Texture` over physical pixels inside
    /// `src`. Software may borrow the target directly; other backends may
    /// read and replace the region.
    /// Returns `Ok(true)` when the closure ran, `Ok(false)` when the
    /// target region is empty, or an error when reading or writing fails.
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    /// Plan the clipped physical target needed by one ordered fallback scope.
    fn plan_scope(&self, _bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Scope))
    }

    /// Replay one ordered scope inside a prepared physical target region.
    /// The nested renderer preserves global command coordinates while
    /// rebasing the region to its local software target.
    fn render_scope(
        &mut self,
        region: FallbackRegion,
        draw: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        if region.rect.is_empty() {
            return Ok(false);
        }
        let scale = self.output_scale();
        if scale <= Fixed::ZERO {
            return Err(RenderError::InvalidGeometry);
        }
        let logical = Rect {
            x: Fixed::from(region.x()) / scale,
            y: Fixed::from(region.y()) / scale,
            w: Fixed::from(region.width()) / scale,
            h: Fixed::from(region.height()) / scale,
        };
        self.modify_target_region(&logical, &mut |target| {
            if target.width != region.width() || target.height != region.height() {
                return Err(RenderError::InvalidGeometry);
            }
            let width = target.width;
            let height = target.height;
            let format = target.format;
            let stride = target.stride;
            let alpha_mode = target.alpha_mode;
            let mut pixels = target.pixels_mut()?;
            let mut texture =
                Texture::new(pixels.bytes_mut(), width, height, format)?.with_stride(stride)?;
            texture.alpha_mode = alpha_mode;
            let mut software = crate::render::backends::sw::SwRenderer::new(texture);
            software.viewport = crate::types::Viewport::new(width, height, scale);
            let mut local = RegionRenderer {
                inner: &mut software,
                origin_x: logical.x,
                origin_y: logical.y,
                physical_origin: (region.x(), region.y()),
            };
            draw(&mut local)?;
            local.flush()
        })
    }

    fn blur_target_region(&mut self, alpha: Fixed, region: &Rect) -> Result<(), RenderError> {
        if alpha <= Fixed::ZERO || alpha >= Fixed::ONE {
            return Ok(());
        }
        self.modify_target_region(region, &mut |texture| {
            let bounds = Rect::new(0, 0, texture.width(), texture.height());
            crate::render::backends::sw::blur::iir_blur_inplace(
                &mut texture.pixels_mut()?,
                alpha,
                bounds,
            );
            Ok(())
        })?;
        Ok(())
    }

    /// Backends that defer draws (currently only `wgpu`) need to submit
    /// pending work before a `sample_target_region` / `read_target_region`
    /// call can see this frame's pixels. Eager backends keep the default
    /// no-op. Callers running readback should invoke this immediately
    /// before the read; idempotent if no work is pending.
    fn prepare_readback(&mut self, _src: &Rect) -> Result<(), RenderError> {
        Ok(())
    }

    /// Whether this backend implements `scroll_target_region`. The
    /// dirty walker checks this before emitting a `RegionShift`.
    fn supports_scroll_blit(&self) -> bool {
        false
    }

    /// Shift `area`'s pixels by `(dx, dy)` logical pixels. Pixels evicted
    /// from `area` are repainted by the caller. Backends opt in by
    /// overriding this and `supports_scroll_blit()`.
    fn scroll_target_region(
        &mut self,
        _area: &Rect,
        _dx: Fixed,
        _dy: Fixed,
    ) -> Result<(), RenderError> {
        Err(RenderError::Unsupported(RenderFeature::ScrollBlit))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/renderer.rs"]
mod tests;
