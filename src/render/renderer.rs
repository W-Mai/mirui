use crate::types::{Fixed, Rect, Transform3D};

use super::command::{CompositeMode, DrawCommand};
use super::texture::ColorFormat;
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use super::texture::{TexBuf, Texture};

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
    ExactFallback { required_bytes: usize },
}

/// Semantic operation that a backend cannot execute exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderFeature {
    AffineGeometry,
    ProjectiveGeometry,
    PathClip,
    PathStroke,
    FillRule,
    GradientPaint,
    StrokeStyle,
    Blur,
    RoundedFill,
    RoundedBlit,
    BlitOpacity,
    Composite(CompositeMode),
    TextureFormat(ColorFormat),
    Readback,
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
    clipped: Rect,
    source_origin: (i32, i32),
    dst: &mut Texture<'_>,
) -> Result<(), RenderError> {
    if dst.format != ColorFormat::RGBA8888
        || !dst.valid_storage()
        || matches!(&dst.buf, TexBuf::Ref(_))
    {
        return Err(RenderError::InvalidTexture);
    }
    let width = usize::try_from(clipped.w.to_int()).map_err(|_| RenderError::InvalidGeometry)?;
    let height = usize::try_from(clipped.h.to_int()).map_err(|_| RenderError::InvalidGeometry)?;
    let row_bytes = width.checked_mul(4).ok_or(RenderError::InvalidGeometry)?;
    let required = row_bytes
        .checked_mul(height)
        .ok_or(RenderError::InvalidGeometry)?;
    if bytes.len() < required {
        return Err(RenderError::BackendFailure);
    }
    let x = usize::try_from(clipped.x.to_int() - source_origin.0)
        .map_err(|_| RenderError::InvalidGeometry)?;
    let y = usize::try_from(clipped.y.to_int() - source_origin.1)
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

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        request.validate_projection()?;
        self.route(request)?;
        if request.projective.is_identity() {
            self.draw(request.command, &request.clip);
            Ok(())
        } else {
            self.draw_projective(request.command, &request.clip, &request.projective)
                .map_err(RenderError::from)
        }
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect);

    /// Draw one command after its affine transform under `transform`.
    fn draw_projective(
        &mut self,
        cmd: &DrawCommand,
        clip: &Rect,
        transform: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if transform.is_identity() {
            self.draw(cmd, clip);
            Ok(())
        } else {
            Err(ProjectiveDrawError::Unsupported)
        }
    }

    /// Validate one clipped command under a homography without drawing it.
    /// Backends with a bounded software path use `clip` to report the exact
    /// target capacity required before any command is drawn.
    fn preflight_projective(
        &self,
        _command: &DrawCommand,
        _clip: &Rect,
        transform: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if transform.is_identity() {
            Ok(())
        } else {
            Err(ProjectiveDrawError::Unsupported)
        }
    }

    fn flush(&mut self);

    fn output_scale(&self) -> Fixed {
        Fixed::ONE
    }

    /// Whether this backend serves
    /// [`crate::ui::OffscreenRender`] entities through the SW
    /// pipeline: an inner `SwRenderer` over an owned buffer, blit'd
    /// back via [`Self::draw`] with `DrawCommand::Blit`. Returning
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
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> Result<bool, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    /// Backends that defer draws (currently only `wgpu`) need to submit
    /// pending work before a `sample_target_region` / `read_target_region`
    /// call can see this frame's pixels. Eager backends keep the default
    /// no-op. Callers running readback should invoke this immediately
    /// before the read; idempotent if no work is pending.
    fn prepare_readback(&mut self, _src: &Rect) {}

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
        copy_packed_rgba8(&source, Rect::new(1, 1, 2, 2), (0, 0), &mut dst).unwrap();
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
            copy_packed_rgba8(&[1, 2, 3, 4], Rect::new(0, 0, 2, 2), (0, 0), &mut dst),
            Err(RenderError::BackendFailure)
        );
        assert_eq!(pixels, [23; 16]);
    }

    #[test]
    fn unsupported_scroll_returns_a_typed_error() {
        struct NoScroll;

        impl Renderer for NoScroll {
            fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {}

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
            fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {}

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
        fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {}
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
        assert_eq!(
            r.preflight_projective(
                &command,
                &Rect::new(0, 0, 1, 1),
                &Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400),),
            ),
            Err(ProjectiveDrawError::Unsupported)
        );
    }

    #[test]
    fn default_projective_path_accepts_only_identity() {
        let mut renderer = NoopRenderer;
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: Rect::new(0, 0, 1, 1),
        };
        assert_eq!(
            renderer.draw_projective(&command, &Rect::new(0, 0, 1, 1), &Transform3D::IDENTITY),
            Ok(())
        );
        assert_eq!(
            renderer.draw_projective(
                &command,
                &Rect::new(0, 0, 1, 1),
                &Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400),),
            ),
            Err(ProjectiveDrawError::Unsupported)
        );
    }

    #[test]
    fn submit_refuses_unknown_route_before_draw() {
        struct CountingRenderer(usize);

        impl Renderer for CountingRenderer {
            fn draw(&mut self, _: &DrawCommand, _: &Rect) {
                self.0 += 1;
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
