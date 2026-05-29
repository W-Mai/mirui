//! Smoke test for the wgpu backend. Opens a window, paints a red
//! rounded rectangle each frame, exits on close. Visual check —
//! the window must show the rectangle, not just open without panic.

use mirui::draw::canvas::Canvas;
use mirui::draw::wgpu_render::WgpuRendererFactory;
use mirui::surface::Surface;
use mirui::surface::wgpu_surface::WgpuSurface;
use mirui::types::{Color, Fixed, Rect, Viewport};

use mirui::app::RendererFactory;
use mirui::draw::renderer::Renderer;

fn main() {
    let mut surface = WgpuSurface::new("mirui wgpu smoke", 480, 320);
    let mut factory = WgpuRendererFactory::new();
    let info = surface.display_info();
    let viewport = Viewport::new(info.width, info.height, info.scale);
    println!("WgpuSurface ready: {}x{}", info.width, info.height);

    loop {
        if let Some(event) = surface.poll_event() {
            if matches!(event, mirui::surface::InputEvent::Quit) {
                break;
            }
        }

        {
            let mut renderer = factory.make(&mut surface, &viewport);
            renderer.fill_rect(
                &Rect::new(80, 60, 320, 200),
                &Rect::new(0, 0, 480, 320),
                &Color::rgb(220, 40, 40),
                Fixed::from(24),
                255,
            );
            Renderer::flush(&mut renderer);
        }
        surface.flush(&Rect::new(0, 0, 480, 320));

        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    println!("done");
}
