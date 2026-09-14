//! Renderer trait → Scene capture. Sits alongside other backends because it
//! satisfies the same trait the screen renderers do, even though the "target"
//! is an op stream rather than pixels.

use alloc::vec::Vec;

use crate::render::command::DrawCommand;
use crate::render::renderer::{DrawRequest, RenderError, RenderFeature, RenderRoute, Renderer};
use crate::render::scene::Scene;
use crate::render::scene::record::{RecordError, ResourceResolver, record_command};
use crate::types::Rect;

/// Records draws into a Scene through the Renderer trait. The legacy `draw`
/// entry accumulates errors; checked submissions report them directly.
pub struct SceneRenderer<'a> {
    pub scene: &'a mut Scene,
    pub resolver: &'a mut dyn ResourceResolver,
    pub errors: Vec<RecordError>,
}

impl<'a> SceneRenderer<'a> {
    pub fn new(scene: &'a mut Scene, resolver: &'a mut dyn ResourceResolver) -> Self {
        Self {
            scene,
            resolver,
            errors: Vec::new(),
        }
    }
}

impl Renderer for SceneRenderer<'_> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        if !request.projective.is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        if matches!(request.command, DrawCommand::ApplyBlur { .. }) {
            return Err(RenderError::Unsupported(RenderFeature::Blur));
        }
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.route(request)?;
        match record_command(request.command, self.resolver) {
            Ok(op) => {
                self.scene.push(op);
                Ok(())
            }
            Err(error) => {
                self.errors.push(error);
                Err(RenderError::BackendFailure)
            }
        }
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        if matches!(
            self.submit(&DrawRequest::new(cmd, *clip)),
            Err(RenderError::Unsupported(_))
        ) {
            self.errors.push(RecordError::UnsupportedCommand);
        }
    }

    fn flush(&mut self) {}
}

impl Scene {
    pub fn renderer<'a>(&'a mut self, resolver: &'a mut dyn ResourceResolver) -> SceneRenderer<'a> {
        SceneRenderer::new(self, resolver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::Font;
    use crate::render::scene::{ResourceRef, SceneOp};
    use crate::render::texture::Texture;
    use crate::types::{Color, Fixed, Point, Transform, Transform3D};

    struct PanicResolver;
    impl ResourceResolver for PanicResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            unreachable!("driver has no font commands")
        }
        fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
            unreachable!("driver does not draw Blit")
        }
    }

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    #[test]
    fn capture_rejects_short_texture_before_resolving_it() {
        let texture = Texture::from_ref(
            &[0u8; 1],
            2,
            2,
            crate::render::texture::ColorFormat::RGBA8888,
        );
        let command = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: crate::render::command::CompositeMode::SourceOver,
        };
        let mut scene = Scene::new();
        let mut resolver = PanicResolver;
        let mut renderer = scene.renderer(&mut resolver);
        assert_eq!(
            renderer.submit(&DrawRequest::new(&command, Rect::new(0, 0, 2, 2))),
            Err(RenderError::InvalidTexture)
        );
        assert!(renderer.scene.ops.is_empty());
    }

    #[test]
    fn scene_renderer_captures_a_full_pipeline_to_mirx_and_back() {
        fn driver(r: &mut dyn Renderer, clip: &Rect) {
            r.draw(
                &DrawCommand::Line {
                    p1: Point::ZERO,
                    p2: Point {
                        x: Fixed::from_int(10),
                        y: Fixed::from_int(10),
                    },
                    transform: Transform::IDENTITY,
                    color: red(),
                    width: Fixed::from_int(1),
                    opa: 255,
                },
                clip,
            );
            r.draw(
                &DrawCommand::Fill {
                    area: Rect {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                        w: Fixed::from_int(8),
                        h: Fixed::from_int(8),
                    },
                    transform: Transform::IDENTITY,
                    quad: None,
                    color: red(),
                    radius: Fixed::ZERO,
                    opa: 200,
                },
                clip,
            );
            r.flush();
        }

        let mut scene = Scene::new();
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(100),
            h: Fixed::from_int(100),
        };
        {
            let mut r = PanicResolver;
            let mut sink = scene.renderer(&mut r);
            driver(&mut sink, &clip);
            assert!(sink.errors.is_empty());
        }
        assert_eq!(scene.ops.len(), 2);

        let payload = scene.encode().unwrap();
        let mut document = mirx::Document::new();
        document
            .push_extension(
                mirx::extension::Extension::owned(mirx::ChunkType::VECTOR, payload)
                    .with_flags(mirx::ChunkFlags::CRITICAL),
            )
            .unwrap();
        let mirx_bytes = document.finish().unwrap();

        let reader = mirx::Reader::open(&mirx_bytes).unwrap();
        let extracted = reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == mirx::ChunkType::VECTOR)
            .map(|chunk| chunk.payload())
            .unwrap();
        let back = Scene::decode(extracted).unwrap();

        assert_eq!(back.ops, scene.ops);
        assert!(matches!(back.ops[0], SceneOp::Line { .. }));
        assert!(matches!(back.ops[1], SceneOp::FillRect { .. }));
    }

    #[test]
    fn blur_capture_never_turns_into_a_group_end() {
        let mut scene = Scene::new();
        let mut resolver = PanicResolver;
        let mut sink = scene.renderer(&mut resolver);
        let clip = Rect::new(0, 0, 32, 32);
        let blur = DrawCommand::ApplyBlur {
            alpha: Fixed::from_ratio(1, 2),
            region: clip,
        };
        assert_eq!(
            sink.submit(&DrawRequest::new(&blur, clip)),
            Err(RenderError::Unsupported(RenderFeature::Blur))
        );
        assert!(sink.scene.ops.is_empty());
        sink.draw(&blur, &clip);
        assert_eq!(sink.errors, [RecordError::UnsupportedCommand]);
        assert!(sink.scene.ops.is_empty());
    }

    #[test]
    fn projected_capture_rejects_the_draw_before_recording() {
        let mut scene = Scene::new();
        let mut resolver = PanicResolver;
        let mut sink = scene.renderer(&mut resolver);
        let clip = Rect::new(0, 0, 32, 32);
        let command = DrawCommand::Fill {
            area: clip,
            transform: Transform::IDENTITY,
            quad: None,
            color: red(),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let request = DrawRequest::new(&command, clip)
            .with_projective(Transform3D::translate(Fixed::from_int(4), Fixed::ZERO));
        assert_eq!(
            sink.submit(&request),
            Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
        );
        assert!(sink.scene.ops.is_empty());
    }
}
