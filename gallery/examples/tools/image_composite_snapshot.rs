#[allow(dead_code)]
mod backend_snapshot_support;

use std::env;
use std::path::PathBuf;
#[cfg(feature = "wgpu")]
use std::time::Duration;

use mirui::prelude::*;
use mirui::render::command::{CompositeMode, DrawCommand};
use mirui::render::sdl_gpu::SdlGpuFactory;
use mirui::render::texture::{ColorFormat, Texture};
#[cfg(feature = "wgpu")]
use mirui::render::wgpu::WgpuRendererFactory;
use mirui::render::{DrawRequest, ProjectiveFallback, RenderError, Renderer, SwRendererFactory};
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::sdl_gpu::SdlGpuSurface;
#[cfg(feature = "wgpu")]
use mirui::surface::wgpu_surface::WgpuSurface;
use mirui::types::Transform;

use backend_snapshot_support::write_png;

const WIDTH: u16 = 320;
const HEIGHT: u16 = 240;
const SCALE: u16 = 2;

fn draw_fixture(
    renderer: &mut impl Renderer,
    projected: bool,
    blurred: bool,
    composite: CompositeMode,
) -> Result<Texture<'static>, RenderError> {
    let clip = Rect::new(0, 0, WIDTH, HEIGHT);
    let background = DrawCommand::Fill {
        area: clip,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(22, 36, 58),
        radius: Fixed::ZERO,
        opa: 255,
    };
    renderer.submit(&DrawRequest::new(&background, clip))?;

    let pixels = [200u8, 62, 112, 190].repeat(12 * 12);
    let texture = Texture::from_ref(&pixels, 12, 12, ColorFormat::RGBA8888);
    let quad = [
        Point::new(76, 42),
        Point::new(242, 53),
        Point::new(222, 198),
        Point::new(92, 182),
    ];
    let image = DrawCommand::Blit {
        pos: Point::new(76, 42),
        size: Point::new(166, 145),
        transform: Transform::IDENTITY,
        quad: projected.then_some(quad),
        texture: &texture,
        opa: 216,
        radius: Fixed::from_int(18),
        composite,
    };
    renderer.submit(&DrawRequest::new(&image, clip))?;
    if blurred {
        let blur = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE / Fixed::from_int(3),
            region: Rect::new(68, 34, 184, 174),
        };
        renderer.submit(&DrawRequest::new(&blur, clip))?;
    }
    renderer.prepare_readback(&clip);
    renderer
        .sample_target_region(&clip)
        .and_then(|image| image.ok_or(RenderError::BackendFailure))
}

fn main() {
    let mut args = env::args().skip(1);
    let backend = args.next().expect("backend: sw, sdl, or wgpu");
    let path = PathBuf::from(args.next().expect("output PNG path"));
    let projected = args.next().as_deref() == Some("quad");
    let blurred = args.next().as_deref() == Some("blur");
    let composite = match args.next().as_deref() {
        None | Some("screen") => CompositeMode::Screen,
        Some("darken") => CompositeMode::Darken,
        Some("lighten") => CompositeMode::Lighten,
        Some("difference") => CompositeMode::Difference,
        Some(mode) => panic!("unknown composite mode: {mode}"),
    };

    let image = match backend.as_str() {
        "sw" => {
            let mut surface = FramebufSurface::with_scale_and_format(
                WIDTH * SCALE,
                HEIGHT * SCALE,
                Fixed::from(SCALE),
                ColorFormat::RGBA8888,
                |_, _| {},
            );
            let mut factory = SwRendererFactory::new();
            let viewport = surface.display_info().viewport();
            draw_fixture(
                &mut factory.make(&mut surface, &viewport),
                projected,
                blurred,
                composite,
            )
            .expect("software composite draw")
        }
        "sdl" => {
            let mut surface = SdlGpuSurface::new("mirui image composite parity", WIDTH, HEIGHT);
            let mut factory = SdlGpuFactory::new()
                .with_projective_fallback(ProjectiveFallback::new(vec![0; 512 * 1024]));
            let viewport = surface.display_info().viewport();
            draw_fixture(
                &mut factory.make(&mut surface, &viewport),
                projected,
                blurred,
                composite,
            )
            .expect("SDL composite draw")
        }
        #[cfg(feature = "wgpu")]
        "wgpu" => {
            let mut surface = WgpuSurface::new("mirui image composite parity", WIDTH, HEIGHT);
            let mut factory = WgpuRendererFactory::new().with_target_edit_budget(512 * 1024);
            let mut image = None;
            for _ in 0..16 {
                while surface.poll_event().is_some() {}
                let viewport = surface.display_info().viewport();
                let result = {
                    let mut renderer = factory.make(&mut surface, &viewport);
                    draw_fixture(&mut renderer, projected, blurred, composite)
                };
                match result {
                    Ok(frame) => {
                        image = Some(frame);
                        break;
                    }
                    Err(RenderError::BackendFailure) => {
                        surface.flush(&Rect::new(0, 0, WIDTH, HEIGHT));
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    Err(error) => panic!("WGPU composite draw failed: {error:?}"),
                }
            }
            image.expect("WGPU swapchain remained unavailable")
        }
        _ => panic!("unknown backend: {backend}"),
    };
    write_png(&path, &image);
    eprintln!("saved {}", path.display());
}
