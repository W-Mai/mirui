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

/// Fixed-capacity target for software-rendered projective commands.
/// The backing storage is supplied once and never grows.
pub struct ProjectiveFallback {
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

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
impl ProjectiveFallbackPlan {
    pub(crate) const fn required_bytes(self) -> usize {
        self.required_bytes
    }
}

impl ProjectiveFallback {
    /// Uses `target` as both the clipped RGBA8888 target and rendering
    /// workspace.
    pub fn new(target: impl Into<Box<[u8]>>) -> Self {
        Self {
            target: target.into(),
        }
    }

    /// Returns the hard byte budget available to projective rendering.
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
        let mut empty = [];
        let mut validator = SwRenderer::new(Texture::new(&mut empty, 0, 0, ColorFormat::RGBA8888));
        validator.viewport = viewport;
        validator.preflight_projective(command, clip, projective)?;

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
            _ => return Err(ProjectiveDrawError::Unsupported),
        };
        let draw_bounds = clip
            .intersect(&visual_bounds.inflate(Fixed::from_int(2) / viewport.scale()))
            .unwrap_or(Rect::ZERO);
        let (physical_width, physical_height) = viewport.physical_size();
        let (x0, y0, x1, y1) = viewport.rect_to_physical_pixel_bounds(draw_bounds);
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

        let plan = ProjectiveFallbackPlan {
            x: x0,
            y: y0,
            width,
            height,
            required_bytes,
            logical_origin_x: Fixed::from_int(x0) / viewport.scale(),
            logical_origin_y: Fixed::from_int(y0) / viewport.scale(),
        };
        let local_projective =
            Transform3D::translate(-plan.logical_origin_x, -plan.logical_origin_y)
                .compose(projective);
        let local_clip = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from(plan.width) / viewport.scale(),
            Fixed::from(plan.height) / viewport.scale(),
        );
        validator.preflight_projective(command, &local_clip, &local_projective)?;
        Ok(plan)
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
    use crate::render::PosedGlyphs;
    use crate::render::command::CompositeMode;
    use crate::render::font::Font;
    use crate::types::{Color, Point, Transform};
    use textflow::placement::GlyphFrame;
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
        let fallback = ProjectiveFallback::new(alloc::vec![0; 15].into_boxed_slice());
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
    fn projected_glyph_uses_ink_bounds_instead_of_full_clip() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 0 },
        )];
        let font = Font::bitmap_8x8();
        let command = DrawCommand::GlyphRun {
            pos: Point::new(10, 20),
            transform: Transform::IDENTITY,
            glyphs: &glyphs,
            font: &font,
            color: Color::rgb(255, 255, 255),
            opa: 255,
        };
        let fallback = ProjectiveFallback::new(alloc::vec![0; 1024]);
        let projection =
            Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));
        let plan = fallback
            .plan(
                &command,
                &Rect::new(0, 0, 64, 64),
                &projection,
                Viewport::new(64, 64, Fixed::ONE),
            )
            .expect("a small glyph should fit in a 1 KiB fallback target");
        assert!(plan.required_bytes <= 1024);
        assert!(plan.width < 64);
        assert!(plan.height < 64);
    }

    #[test]
    fn rendering_reuses_seeded_target_without_growth() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 8 << 8 },
        )];
        let font = Font::bitmap_8x8();
        let command = glyph_command(&glyphs, &font);
        let mut fallback = ProjectiveFallback::new(alloc::vec![7; 8 * 8 * 4]);
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
        let mut actual = expected.clone();

        let mut direct = SwRenderer::new(Texture::new(
            &mut expected,
            WIDTH as u16,
            HEIGHT as u16,
            ColorFormat::RGBA8888,
        ));
        direct.viewport = viewport;
        direct.draw_projective(&command, &clip, &transform).unwrap();

        let mut fallback = ProjectiveFallback::new(alloc::vec![0; 9 * 8 * 4]);
        let plan = fallback
            .plan(&command, &clip, &transform, viewport)
            .unwrap();
        let width = usize::from(plan.width);
        let height = usize::from(plan.height);
        let target = fallback.target_mut(plan);
        for row in 0..height {
            let source = ((plan.y as usize + row) * WIDTH + plan.x as usize) * 4;
            let offset = row * width * 4;
            target[offset..offset + width * 4].copy_from_slice(&actual[source..source + width * 4]);
        }
        fallback
            .render(plan, &command, &transform, viewport)
            .unwrap();
        let target = fallback.target(plan);
        for row in 0..height {
            let dest = ((plan.y as usize + row) * WIDTH + plan.x as usize) * 4;
            let offset = row * width * 4;
            actual[dest..dest + width * 4].copy_from_slice(&target[offset..offset + width * 4]);
        }
        if let Some(index) = actual.iter().zip(&expected).position(|(a, b)| a != b) {
            panic!(
                "pixel mismatch at byte {index}: actual {}, expected {}, plan {plan:?}",
                actual[index], expected[index]
            );
        }
    }

    fn matches_software_on_its_projected_region(command: &DrawCommand<'_>, tolerance: u8) {
        const WIDTH: usize = 64;
        const HEIGHT: usize = 64;
        let viewport = Viewport::new(WIDTH as u16, HEIGHT as u16, Fixed::ONE);
        let clip = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
        let transform =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
        let mut expected = alloc::vec![19; WIDTH * HEIGHT * 4];
        let mut actual = expected.clone();
        let mut direct = SwRenderer::new(Texture::new(
            &mut expected,
            WIDTH as u16,
            HEIGHT as u16,
            ColorFormat::RGBA8888,
        ));
        direct.viewport = viewport;
        direct.draw_projective(command, &clip, &transform).unwrap();

        let mut fallback = ProjectiveFallback::new(alloc::vec![0; 32 * 32 * 4]);
        let plan = fallback.plan(command, &clip, &transform, viewport).unwrap();
        assert!(plan.width < WIDTH as u16);
        assert!(plan.height < HEIGHT as u16);
        let width = usize::from(plan.width);
        let height = usize::from(plan.height);
        let target = fallback.target_mut(plan);
        for row in 0..height {
            let source = ((plan.y as usize + row) * WIDTH + plan.x as usize) * 4;
            let offset = row * width * 4;
            target[offset..offset + width * 4].copy_from_slice(&actual[source..source + width * 4]);
        }
        fallback
            .render(plan, command, &transform, viewport)
            .unwrap();
        let target = fallback.target(plan);
        for row in 0..height {
            let dest = ((plan.y as usize + row) * WIDTH + plan.x as usize) * 4;
            let offset = row * width * 4;
            actual[dest..dest + width * 4].copy_from_slice(&target[offset..offset + width * 4]);
        }
        let mut max_inside = 0;
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            let pixel = index / 4;
            let x = pixel % WIDTH;
            let y = pixel / WIDTH;
            let inside = x >= plan.x as usize
                && x < plan.x as usize + width
                && y >= plan.y as usize
                && y < plan.y as usize + height;
            let difference = actual.abs_diff(*expected);
            if inside {
                max_inside = max_inside.max(difference);
            } else {
                assert_eq!(difference, 0, "outside plan at byte {index}: {plan:?}");
            }
        }
        assert!(
            max_inside <= tolerance,
            "maximum pixel difference {max_inside}: {plan:?}"
        );
    }

    #[test]
    fn projected_shapes_fit_a_tight_budget_and_match_software() {
        let fill = DrawCommand::Fill {
            area: Rect::new(12, 12, 20, 18),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(12, 180, 220),
            radius: Fixed::from_int(3),
            opa: 255,
        };
        matches_software_on_its_projected_region(&fill, 0);

        let border = DrawCommand::Border {
            area: Rect::new(12, 12, 20, 18),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(240, 50, 80),
            width: Fixed::from_int(2),
            radius: Fixed::from_int(3),
            opa: 255,
        };
        matches_software_on_its_projected_region(&border, 0);

        let mut pixels = [160; 8 * 8 * 4];
        let texture = Texture::new(&mut pixels, 8, 8, ColorFormat::RGBA8888);
        let blit = DrawCommand::Blit {
            pos: Point::new(12, 12),
            size: Point::new(20, 18),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        matches_software_on_its_projected_region(&blit, 0);
    }

    #[test]
    fn projected_glyph_bounds_preserve_full_software_output() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 0 },
        )];
        let font = Font::bitmap_8x8();
        let command = DrawCommand::GlyphRun {
            pos: Point::new(20, 25),
            transform: Transform::rotate_deg(Fixed::from_int(8)),
            glyphs: &glyphs,
            font: &font,
            color: Color::rgb(255, 255, 255),
            opa: 255,
        };
        matches_software_on_its_projected_region(&command, 4);
    }

    #[test]
    fn projected_posed_glyph_bounds_preserve_full_software_output() {
        let glyphs = [PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            FlowPoint { x: 0, y: 0 },
        )];
        let frames = [GlyphFrame {
            local_origin: FlowPoint {
                x: 20 << 8,
                y: 20 << 8,
            },
            unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
        }];
        let font = Font::bitmap_8x8();
        let command = DrawCommand::PosedGlyphRun {
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            glyphs: PosedGlyphs::new(&glyphs, &frames).unwrap(),
            font: &font,
            color: Color::rgb(255, 255, 255),
            opa: 255,
        };
        matches_software_on_its_projected_region(&command, 4);
    }

    #[test]
    fn card_budget_uses_projected_pixels_not_the_full_viewport() {
        let fill = DrawCommand::Fill {
            area: Rect::new(140, 70, 200, 180),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(88, 166, 255),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let fallback = ProjectiveFallback::new(alloc::vec![0; 512 * 160 * 4]);
        let plan = fallback
            .plan(
                &fill,
                &Rect::new(0, 0, 480, 320),
                &Transform3D::rotate_y_perspective(Fixed::from_int(8), Fixed::from_int(400)),
                Viewport::new(480, 320, Fixed::ONE),
            )
            .unwrap();
        assert!(plan.width > 0 && plan.height > 0);
        assert!(plan.required_bytes <= fallback.capacity());
    }

    #[test]
    fn projective_image_survives_a_full_y_rotation() {
        let mut pixels = alloc::vec![255; 16 * 16 * 4];
        let texture = Texture::new(&mut pixels, 16, 16, ColorFormat::RGBA8888);
        let command = DrawCommand::Blit {
            pos: Point::new(180, 180),
            size: Point::new(120, 120),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        let mut fallback = ProjectiveFallback::new(alloc::vec![0; 512 * 160 * 4]);
        let clip = Rect::new(0, 0, 480, 320);
        let viewport = Viewport::new(960, 640, Fixed::from_int(2));
        let mut rendered = 0;
        for angle in 0..360 {
            let center = Transform3D::translate(Fixed::from_int(240), Fixed::from_int(240));
            let origin = Transform3D::translate(Fixed::from_int(-240), Fixed::from_int(-240));
            let phase = Fixed::from_int(angle % 180) / Fixed::from_int(180);
            let two_t_minus_one = phase * Fixed::from_int(2) - Fixed::ONE;
            let height = Fixed::ONE - two_t_minus_one * two_t_minus_one;
            let bounce =
                Transform3D::translate(Fixed::ZERO, Fixed::ZERO - height * Fixed::from_int(100));
            let scale = Transform3D::scale(
                Fixed::ONE - (Fixed::ONE - height) / Fixed::from_int(4),
                Fixed::ONE + height / Fixed::from_int(8),
            );
            let projective = center
                .compose(&bounce)
                .compose(&Transform3D::rotate_y_perspective(
                    Fixed::from_int(angle),
                    Fixed::from_int(400),
                ))
                .compose(&scale)
                .compose(&origin);
            if let Ok(plan) = fallback.plan(&command, &clip, &projective, viewport) {
                fallback.target_mut(plan).fill(0);
                fallback
                    .render(plan, &command, &projective, viewport)
                    .unwrap();
                rendered += 1;
            }
        }
        assert!(rendered > 340, "rendered {rendered} angles");
    }
}
