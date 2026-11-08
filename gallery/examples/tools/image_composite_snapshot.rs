#[allow(dead_code)]
mod backend_snapshot_support;

use std::borrow::Cow;
use std::env;
use std::path::PathBuf;
#[cfg(feature = "wgpu")]
use std::time::Duration;

use mirui::prelude::*;
use mirui::render::command::{CompositeMode, DrawCommand};
use mirui::render::path::Path;
use mirui::render::raster::FillRule;
use mirui::render::sdl_gpu::SdlGpuFactory;
use mirui::render::texture::{ColorFormat, Texture};
#[cfg(feature = "wgpu")]
use mirui::render::wgpu::WgpuRendererFactory;
use mirui::render::{DrawRequest, ProjectiveFallback, RenderError, Renderer, SwRendererFactory};
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::sdl_gpu::{SdlGpuConfig, SdlGpuDriver, SdlGpuSurface};
#[cfg(feature = "wgpu")]
use mirui::surface::wgpu_surface::WgpuSurface;
use mirui::types::Transform;
use mirx::scene::{GradientStop, GradientUnits, LinearGradient, Paint, RadialGradient, SpreadMode};

use backend_snapshot_support::write_png;

const WIDTH: u16 = 320;
const HEIGHT: u16 = 240;
const SCALE: u16 = 2;

#[derive(Clone, Copy)]
enum GradientCase {
    None,
    Linear,
    Radial,
}

fn capture<B: Surface>(
    surface: &mut B,
    factory: &mut impl RendererFactory<B>,
    projected: bool,
    blurred: bool,
    composite: CompositeMode,
    gradient: GradientCase,
) -> Result<Texture<'static>, RenderError> {
    let viewport = surface.display_info().viewport();
    let result = (|| {
        surface.begin_flush()?;
        let image = {
            let mut renderer = factory.make(surface, &viewport);
            let image = draw_fixture(&mut renderer, projected, blurred, composite, gradient)?;
            Renderer::flush(&mut renderer)?;
            image
        };
        let (width, height) = viewport.physical_size();
        surface.flush(PhysicalRect::from_size(width, height))?;
        surface.end_flush()?;
        Ok(image)
    })();
    if result.is_err() {
        surface.abort_flush();
    }
    result
}

fn draw_fixture(
    renderer: &mut impl Renderer,
    projected: bool,
    blurred: bool,
    composite: CompositeMode,
    gradient: GradientCase,
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
    renderer.draw_request(&DrawRequest::new(&background, clip))?;

    if !matches!(gradient, GradientCase::None) {
        let path = Path::rounded_rect(
            Fixed::from_int(76),
            Fixed::from_int(42),
            Fixed::from_int(166),
            Fixed::from_int(145),
            Fixed::from_int(18),
        );
        let mut paint = Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point::new(
                mirx::types::Fixed::from_int(76),
                mirx::types::Fixed::from_int(42),
            ),
            end: mirx::types::Point::new(
                mirx::types::Fixed::from_int(242),
                mirx::types::Fixed::from_int(187),
            ),
            stops: Cow::Owned(vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgb(52, 118, 255),
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: mirx::types::Color::rgb(250, 72, 120),
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::UserSpaceOnUse,
            transform: mirx::types::Transform::IDENTITY,
        });
        if matches!(gradient, GradientCase::Radial) {
            let Paint::LinearGradient(linear) = paint else {
                unreachable!()
            };
            let center = mirx::types::Point::new(
                mirx::types::Fixed::from_ratio(1, 2),
                mirx::types::Fixed::from_ratio(1, 2),
            );
            paint = Paint::RadialGradient(RadialGradient {
                center,
                focal: center,
                radius: mirx::types::Fixed::from_ratio(1, 2),
                focal_radius: mirx::types::Fixed::ZERO,
                stops: linear.stops,
                spread: SpreadMode::Pad,
                units: GradientUnits::ObjectBoundingBox,
                transform: mirx::types::Transform::IDENTITY,
            });
        }
        renderer.draw_request(&DrawRequest::new(
            &DrawCommand::FillPath {
                path: &path,
                transform: Transform::IDENTITY,
                paint: &paint,
                opa: 216,
                fill_rule: FillRule::NonZero,
            },
            clip,
        ))?;
    } else {
        let pixels = [200u8, 62, 112, 190].repeat(12 * 12);
        let texture =
            Texture::from_ref(&pixels, 12, 12, ColorFormat::RGBA8888).expect("valid pixel storage");
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
        renderer.draw_request(&DrawRequest::new(&image, clip))?;
    }
    if blurred {
        let blur = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE / Fixed::from_int(3),
            region: Rect::new(68, 34, 184, 174),
        };
        renderer.draw_request(&DrawRequest::new(&blur, clip))?;
    }
    renderer.prepare_readback(&clip)?;
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
    let gradient = match args.next().as_deref() {
        Some("gradient") => GradientCase::Linear,
        Some("radial") => GradientCase::Radial,
        None => GradientCase::None,
        Some(other) => panic!("unknown gradient case: {other}"),
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
            capture(
                &mut surface,
                &mut factory,
                projected,
                blurred,
                composite,
                gradient,
            )
            .expect("software composite draw")
        }
        "sdl" | "gles" => {
            let config = if backend == "gles" {
                SdlGpuConfig {
                    driver: SdlGpuDriver::OpenGles2 { samples: 0 },
                    ..Default::default()
                }
            } else {
                SdlGpuConfig::default()
            };
            let mut surface =
                SdlGpuSurface::with_config("mirui image composite parity", WIDTH, HEIGHT, config)
                    .expect("SDL GPU surface");
            let mut factory = SdlGpuFactory::new()
                .with_projective_fallback(ProjectiveFallback::new(vec![0; 512 * 1024]));
            capture(
                &mut surface,
                &mut factory,
                projected,
                blurred,
                composite,
                gradient,
            )
            .expect("SDL composite draw")
        }
        #[cfg(feature = "wgpu")]
        "wgpu" => {
            let mut surface = WgpuSurface::new("mirui image composite parity", WIDTH, HEIGHT)
                .expect("WGPU surface initialization");
            let mut factory = WgpuRendererFactory::new().with_target_edit_budget(512 * 1024);
            let mut image = None;
            for _ in 0..16 {
                while surface.poll_event().is_some() {}
                let result = capture(
                    &mut surface,
                    &mut factory,
                    projected,
                    blurred,
                    composite,
                    gradient,
                );
                match result {
                    Ok(frame) => {
                        image = Some(frame);
                        break;
                    }
                    Err(RenderError::SurfaceUnavailable) => {
                        surface.frame_end().expect("surface maintenance");
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
