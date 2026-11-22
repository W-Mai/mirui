use crate::render::scratch::{PlaneError, PlaneRequirements};
use alloc::boxed::Box;

#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::backends::sw::SwRenderer;
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::command::DrawCommand;
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::renderer::{ProjectiveDrawError, RenderError, Renderer};
use crate::render::texture::AlignedBytes;
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::texture::{ColorFormat, Texture};
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::types::{Fixed, PhysicalRect, Point, Rect, Transform3D, Viewport};

#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
fn affine_from_homography(value: Transform3D) -> Option<crate::types::Transform> {
    if !value.m20.is_zero() || !value.m21.is_zero() || value.m22 != crate::types::Fixed64::ONE {
        return None;
    }
    Some(crate::types::Transform {
        m00: value.m00.to_fixed(),
        m01: value.m01.to_fixed(),
        tx: value.m02.to_fixed(),
        m10: value.m10.to_fixed(),
        m11: value.m11.to_fixed(),
        ty: value.m12.to_fixed(),
    })
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu"))]
pub(crate) fn transformed_stroke(request: &crate::render::renderer::DrawRequest<'_, '_>) -> bool {
    matches!(
        request.command,
        DrawCommand::Line { .. } | DrawCommand::Arc { .. }
    ) && (!request.projective.is_identity()
        || !matches!(
            request.command.transform().classify(),
            crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
        ))
}

/// Fixed-capacity target for software-rendered projective commands.
/// The backing storage is supplied once and never grows.
pub struct ProjectiveFallback<S = Box<[u8]>> {
    target: S,
    requirements: PlaneRequirements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
pub(crate) struct ProjectiveFallbackPlan {
    region: crate::render::renderer::FallbackRegion,
    logical_origin: Point,
}

#[cfg(any(
    feature = "sdl-gpu",
    feature = "wgpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
impl ProjectiveFallbackPlan {
    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) const fn region(self) -> crate::render::renderer::FallbackRegion {
        self.region
    }

    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn from_region(
        region: crate::render::renderer::FallbackRegion,
        viewport: Viewport,
        requirements: PlaneRequirements,
    ) -> Result<Self, ProjectiveDrawError> {
        let (physical_width, physical_height) = viewport.physical_size();
        if region.rect().right() > physical_width || region.rect().bottom() > physical_height {
            return Err(ProjectiveDrawError::InvalidProjection);
        }
        region
            .plane_layout(requirements)
            .map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        Ok(Self {
            region,
            logical_origin: Point {
                x: Fixed::from(region.x()) / viewport.scale(),
                y: Fixed::from(region.y()) / viewport.scale(),
            },
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) const fn x(self) -> u16 {
        self.region.x()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) const fn y(self) -> u16 {
        self.region.y()
    }

    pub(crate) const fn width(self) -> u16 {
        self.region.width()
    }

    pub(crate) const fn height(self) -> u16 {
        self.region.height()
    }

    pub(crate) const fn required_bytes(self) -> usize {
        self.region.required_bytes()
    }
}

impl ProjectiveFallback<Box<[u8]>> {
    /// Uses `target` as both the clipped RGBA8888 target and rendering
    /// workspace.
    pub fn new(target: impl Into<Box<[u8]>>) -> Self {
        Self {
            target: target.into(),
            requirements: PlaneRequirements::CPU,
        }
    }
}

impl ProjectiveFallback<AlignedBytes> {
    pub fn aligned(
        capacity_bytes: usize,
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        let target = AlignedBytes::zeroed(capacity_bytes, requirements.address_alignment())
            .ok_or(PlaneError::Overflow)?;
        Ok(Self {
            target,
            requirements,
        })
    }
}

impl<'a> ProjectiveFallback<&'a mut [u8]> {
    /// Use a caller-owned fixed-capacity target without allocating.
    pub fn borrowed(target: &'a mut [u8]) -> Self {
        Self {
            target,
            requirements: PlaneRequirements::CPU,
        }
    }

    pub fn borrowed_with(
        target: &'a mut [u8],
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        requirements.validate_address(target)?;
        Ok(Self {
            target,
            requirements,
        })
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> ProjectiveFallback<S> {
    pub fn with_requirements(
        target: S,
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        requirements.validate_address(target.as_ref())?;
        Ok(Self {
            target,
            requirements,
        })
    }

    /// Returns the hard byte budget available to projective rendering.
    pub fn capacity(&self) -> usize {
        self.target.as_ref().len()
    }

    pub const fn requirements(&self) -> PlaneRequirements {
        self.requirements
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn plan(
        &self,
        command: &DrawCommand<'_>,
        clip: &Rect,
        projective: &Transform3D,
        viewport: Viewport,
    ) -> Result<ProjectiveFallbackPlan, ProjectiveDrawError> {
        Self::measure(
            self.target.as_ref().len(),
            command,
            clip,
            projective,
            viewport,
            self.requirements,
        )
    }

    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn measure(
        capacity_bytes: usize,
        command: &DrawCommand<'_>,
        clip: &Rect,
        projective: &Transform3D,
        viewport: Viewport,
        requirements: PlaneRequirements,
    ) -> Result<ProjectiveFallbackPlan, ProjectiveDrawError> {
        let mut pixel = [0; 4];
        let mut validator = SwRenderer::new(
            Texture::new(&mut pixel, 1, 1, ColorFormat::RGBA8888).expect("one RGBA pixel"),
        );
        validator.viewport = viewport;
        validator.preflight_projective(command, clip, projective)?;
        if projective.is_identity()
            && matches!(
                command,
                DrawCommand::FillPath { .. } | DrawCommand::StrokePath { .. }
            )
        {
            validator
                .route(&crate::render::renderer::DrawRequest::new(command, *clip))
                .map_err(|error| match error {
                    crate::render::renderer::RenderError::InvalidGeometry => {
                        ProjectiveDrawError::InvalidProjection
                    }
                    _ => ProjectiveDrawError::Unsupported,
                })?;
        }

        let logical = projective.compose(&Transform3D::from_affine(command.transform()));
        let visual_bounds = match command {
            DrawCommand::Fill { area, .. } | DrawCommand::Border { area, .. } => {
                let quad = logical
                    .apply_rect(*area)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                let bounds = Rect::bounding_quad(&quad);
                if let DrawCommand::Border { width, .. } = command {
                    bounds.inflate(*width)
                } else {
                    bounds
                }
            }
            DrawCommand::Blit {
                quad: Some(quad),
                texture,
                ..
            } => {
                if !projective.is_identity() {
                    return Err(ProjectiveDrawError::Unsupported);
                }
                let source = Rect::new(0, 0, texture.width, texture.height);
                let projection = Transform3D::from_quad(source, quad)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                if projection.inverse().is_none() || projection.apply_rect(source).is_none() {
                    return Err(ProjectiveDrawError::InvalidProjection);
                }
                Rect::bounding_quad(quad)
            }
            DrawCommand::Blit { pos, size, .. } => {
                let quad = logical
                    .apply_rect(Rect::new(pos.x, pos.y, size.x, size.y))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                Rect::bounding_quad(&quad)
            }
            DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                ..
            } => {
                let physical = Transform3D::from_affine(viewport.as_transform()).compose(&logical);
                let output_ppem = crate::render::font::output_ppem(
                    font.size.max(1),
                    physical.raster_scale_at(*pos),
                );
                font.glyph_run_ink_bounds(glyphs, *pos, *transform, output_ppem)
                    .and_then(|bounds| projective.apply_rect(bounds))
                    .map(|quad| Rect::bounding_quad(&quad))
                    .unwrap_or(*clip)
            }
            DrawCommand::PosedGlyphRun {
                pos,
                transform,
                glyphs,
                font,
                ..
            } => {
                let physical =
                    Transform3D::from_affine(viewport.as_transform()).compose(projective);
                let output_ppem = crate::render::font::output_ppem(
                    font.size.max(1),
                    physical.raster_scale_at(*pos),
                );
                glyphs
                    .ink_bounds_for_output(font, *pos, *transform, output_ppem)
                    .and_then(|bounds| projective.apply_rect(bounds))
                    .map(|quad| Rect::bounding_quad(&quad))
                    .unwrap_or(*clip)
            }
            DrawCommand::FillPath { path, .. } => {
                let bounds = path.bbox().ok_or(ProjectiveDrawError::InvalidProjection)?;
                logical
                    .apply_rect(bounds)
                    .map(|quad| Rect::bounding_quad(&quad))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?
            }
            DrawCommand::StrokePath {
                path,
                width,
                miter_limit,
                ..
            } => {
                let bounds = path.bbox().ok_or(ProjectiveDrawError::InvalidProjection)?;
                let extent = *width / 2 * (*miter_limit).max(Fixed::ONE);
                logical
                    .apply_rect(bounds.inflate(extent))
                    .map(|quad| Rect::bounding_quad(&quad))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?
            }
            DrawCommand::Line { p1, p2, width, .. } => {
                let bounds = Rect::new(
                    p1.x.min(p2.x),
                    p1.y.min(p2.y),
                    (p2.x - p1.x).abs(),
                    (p2.y - p1.y).abs(),
                )
                .inflate((*width).max(Fixed::ZERO));
                logical
                    .apply_rect(bounds)
                    .map(|quad| Rect::bounding_quad(&quad))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?
            }
            DrawCommand::Arc {
                center,
                radius,
                width,
                ..
            } => {
                let extent = (*radius).max(Fixed::ZERO) + (*width).max(Fixed::ZERO);
                logical
                    .apply_rect(Rect::new(
                        center.x - extent,
                        center.y - extent,
                        extent * 2,
                        extent * 2,
                    ))
                    .map(|quad| Rect::bounding_quad(&quad))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?
            }
            _ => return Err(ProjectiveDrawError::Unsupported),
        };
        let draw_bounds = clip
            .intersect(&visual_bounds.inflate(Fixed::from_int(2) / viewport.scale()))
            .unwrap_or(Rect::ZERO);
        let rect = viewport
            .physical_rect(draw_bounds)
            .unwrap_or(PhysicalRect::EMPTY);
        let region = crate::render::renderer::FallbackRegion::rgba8(rect, requirements)
            .map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        let required_bytes = region.required_bytes();
        if required_bytes > capacity_bytes {
            return Err(ProjectiveDrawError::InsufficientFallbackStorage {
                required_bytes,
                capacity_bytes,
            });
        }

        let plan = ProjectiveFallbackPlan {
            region,
            logical_origin: Point {
                x: Fixed::from(region.x()) / viewport.scale(),
                y: Fixed::from(region.y()) / viewport.scale(),
            },
        };
        let local_projective =
            Transform3D::translate(-plan.logical_origin.x, -plan.logical_origin.y)
                .compose(projective);
        let local_clip = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from(plan.width()) / viewport.scale(),
            Fixed::from(plan.height()) / viewport.scale(),
        );
        if !matches!(
            command,
            DrawCommand::Blit { quad: Some(_), .. }
                | DrawCommand::FillPath { .. }
                | DrawCommand::StrokePath { .. }
        ) {
            validator.preflight_projective(command, &local_clip, &local_projective)?;
        }
        Ok(plan)
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn target_mut(&mut self, plan: ProjectiveFallbackPlan) -> &mut [u8] {
        &mut self.target.as_mut()[..plan.required_bytes()]
    }

    #[cfg(test)]
    pub(crate) fn target(&self, plan: ProjectiveFallbackPlan) -> &[u8] {
        &self.target.as_ref()[..plan.required_bytes()]
    }

    #[cfg(any(all(feature = "web-canvas", target_arch = "wasm32"), test))]
    pub(crate) fn copy_from_packed_rgba8(
        &mut self,
        plan: ProjectiveFallbackPlan,
        source: &[u8],
    ) -> Result<(), ProjectiveDrawError> {
        let packed_stride = usize::from(plan.width()) * 4;
        let packed_len = packed_stride
            .checked_mul(usize::from(plan.height()))
            .ok_or(ProjectiveDrawError::InvalidProjection)?;
        if source.len() != packed_len {
            return Err(ProjectiveDrawError::BackendFailure);
        }
        let stride = plan.region().stride_bytes();
        let target = self.target_mut(plan);
        for row in 0..usize::from(plan.height()) {
            let source_start = row * packed_stride;
            let target_start = row * stride;
            target[target_start..target_start + packed_stride]
                .copy_from_slice(&source[source_start..source_start + packed_stride]);
        }
        Ok(())
    }

    #[cfg(any(all(feature = "web-canvas", target_arch = "wasm32"), test))]
    pub(crate) fn packed_rgba8_mut(
        &mut self,
        plan: ProjectiveFallbackPlan,
    ) -> Result<&mut [u8], ProjectiveDrawError> {
        let packed_stride = usize::from(plan.width()) * 4;
        let packed_len = packed_stride
            .checked_mul(usize::from(plan.height()))
            .ok_or(ProjectiveDrawError::InvalidProjection)?;
        let stride = plan.region().stride_bytes();
        let target = self.target_mut(plan);
        if stride != packed_stride {
            for row in 1..usize::from(plan.height()) {
                let source_start = row * stride;
                let target_start = row * packed_stride;
                target.copy_within(source_start..source_start + packed_stride, target_start);
            }
        }
        Ok(&mut target[..packed_len])
    }

    #[cfg(any(
        feature = "sdl-gpu",
        feature = "wgpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn render(
        &mut self,
        plan: ProjectiveFallbackPlan,
        command: &DrawCommand<'_>,
        projective: &Transform3D,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        if plan.required_bytes() == 0 {
            return Ok(());
        }
        let scale = viewport.scale();
        let local_projective =
            Transform3D::translate(-plan.logical_origin.x, -plan.logical_origin.y)
                .compose(projective);
        let local_clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from(plan.width()) / scale,
            h: Fixed::from(plan.height()) / scale,
        };
        let layout = plan
            .region
            .plane_layout(self.requirements)
            .map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        let mut plane = layout
            .bind(&mut self.target.as_mut()[..plan.required_bytes()])
            .map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        let mut texture = plane.texture();
        texture.alpha_mode = crate::render::texture::AlphaMode::Blend;
        let mut renderer = SwRenderer::new(texture);
        renderer.viewport = Viewport::new(plan.width(), plan.height(), scale);
        if matches!(command, DrawCommand::Line { .. } | DrawCommand::Arc { .. })
            || (!projective.is_identity()
                && matches!(
                    command,
                    DrawCommand::FillPath { .. } | DrawCommand::StrokePath { .. }
                ))
        {
            if projective.is_identity() {
                let mut local = crate::render::renderer::RegionRenderer::new(
                    &mut renderer,
                    (plan.region.x(), plan.region.y()),
                );
                return local.draw_request(&crate::render::renderer::DrawRequest::new(
                    command,
                    Rect::new(
                        plan.logical_origin.x,
                        plan.logical_origin.y,
                        local_clip.w,
                        local_clip.h,
                    ),
                ));
            }
            let physical = Transform3D::translate(
                -Fixed::from(plan.region.x()),
                -Fixed::from(plan.region.y()),
            )
            .compose(&Transform3D::from_affine(viewport.as_transform()).compose(projective));
            renderer.viewport = Viewport::new(plan.width(), plan.height(), Fixed::ONE);
            return renderer.draw_projective(
                command,
                &Rect::new(0, 0, plan.width(), plan.height()),
                &physical,
            );
        }
        if let DrawCommand::Blit {
            pos,
            size,
            transform,
            quad: Some(quad),
            texture,
            opa,
            radius,
            composite,
        } = command
            && projective.is_identity()
        {
            let local_quad = quad.map(|point| crate::types::Point {
                x: point.x - plan.logical_origin.x,
                y: point.y - plan.logical_origin.y,
            });
            renderer.draw(
                &DrawCommand::Blit {
                    pos: *pos,
                    size: *size,
                    transform: *transform,
                    quad: Some(local_quad),
                    texture,
                    opa: *opa,
                    radius: *radius,
                    composite: *composite,
                },
                &local_clip,
            );
            Renderer::flush(&mut renderer)
        } else if let DrawCommand::FillPath {
            path,
            transform,
            paint,
            opa,
            fill_rule,
        } = command
        {
            let local = Transform3D::translate(-plan.logical_origin.x, -plan.logical_origin.y)
                .compose(projective)
                .compose(&Transform3D::from_affine(*transform));
            let affine = affine_from_homography(local).ok_or(ProjectiveDrawError::Unsupported)?;
            renderer.draw_request(&crate::render::renderer::DrawRequest::new(
                &DrawCommand::FillPath {
                    path,
                    transform: affine,
                    paint,
                    opa: *opa,
                    fill_rule: *fill_rule,
                },
                local_clip,
            ))
        } else if let DrawCommand::StrokePath {
            path,
            transform,
            paint,
            width,
            opa,
            line_cap,
            line_join,
            miter_limit,
            dash,
        } = command
        {
            let local = Transform3D::translate(-plan.logical_origin.x, -plan.logical_origin.y)
                .compose(projective)
                .compose(&Transform3D::from_affine(*transform));
            let affine = affine_from_homography(local).ok_or(ProjectiveDrawError::Unsupported)?;
            renderer.draw_request(&crate::render::renderer::DrawRequest::new(
                &DrawCommand::StrokePath {
                    path,
                    transform: affine,
                    paint,
                    width: *width,
                    opa: *opa,
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                    dash,
                },
                local_clip,
            ))
        } else {
            renderer.draw_projective(command, &local_clip, &local_projective)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/projective_fallback.rs"]
mod tests;
