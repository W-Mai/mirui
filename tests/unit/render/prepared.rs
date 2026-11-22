use super::*;
use crate::render::{
    CompositeMode, Renderer, SwRenderer,
    texture::{ColorFormat, Texture},
};
use crate::test_support::tracked_allocations;
use crate::types::{Point, Transform};

fn command<'a>(texture: &'a Texture<'a>, pos: Point, size: Point) -> DrawCommand<'a> {
    DrawCommand::Blit {
        texture,
        pos,
        size,
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    }
}

#[test]
fn preparation_checks_geometry_and_storage_before_creating_a_ready_draw() {
    let bytes = [255; 4 * 4 * 4];
    let mut texture = Texture::from_ref(&bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
    for (width, size, scale, expected) in [
        (
            4,
            Point::new(4, 4),
            Fixed::ZERO,
            RenderError::InvalidGeometry,
        ),
        (
            4,
            Point::new(0, 4),
            Fixed::ONE,
            RenderError::InvalidGeometry,
        ),
        (5, Point::new(4, 4), Fixed::ONE, RenderError::InvalidTexture),
    ] {
        texture.width = width;
        let cmd = command(&texture, Point::ZERO, size);
        let request = DrawRequest::new(&cmd, Rect::new(0, 0, 4, 4));
        let mut cx = Context::from_waker(core::task::Waker::noop());
        for result in [
            PreparedDraw::poll_cpu(request, RenderRoute::Native, scale, &mut cx),
            PreparedDraw::poll_upload(request, RenderRoute::Native, scale, &mut cx),
            PreparedDraw::poll_complete(request, RenderRoute::Native, scale, &mut cx),
            Poll::Ready(PreparedDraw::native(request, RenderRoute::Native, scale)),
        ] {
            assert!(matches!(result, Poll::Ready(Err(error)) if error == expected));
        }
    }
}

#[test]
fn upload_footprint_covers_pixel_centers_and_filter_neighbors() {
    let bytes = [0u8; 400 * 4];
    let source = Texture::from_ref(&bytes, 400, 1, ColorFormat::RGBA8888).unwrap();
    let cmd = command(&source, Point::ZERO, Point::new(4, 1));
    let request = DrawRequest::new(&cmd, Rect::new(1, 0, 1, 1));
    let mut cx = Context::from_waker(core::task::Waker::noop());
    let Poll::Ready(Ok(draw)) =
        PreparedDraw::poll_upload(request, RenderRoute::Native, Fixed::ONE, &mut cx)
    else {
        panic!("resident upload");
    };
    let area = draw.texture_region().unwrap().region();
    assert_eq!(area, Region::new(99, 0, 102, 1).unwrap());
    // A center sample at 150 must not use the software left-edge footprint.
    assert!(draw.texture_region().unwrap().get_pixel(150, 0).is_some());
}

#[test]
fn preparation_retains_resources_not_temporary_command_descriptions() {
    let bytes = [255; 8 * 8 * 4];
    let texture = Texture::from_ref(&bytes, 8, 8, ColorFormat::RGBA8888).unwrap();
    let mut cx = Context::from_waker(core::task::Waker::noop());
    assert_eq!(
        tracked_allocations(|| {
            let draw = {
                let cmd = command(&texture, Point::new(2, 3), Point::new(8, 8));
                let Poll::Ready(Ok(draw)) = PreparedDraw::poll_cpu(
                    DrawRequest::new(&cmd, Rect::new(4, 6, 3, 2)),
                    RenderRoute::Native,
                    Fixed::ONE,
                    &mut cx,
                ) else {
                    panic!("static source must be ready");
                };
                draw
            };
            let source = draw.texture_region().unwrap();
            assert_eq!(source.region(), Region::new(0, 0, 8, 8).unwrap());
            assert_eq!(source.pixels().unwrap().bytes().as_ptr(), bytes.as_ptr());
            assert_eq!(draw.request().clip, Rect::new(4, 6, 3, 2));
            assert_eq!(draw.route(), RenderRoute::Native);
        }),
        0
    );
}

#[test]
fn prepared_region_contains_every_sample_of_the_existing_rasterizer() {
    let bytes: [u8; 7 * 5 * 4] = core::array::from_fn(|i| match i % 4 {
        0 => (i / 4 % 7 + 1) as u8,
        1 => (i / 4 / 7 + 1) as u8,
        _ => 255,
    });
    let texture = Texture::from_ref(&bytes, 7, 5, ColorFormat::RGBA8888).unwrap();
    let mut cx = Context::from_waker(core::task::Waker::noop());
    for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
        for offset in [-193, 0, 128, 256] {
            for extent in [128, 1024, 1792, 3584] {
                for clip in [Rect::new(0, 0, 16, 16), Rect::new(2, 1, 3, 4)] {
                    let mut cmd = command(
                        &texture,
                        Point::new(Fixed::from_ratio(offset, 256), Fixed::HALF),
                        Point::new(Fixed::from_ratio(extent, 256), 5),
                    );
                    for opacity in [117, 255] {
                        if let DrawCommand::Blit { opa, .. } = &mut cmd {
                            *opa = opacity;
                        }
                        let request = DrawRequest::new(&cmd, clip)
                            .with_physical_clip(PhysicalRect::new(1, 1, 13, 13).unwrap());
                        let Poll::Ready(Ok(draw)) =
                            PreparedDraw::poll_cpu(request, RenderRoute::Native, scale, &mut cx)
                        else {
                            panic!("static source must be ready");
                        };
                        let target = Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap();
                        let mut renderer = SwRenderer::new(target)
                            .with_alpha_mode(crate::render::texture::AlphaMode::Blend);
                        renderer.viewport = crate::types::Viewport::new(24, 24, scale);
                        renderer.draw_request(&request).unwrap();
                        let pixels = renderer.target.read_pixels().unwrap();
                        for y in 0..24 {
                            for x in 0..24 {
                                let color = pixels.get_pixel(x, y);
                                if color.a != 0 {
                                    assert!(
                                        draw.texture_region()
                                            .unwrap()
                                            .get_pixel(
                                                u32::from(color.r - 1),
                                                u32::from(color.g - 1)
                                            )
                                            .is_some(),
                                        "offset={offset} extent={extent} scale={scale:?} pixel={color:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn transformed_requests_preserve_geometry_and_conservatively_prepare_the_source() {
    let bytes = [255; 4 * 4 * 4];
    let texture = Texture::from_ref(&bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
    let mut cmd = command(&texture, Point::new(1, 2), Point::new(8, 8));
    if let DrawCommand::Blit { transform, .. } = &mut cmd {
        *transform = Transform::scale(Fixed::from_int(2), Fixed::ONE);
    }
    let mut cx = Context::from_waker(core::task::Waker::noop());
    let Poll::Ready(Ok(draw)) = PreparedDraw::poll_cpu(
        DrawRequest::new(&cmd, Rect::new(2, 3, 1, 1)),
        RenderRoute::Native,
        Fixed::ONE,
        &mut cx,
    ) else {
        panic!("static source must be ready");
    };
    assert_eq!(
        draw.texture_region().unwrap().region(),
        Region::new(0, 0, 4, 4).unwrap()
    );
    assert_eq!(draw.request().command.transform(), cmd.transform());
}
