use crate::types::{Fixed, PhysicalRect, Rect, Transform3D, Viewport};

use super::command::{CompositeMode, DrawCommand};
use super::scratch::{PlaneLayout, PlaneRequirements};
use super::texture::{ColorFormat, TexBuf, Texture};

/// One draw under its effective clip and shared projective transform.
#[derive(Clone, Copy)]
pub struct DrawRequest<'cmd, 'data> {
    pub command: &'cmd DrawCommand<'data>,
    pub clip: Rect,
    pub projective: Transform3D,
}

impl<'cmd, 'data> DrawRequest<'cmd, 'data> {
    pub const fn new(command: &'cmd DrawCommand<'data>, clip: Rect) -> Self {
        Self {
            command,
            clip,
            projective: Transform3D::IDENTITY,
        }
    }

    pub const fn with_projective(mut self, projective: Transform3D) -> Self {
        self.projective = projective;
        self
    }

    /// Validate command data shared by all renderer routes.
    pub fn validate(&self) -> Result<(), RenderError> {
        self.validate_projection()?;
        self.validate_texture()
    }

    pub(crate) fn validate_texture(&self) -> Result<(), RenderError> {
        if let DrawCommand::Blit { size, texture, .. } = self.command {
            if !texture.valid_storage() {
                return Err(RenderError::InvalidTexture);
            }
            if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
                return Err(RenderError::InvalidGeometry);
            }
        }
        Ok(())
    }

    pub(crate) fn validate_projection(&self) -> Result<(), RenderError> {
        if !self.projective.is_identity()
            && matches!(
                self.command,
                DrawCommand::Fill { quad: Some(_), .. }
                    | DrawCommand::Border { quad: Some(_), .. }
                    | DrawCommand::Blit { quad: Some(_), .. }
            )
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
}

impl RegionRenderer<'_> {
    pub(crate) fn new(
        inner: &mut dyn Renderer,
        origin_x: Fixed,
        origin_y: Fixed,
    ) -> RegionRenderer<'_> {
        RegionRenderer {
            inner,
            origin_x,
            origin_y,
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

    fn explicit_quad<'data>(&self, command: &DrawCommand<'data>) -> Option<DrawCommand<'data>> {
        let translate = |point: crate::types::Point| crate::types::Point {
            x: point.x - self.origin_x,
            y: point.y - self.origin_y,
        };
        match command {
            DrawCommand::Fill {
                area,
                transform,
                quad: Some(quad),
                color,
                radius,
                opa,
            } => Some(DrawCommand::Fill {
                area: *area,
                transform: *transform,
                quad: Some(quad.map(translate)),
                color: *color,
                radius: *radius,
                opa: *opa,
            }),
            DrawCommand::Border {
                area,
                transform,
                quad: Some(quad),
                color,
                width,
                radius,
                opa,
            } => Some(DrawCommand::Border {
                area: *area,
                transform: *transform,
                quad: Some(quad.map(translate)),
                color: *color,
                width: *width,
                radius: *radius,
                opa: *opa,
            }),
            DrawCommand::Blit {
                pos,
                size,
                transform,
                quad: Some(quad),
                texture,
                opa,
                radius,
                composite,
            } => Some(DrawCommand::Blit {
                pos: *pos,
                size: *size,
                transform: *transform,
                quad: Some(quad.map(translate)),
                texture,
                opa: *opa,
                radius: *radius,
                composite: *composite,
            }),
            _ => None,
        }
    }

    fn request<'cmd, 'data>(&self, request: &DrawRequest<'cmd, 'data>) -> DrawRequest<'cmd, 'data> {
        DrawRequest::new(request.command, self.clip(request.clip)).with_projective(
            Transform3D::translate(-self.origin_x, -self.origin_y).compose(&request.projective),
        )
    }
}

impl Renderer for RegionRenderer<'_> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        if let Some(command) = self.explicit_quad(request.command) {
            return self.inner.route(
                &DrawRequest::new(&command, self.clip(request.clip))
                    .with_projective(request.projective),
            );
        }
        self.inner.route(&self.request(request))
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        if let Some(command) = self.explicit_quad(request.command) {
            return self.inner.submit(
                &DrawRequest::new(&command, self.clip(request.clip))
                    .with_projective(request.projective),
            );
        }
        self.inner.submit(&self.request(request))
    }

    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        route: RenderRoute,
    ) -> Result<(), RenderError> {
        if let Some(command) = self.explicit_quad(request.command) {
            return self.inner.submit_with_route(
                &DrawRequest::new(&command, self.clip(request.clip))
                    .with_projective(request.projective),
                route,
            );
        }
        self.inner.submit_with_route(&self.request(request), route)
    }

    fn flush(&mut self) {
        self.inner.flush();
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
        let origin_x = (self.origin_x * scale).to_int();
        let origin_y = (self.origin_y * scale).to_int();
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
    Unsupported(RenderFeature),
    InvalidGeometry,
    InvalidTexture,
    MissingWorkspace,
    InsufficientWorkspace {
        required_bytes: usize,
        capacity_bytes: usize,
    },
    ResourceLimit(RenderResource),
    BackendFailure,
}

#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
pub(crate) fn copy_packed_rgba8(
    bytes: &[u8],
    clipped: PhysicalRect,
    source_origin: (i32, i32),
    dst: &mut Texture<'_>,
) -> Result<(), RenderError> {
    if dst.format != ColorFormat::RGBA8888
        || !dst.valid_storage()
        || matches!(&dst.buf, TexBuf::Ref(_))
    {
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
    let x = usize::try_from(i32::from(clipped.x()) - source_origin.0)
        .map_err(|_| RenderError::InvalidGeometry)?;
    let y = usize::try_from(i32::from(clipped.y()) - source_origin.1)
        .map_err(|_| RenderError::InvalidGeometry)?;
    let copy_width = width.min(usize::from(dst.width).saturating_sub(x));
    let copy_height = height.min(usize::from(dst.height).saturating_sub(y));
    if copy_width == 0 || copy_height == 0 {
        return Ok(());
    }
    let copy_bytes = copy_width * 4;
    let dst_stride = dst.stride;
    let dst_bytes = dst.buf.as_mut_slice();
    for row in 0..copy_height {
        let src_start = row * row_bytes;
        let dst_start = (y + row) * dst_stride + x * 4;
        dst_bytes[dst_start..dst_start + copy_bytes]
            .copy_from_slice(&bytes[src_start..src_start + copy_bytes]);
    }
    Ok(())
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

pub trait Renderer {
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

    /// Execute one validated draw request without silently changing its
    /// semantics.
    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError>;

    /// Execute a request with the exact route returned by an earlier
    /// [`Self::route`] call for the same request.
    ///
    /// The default keeps custom renderers correct by submitting normally.
    /// Backends with measurable fallback work can override this to reuse the
    /// prepared physical region.
    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        _route: RenderRoute,
    ) -> Result<(), RenderError> {
        self.submit(request)
    }

    fn flush(&mut self);

    fn output_scale(&self) -> Fixed {
        Fixed::ONE
    }

    /// Whether this backend serves
    /// [`crate::ui::OffscreenRender`] entities through the SW
    /// pipeline: an inner `SwRenderer` over an owned buffer, blit'd
    /// back through [`Self::submit`] with `DrawCommand::Blit`. Returning
    /// `false` makes the render walker skip the offscreen path
    /// entirely and inline-render the subtree.
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
            let texture = Texture {
                buf: TexBuf::Mut(target.buf.as_mut_slice()),
                width,
                height,
                format,
                stride,
                alpha_mode,
                cache_revision: 0,
                transient: true,
            };
            let mut software = crate::render::backends::sw::SwRenderer::new(texture);
            software.viewport = crate::types::Viewport::new(width, height, scale);
            let mut local = RegionRenderer {
                inner: &mut software,
                origin_x: logical.x,
                origin_y: logical.y,
            };
            draw(&mut local)
        })
    }

    fn blur_target_region(&mut self, alpha: Fixed, region: &Rect) -> Result<(), RenderError> {
        if alpha <= Fixed::ZERO || alpha >= Fixed::ONE {
            return Ok(());
        }
        self.modify_target_region(region, &mut |texture| {
            crate::render::backends::sw::blur::iir_blur_inplace(
                texture,
                alpha,
                Rect::new(0, 0, texture.width, texture.height),
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
mod tests {
    use super::*;

    #[test]
    fn target_fallback_budget_uses_physical_rgba_extent() {
        assert_eq!(
            RenderRoute::target_readback(
                PhysicalRect::new(1, 2, 3, 4),
                Some(48),
                PlaneRequirements::CPU,
            ),
            Ok(RenderRoute::ExactFallback(FallbackRegion::from_parts(
                1, 2, 3, 4, 12
            )))
        );
        assert_eq!(
            RenderRoute::target_readback(None, None, PlaneRequirements::CPU),
            Ok(RenderRoute::ExactFallback(FallbackRegion::from_parts(
                0, 0, 0, 0, 0
            )))
        );
        assert_eq!(
            RenderRoute::target_readback(
                PhysicalRect::new(1, 2, 3, 4),
                Some(47),
                PlaneRequirements::CPU,
            ),
            Err(RenderError::InsufficientWorkspace {
                required_bytes: 48,
                capacity_bytes: 47,
            })
        );
        assert_eq!(
            RenderRoute::target_readback(
                PhysicalRect::new(1, 2, 3, 4),
                None,
                PlaneRequirements::CPU,
            ),
            Err(RenderError::MissingWorkspace)
        );
    }

    #[test]
    fn physical_fallback_route_stays_compact() {
        assert_eq!(core::mem::size_of::<PhysicalRect>(), 8);
        let word = core::mem::size_of::<usize>();
        assert_eq!(core::mem::size_of::<FallbackRegion>(), 8 + word);
        assert!(core::mem::size_of::<RenderRoute>() <= 16 + word);
    }

    #[test]
    fn scope_plan_clips_hidpi_bounds_before_charging_capacity() {
        let viewport = Viewport::new(20, 12, Fixed::from_int(2));
        assert_eq!(
            FallbackRegion::from_logical_bounds(
                Rect::new(-2, 1, 8, 8),
                viewport,
                Some(12 * 10 * 4),
            ),
            Ok(FallbackRegion::from_parts(0, 2, 12, 10, 48))
        );
        assert_eq!(
            FallbackRegion::from_logical_bounds(
                Rect::new(-2, 1, 8, 8),
                viewport,
                Some(12 * 10 * 4 - 1),
            ),
            Err(RenderError::InsufficientWorkspace {
                required_bytes: 12 * 10 * 4,
                capacity_bytes: 12 * 10 * 4 - 1,
            })
        );
    }

    #[test]
    fn target_io_failures_do_not_masquerade_as_unsupported_geometry() {
        assert_eq!(
            RenderError::from(ProjectiveDrawError::BackendFailure),
            RenderError::BackendFailure
        );
    }

    #[test]
    fn clipped_readback_preserves_pixel_rows_and_destination_offset() {
        let mut pixels = [0u8; 4 * 3 * 4];
        let mut dst = Texture::new(&mut pixels, 4, 3, ColorFormat::RGBA8888);
        let source = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        copy_packed_rgba8(
            &source,
            PhysicalRect::new(1, 1, 2, 2).unwrap(),
            (0, 0),
            &mut dst,
        )
        .unwrap();
        assert_eq!(&pixels[..20], &[0; 20]);
        assert_eq!(&pixels[20..28], &source[..8]);
        assert_eq!(&pixels[36..44], &source[8..]);
        assert_eq!(&pixels[44..], &[0; 4]);
    }

    #[test]
    fn short_readback_fails_before_modifying_destination() {
        let mut pixels = [23u8; 16];
        let mut dst = Texture::new(&mut pixels, 2, 2, ColorFormat::RGBA8888);
        assert_eq!(
            copy_packed_rgba8(
                &[1, 2, 3, 4],
                PhysicalRect::new(0, 0, 2, 2).unwrap(),
                (0, 0),
                &mut dst,
            ),
            Err(RenderError::BackendFailure)
        );
        assert_eq!(pixels, [23; 16]);
    }

    #[test]
    fn unsupported_scroll_returns_a_typed_error() {
        struct NoScroll;

        impl Renderer for NoScroll {
            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request).map(|_| ())
            }

            fn flush(&mut self) {}
        }

        let mut renderer = NoScroll;
        assert_eq!(
            renderer.scroll_target_region(&Rect::new(0, 0, 8, 8), Fixed::ONE, Fixed::ZERO),
            Err(RenderError::Unsupported(RenderFeature::ScrollBlit))
        );
    }

    #[test]
    fn unsupported_target_edit_does_not_run_callback() {
        struct NoTargetEdit;

        impl Renderer for NoTargetEdit {
            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request).map(|_| ())
            }

            fn flush(&mut self) {}
        }

        let mut renderer = NoTargetEdit;
        assert_eq!(
            renderer.modify_target_region(&Rect::new(0, 0, 8, 8), &mut |_| {
                panic!("unsupported edit must not run its callback")
            }),
            Err(RenderError::Unsupported(RenderFeature::Readback))
        );
    }

    #[test]
    fn scope_replay_preserves_global_coordinates() {
        let mut pixels = [0u8; 12 * 10 * 4];
        let texture = Texture::new(&mut pixels, 12, 10, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
        let region = FallbackRegion::from_parts(4, 3, 4, 3, 16);
        let command = DrawCommand::Fill {
            area: Rect::new(4, 3, 4, 3),
            transform: crate::types::Transform::IDENTITY,
            quad: None,
            color: crate::types::Color::rgb(20, 80, 140),
            radius: Fixed::ZERO,
            opa: 255,
        };

        let rendered = renderer
            .render_scope(region, &mut |local| {
                local.submit(&DrawRequest::new(&command, Rect::new(0, 0, 12, 10)))
            })
            .unwrap();

        assert!(rendered);
        assert_eq!(
            &pixels[(3 * 12 + 4) * 4..(3 * 12 + 4) * 4 + 4],
            &[20, 80, 140, 255]
        );
        assert_eq!(
            &pixels[(5 * 12 + 7) * 4..(5 * 12 + 7) * 4 + 4],
            &[20, 80, 140, 255]
        );
        assert_eq!(&pixels[(3 * 12 + 3) * 4..(3 * 12 + 3) * 4 + 4], &[0; 4]);
        assert_eq!(&pixels[(6 * 12 + 4) * 4..(6 * 12 + 4) * 4 + 4], &[0; 4]);
    }

    #[test]
    fn scope_replay_rebases_explicit_quads_once() {
        let mut pixels = [0u8; 12 * 10 * 4];
        let texture = Texture::new(&mut pixels, 12, 10, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
        let region = FallbackRegion::from_parts(4, 3, 4, 3, 16);
        let command = DrawCommand::Fill {
            area: Rect::new(4, 3, 4, 3),
            transform: crate::types::Transform::IDENTITY,
            quad: Some([
                crate::types::Point::new(4, 3),
                crate::types::Point::new(8, 3),
                crate::types::Point::new(8, 6),
                crate::types::Point::new(4, 6),
            ]),
            color: crate::types::Color::rgb(200, 70, 30),
            radius: Fixed::ZERO,
            opa: 255,
        };

        renderer
            .render_scope(region, &mut |local| {
                local.submit(&DrawRequest::new(&command, Rect::new(0, 0, 12, 10)))
            })
            .unwrap();

        assert_eq!(
            &pixels[(4 * 12 + 5) * 4..(4 * 12 + 5) * 4 + 4],
            &[200, 70, 30, 255]
        );
        assert_eq!(&pixels[(4 * 12 + 3) * 4..(4 * 12 + 3) * 4 + 4], &[0; 4]);
    }

    #[test]
    fn request_keeps_command_data_borrow_independent_of_command_borrow() {
        let clip = Rect::new(0, 0, 12, 12);
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: clip,
        };
        let projection =
            Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400));
        let request = DrawRequest::new(&command, clip).with_projective(projection);
        assert_eq!(request.clip, clip);
        assert_eq!(request.projective, projection);
        assert!(matches!(request.command, DrawCommand::ApplyBlur { .. }));
    }

    #[test]
    fn request_rejects_explicit_quad_with_shared_projection() {
        let clip = Rect::new(0, 0, 12, 12);
        let command = DrawCommand::Fill {
            area: clip,
            transform: crate::types::Transform::IDENTITY,
            quad: Some([
                crate::types::Point::new(0, 0),
                crate::types::Point::new(12, 0),
                crate::types::Point::new(12, 12),
                crate::types::Point::new(0, 12),
            ]),
            color: crate::types::Color::rgb(20, 30, 40),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let request = DrawRequest::new(&command, clip).with_projective(
            Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400)),
        );
        assert_eq!(
            request.validate_projection(),
            Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
        );
    }

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.route(request).map(|_| ())
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn default_capabilities_are_disabled() {
        let r = NoopRenderer;
        assert!(!r.supports_offscreen());
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: Rect::new(0, 0, 1, 1),
        };
        assert_eq!(
            r.route(&DrawRequest::new(&command, Rect::new(0, 0, 1, 1))),
            Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
    }

    #[test]
    fn default_route_rejects_affine_and_projective_requests() {
        let mut renderer = NoopRenderer;
        let clip = Rect::new(0, 0, 1, 1);
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: clip,
        };
        assert_eq!(
            renderer.submit(&DrawRequest::new(&command, clip)),
            Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
        assert_eq!(
            renderer.submit(&DrawRequest::new(&command, clip).with_projective(
                Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400)),
            )),
            Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
        );
    }

    #[test]
    fn submit_refuses_unknown_route_before_draw() {
        struct CountingRenderer(usize);

        impl Renderer for CountingRenderer {
            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request)?;
                self.0 += 1;
                Ok(())
            }

            fn flush(&mut self) {}
        }

        let clip = Rect::new(0, 0, 1, 1);
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: clip,
        };
        let mut renderer = CountingRenderer(0);
        assert_eq!(
            renderer.submit(&DrawRequest::new(&command, clip)),
            Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
        assert_eq!(renderer.0, 0);
    }
}
