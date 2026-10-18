use super::*;
use crate::{
    render::{
        backends::sw::SwRenderer,
        canvas::{Canvas, Paint},
        path::Path,
        raster::FillRule,
    },
    types::{Color, Transform},
};

#[test]
fn physical_write_boundary_does_not_shrink_in_place_filter_sampling() {
    let mut renderer = SwRenderer::new(Texture::owned(10, 8, ColorFormat::RGBA8888).unwrap());
    renderer.clear(&PhysicalRect::from_size(10, 8), &Color::rgb(10, 30, 80));
    let before = renderer.target.read_pixels().unwrap().bytes().to_vec();
    let blur = DrawCommand::ApplyBlur {
        alpha: Fixed::from_ratio(1, 2),
        region: Rect::new(0, 0, 10, 8),
    };
    let request = DrawRequest::new(&blur, Rect::new(0, 0, 10, 8))
        .with_physical_clip(PhysicalRect::from_size(5, 8));
    assert_eq!(
        renderer.route(&request),
        Err(RenderError::Unsupported(RenderFeature::Blur))
    );
    assert_eq!(
        renderer.submit(&request),
        Err(RenderError::Unsupported(RenderFeature::Blur))
    );
    assert_eq!(renderer.target.read_pixels().unwrap().bytes(), before);
    renderer
        .submit(
            &DrawRequest::new(&blur, request.clip)
                .with_physical_clip(PhysicalRect::from_size(10, 8)),
        )
        .unwrap();
}

#[test]
fn physical_scissors_partition_draws_without_rescaling_geometry() {
    let background = Color::rgb(17, 31, 53);
    let area = Rect::new(0, 0, 40, 30);
    let path = Path::rect(1.into(), 2.into(), 20.into(), 18.into());
    let paint = Paint::Color(Color::rgba(20, 170, 40, 150).into());
    let commands = [
        DrawCommand::Fill {
            area,
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgba(200, 80, 20, 128),
            radius: 3.into(),
            opa: 210,
        },
        DrawCommand::FillPath {
            path: &path,
            transform: Transform::rotate_deg(12.into()),
            paint: &paint,
            opa: 180,
            fill_rule: FillRule::EvenOdd,
        },
        DrawCommand::Line {
            p1: crate::types::Point::new(0, 0),
            p2: crate::types::Point::new(22, 18),
            transform: Transform::IDENTITY,
            color: Color::rgba(80, 120, 220, 160),
            width: 3.into(),
            opa: 200,
        },
    ];
    for scale in [
        Fixed::from_ratio(229, 256),
        Fixed::from_f32(1.5),
        Fixed::from_int(3),
    ] {
        let viewport = Viewport::new(31, 23, scale);
        let mut full = SwRenderer::new(Texture::owned(31, 23, ColorFormat::RGBA8888).unwrap());
        full.viewport = viewport;
        full.clear(&PhysicalRect::from_size(31, 23), &background);
        let mut split = SwRenderer::new(Texture::owned(31, 23, ColorFormat::RGBA8888).unwrap());
        split.viewport = viewport;
        split.clear(&PhysicalRect::from_size(31, 23), &background);
        for command in &commands {
            full.submit(&DrawRequest::new(command, area)).unwrap();
        }
        for clip in [
            PhysicalRect::new(0, 0, 7, 23).unwrap(),
            PhysicalRect::new(7, 0, 13, 23).unwrap(),
            PhysicalRect::new(20, 0, 11, 23).unwrap(),
        ] {
            for command in &commands {
                split
                    .submit(&DrawRequest::new(command, area).with_physical_clip(clip))
                    .unwrap();
            }
        }
        assert_eq!(
            full.target.read_pixels().unwrap().bytes(),
            split.target.read_pixels().unwrap().bytes(),
            "scale={scale:?}"
        );
        let before = split.target.read_pixels().unwrap().bytes().to_vec();
        split
            .submit(&DrawRequest::new(&commands[0], area).with_physical_clip(PhysicalRect::EMPTY))
            .unwrap();
        assert_eq!(split.target.read_pixels().unwrap().bytes(), before);
        split.submit(&DrawRequest::new(&commands[0], area)).unwrap();
        assert_ne!(split.target.read_pixels().unwrap().bytes(), before);
    }
}

#[test]
fn region_rebases_physical_boundaries_without_fractional_roundtrip() {
    struct Recorder {
        clip: Option<PhysicalRect>,
    }
    impl Renderer for Recorder {
        type TargetState = ();
        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.clip = request.physical_clip;
            Ok(())
        }
        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
        fn output_scale(&self) -> Fixed {
            Fixed::from_ratio(229, 256)
        }
    }
    let command = DrawCommand::PopClip;
    let mut recorder = Recorder { clip: None };
    let mut region = RegionRenderer::new(&mut recorder, (13, 17));
    region
        .submit(
            &DrawRequest::new(&command, Rect::new(0, 0, 100, 100))
                .with_physical_clip(PhysicalRect::new(10, 19, 9, 5).unwrap()),
        )
        .unwrap();
    assert_eq!(recorder.clip, PhysicalRect::new(0, 2, 6, 5));
}
