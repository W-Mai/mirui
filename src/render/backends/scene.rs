//! Renderer trait → Scene capture. Sits alongside other backends because it
//! satisfies the same trait the screen renderers do, even though the "target"
//! is an op stream rather than pixels.

use alloc::vec::Vec;

use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::render::scene::Scene;
use crate::render::scene::record::{RecordError, ResourceResolver, record_command};
use crate::types::Rect;

/// Records draws into a Scene through the Renderer trait. Errors accumulate
/// because Renderer::draw can't return Result without desyncing the caller's
/// group stack.
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
    fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
        match record_command(cmd, self.resolver) {
            Ok(op) => {
                self.scene.push(op);
            }
            Err(e) => self.errors.push(e),
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
    use crate::types::{Color, Fixed, Point, Transform};

    struct PanicResolver;
    impl ResourceResolver for PanicResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            unreachable!("driver does not draw Label")
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
        let mirx_bytes = mirx::encode_chunk_generic(
            mirx::chunk_type::VECTOR,
            mirx::ChunkEntry::FLAG_CRITICAL,
            &payload,
        );

        let parsed = mirx::parse_chunk(&mirx_bytes).unwrap();
        let extracted = parsed
            .chunk_payload(&mirx_bytes, mirx::chunk_type::VECTOR)
            .unwrap();
        let back = Scene::decode(extracted).unwrap();

        assert_eq!(back.ops, scene.ops);
        assert!(matches!(back.ops[0], SceneOp::Line { .. }));
        assert!(matches!(back.ops[1], SceneOp::FillRect { .. }));
    }
}
