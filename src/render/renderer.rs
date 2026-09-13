use crate::types::{Fixed, Rect, Transform3D};

use super::command::{CompositeMode, DrawCommand};
use super::texture::ColorFormat;

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
    MissingWorkspace,
    InsufficientWorkspace {
        required_bytes: usize,
        capacity_bytes: usize,
    },
    ResourceLimit(RenderResource),
    BackendFailure,
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

    /// Copy a logical-pixel rect from the current target into `dst`.
    /// `dst` is sized in physical pixels by the caller — usually
    /// because they already own the buffer (offscreen pre-seed). A
    /// backend that returns `Some` from [`Self::offscreen_format`]
    /// must override this. Effects that just want to grab a region
    /// of the framebuffer should use [`Self::sample_target_region`]
    /// instead.
    fn read_target_region(&self, _src: &Rect, _dst: &mut crate::render::texture::Texture) {
        unimplemented!("Renderer::read_target_region not implemented for this backend")
    }

    /// Logical-pixel `src` in, physical-resolution texture out.
    /// Backends that can't read their own target should return `None`;
    /// the default panics so a missing override is loud, not silent.
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Option<crate::render::texture::Texture<'static>> {
        unimplemented!("Renderer::sample_target_region not implemented for this backend")
    }

    /// Hand the closure a mutable `Texture` view over physical-pixel
    /// framebuffer bytes inside `src`, skipping the alloc-and-blit-back
    /// round-trip that `sample_target_region` + `draw(Blit)` would do.
    /// Returns `true` when the closure ran. Default panics so a missing
    /// override is loud.
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        unimplemented!("Renderer::modify_target_region not implemented for this backend")
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

    /// Shift `area`'s pixels by `(dx, dy)` *logical* pixels (memmove,
    /// no draw, no flush). Pixels evicted from `area` are dropped; the
    /// caller repaints the newly exposed strip(s). Default panics;
    /// backends opt in by overriding both this and
    /// `supports_scroll_blit()`.
    fn scroll_target_region(&mut self, _area: &Rect, _dx: Fixed, _dy: Fixed) {
        unimplemented!("Renderer::scroll_target_region not implemented for this backend")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
