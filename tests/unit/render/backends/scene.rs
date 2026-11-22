use super::*;
use crate::render::font::Font;
use crate::render::scene::{ResourceRef, SceneOp};
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D};

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
fn capture_rejects_empty_image_geometry_before_resolving_it() {
    let texture = Texture::from_ref(
        &[0u8; 16],
        2,
        2,
        crate::render::texture::ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    let command = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(0, 2),
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
        renderer.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 2, 2))),
        Err(RenderError::InvalidGeometry)
    );
    assert!(renderer.scene.ops.is_empty());
}

#[test]
fn scene_renderer_captures_a_full_pipeline_to_mirx_and_back() {
    fn driver(r: &mut dyn Renderer, clip: &Rect) {
        let line = DrawCommand::Line {
            p1: Point::ZERO,
            p2: Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(10),
            },
            transform: Transform::IDENTITY,
            color: red(),
            width: Fixed::from_int(1),
            opa: 255,
        };
        r.draw_request(&DrawRequest::new(&line, *clip)).unwrap();
        let fill = DrawCommand::Fill {
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
        };
        r.draw_request(&DrawRequest::new(&fill, *clip)).unwrap();
        r.flush().unwrap();
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
        sink.draw_request(&DrawRequest::new(&blur, clip)),
        Err(RenderError::Unsupported(RenderFeature::Blur))
    );
    assert!(sink.scene.ops.is_empty());
    assert_eq!(sink.errors, []);
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
        sink.draw_request(&request),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );
    assert!(sink.scene.ops.is_empty());
}
