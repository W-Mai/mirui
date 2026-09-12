use alloc::boxed::Box;

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::backends::sw::SwRenderer;
#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::command::DrawCommand;
#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::renderer::{ProjectiveDrawError, Renderer};
#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::render::texture::{ColorFormat, Texture};
#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
use crate::types::{Fixed, Rect, Transform3D, Viewport};

/// Fixed-capacity target used by GPU backends for exact software-rendered
/// projective glyphs. The backing storage is supplied once and never grows.
pub struct ProjectiveGlyphFallback {
    target: Box<[u8]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32"),
    test
))]
pub(crate) struct ProjectiveFallbackPlan {
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
    required_bytes: usize,
    logical_origin_x: Fixed,
    logical_origin_y: Fixed,
}

impl ProjectiveGlyphFallback {
    /// Uses `target` as both the clipped RGBA8888 target and rendering
    /// workspace.
    pub fn new(target: impl Into<Box<[u8]>>) -> Self {
        Self {
            target: target.into(),
        }
    }

    /// Returns the hard byte budget available to projective glyph rendering.
    pub fn capacity(&self) -> usize {
        self.target.len()
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
        if !matches!(
            command,
            DrawCommand::GlyphRun { .. } | DrawCommand::PosedGlyphRun { .. }
        ) {
            return Err(ProjectiveDrawError::Unsupported);
        }

        let mut empty = [];
        let mut validator = SwRenderer::new(Texture::new(&mut empty, 0, 0, ColorFormat::RGBA8888));
        validator.viewport = viewport;
        validator.preflight_projective(command, clip, projective)?;

        let (physical_width, physical_height) = viewport.physical_size();
        let (x0, y0, x1, y1) = viewport.rect_to_physical_pixel_bounds(*clip);
        let x0 = x0.clamp(0, i32::from(physical_width));
        let y0 = y0.clamp(0, i32::from(physical_height));
        let x1 = x1.clamp(x0, i32::from(physical_width));
        let y1 = y1.clamp(y0, i32::from(physical_height));
        let width = u16::try_from(x1 - x0).map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        let height = u16::try_from(y1 - y0).map_err(|_| ProjectiveDrawError::InvalidProjection)?;
        let required_bytes = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(ColorFormat::RGBA8888.bytes_per_pixel()))
            .ok_or(ProjectiveDrawError::InvalidProjection)?;
        if required_bytes > self.target.len() {
            return Err(ProjectiveDrawError::InsufficientFallbackStorage {
                required_bytes,
                capacity_bytes: self.target.len(),
            });
        }

        Ok(ProjectiveFallbackPlan {
            x: x0,
            y: y0,
            width,
            height,
            required_bytes,
            logical_origin_x: Fixed::from_int(x0) / viewport.scale(),
            logical_origin_y: Fixed::from_int(y0) / viewport.scale(),
        })
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn target_mut(&mut self, plan: ProjectiveFallbackPlan) -> &mut [u8] {
        &mut self.target[..plan.required_bytes]
    }

    #[cfg(any(feature = "sdl-gpu", test))]
    pub(crate) fn target(&self, plan: ProjectiveFallbackPlan) -> &[u8] {
        &self.target[..plan.required_bytes]
    }

    #[cfg(any(
        feature = "sdl-gpu",
        all(feature = "web-canvas", target_arch = "wasm32"),
        test
    ))]
    pub(crate) fn render(
        &mut self,
        plan: ProjectiveFallbackPlan,
        command: &DrawCommand<'_>,
        projective: &Transform3D,
        viewport: Viewport,
    ) -> Result<(), ProjectiveDrawError> {
        if plan.required_bytes == 0 {
            return Ok(());
        }
        let scale = viewport.scale();
        let local_projective =
            Transform3D::translate(-plan.logical_origin_x, -plan.logical_origin_y)
                .compose(projective);
        let local_clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from(plan.width) / scale,
            h: Fixed::from(plan.height) / scale,
        };
        let texture = Texture::new(
            &mut self.target[..plan.required_bytes],
            plan.width,
            plan.height,
            ColorFormat::RGBA8888,
        );
        let mut renderer = SwRenderer::new(texture);
        renderer.viewport = Viewport::new(plan.width, plan.height, scale);
        renderer.draw_projective(command, &local_clip, &local_projective)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::Font;
    use crate::types::{Color, Point, Transform};
    use textflow::shaping::{FlowPoint, GlyphId, PositionedGlyph};

    fn glyph_command<'a>(glyphs: &'a [PositionedGlyph], font: &'a Font) -> DrawCommand<'a> {
        DrawCommand::GlyphRun {
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            glyphs,
            font,
            color: Color::rgb(255, 255, 255),
            opa: 255,
        }
    }

    #[test]
    fn clip_is_the_complete_capacity_budget() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 0 },
        )];
        let font = Font::bitmap_8x8();
        let command = glyph_command(&glyphs, &font);
        let fallback = ProjectiveGlyphFallback::new(alloc::vec![0; 15].into_boxed_slice());
        let transform =
            Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));

        assert_eq!(
            fallback.plan(
                &command,
                &Rect::new(1, 2, 2, 2),
                &transform,
                Viewport::new(8, 8, Fixed::ONE),
            ),
            Err(ProjectiveDrawError::InsufficientFallbackStorage {
                required_bytes: 16,
                capacity_bytes: 15,
            })
        );
    }

    #[test]
    fn rendering_reuses_seeded_target_without_growth() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 8 << 8 },
        )];
        let font = Font::bitmap_8x8();
        let command = glyph_command(&glyphs, &font);
        let mut fallback = ProjectiveGlyphFallback::new(alloc::vec![7; 8 * 8 * 4]);
        let viewport = Viewport::new(8, 8, Fixed::ONE);
        let transform = Transform3D::rotate_y_perspective(Fixed::from_int(5), Fixed::from_int(400));
        let plan = fallback
            .plan(&command, &Rect::new(0, 0, 8, 8), &transform, viewport)
            .unwrap();
        let capacity = fallback.capacity();
        fallback.target_mut(plan).fill(7);

        fallback
            .render(plan, &command, &transform, viewport)
            .unwrap();

        assert_eq!(fallback.capacity(), capacity);
        assert!(fallback.target(plan).iter().any(|byte| *byte != 7));
    }

    #[test]
    fn clipped_target_matches_the_same_region_of_the_software_backend() {
        const WIDTH: usize = 16;
        const HEIGHT: usize = 12;
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint {
                x: 4 << 8,
                y: 9 << 8,
            },
        )];
        let font = Font::bitmap_8x8();
        let command = glyph_command(&glyphs, &font);
        let viewport = Viewport::new(WIDTH as u16, HEIGHT as u16, Fixed::ONE);
        let clip = Rect::new(3, 2, 9, 8);
        let transform = Transform3D::rotate_y_perspective(Fixed::from_int(8), Fixed::from_int(400));
        let mut expected = alloc::vec![19; WIDTH * HEIGHT * 4];
        let mut actual = alloc::vec![0; 9 * 8 * 4];
        for row in 0..8 {
            let source = ((row + 2) * WIDTH + 3) * 4;
            let target = row * 9 * 4;
            actual[target..target + 9 * 4].copy_from_slice(&expected[source..source + 9 * 4]);
        }

        let mut direct = SwRenderer::new(Texture::new(
            &mut expected,
            WIDTH as u16,
            HEIGHT as u16,
            ColorFormat::RGBA8888,
        ));
        direct.viewport = viewport;
        direct.draw_projective(&command, &clip, &transform).unwrap();

        let mut fallback = ProjectiveGlyphFallback::new(actual);
        let plan = fallback
            .plan(&command, &clip, &transform, viewport)
            .unwrap();
        fallback
            .render(plan, &command, &transform, viewport)
            .unwrap();

        for row in 0..8 {
            let expected_start = ((row + 2) * WIDTH + 3) * 4;
            let actual_start = row * 9 * 4;
            assert_eq!(
                &fallback.target(plan)[actual_start..actual_start + 9 * 4],
                &expected[expected_start..expected_start + 9 * 4],
            );
        }
    }
}
