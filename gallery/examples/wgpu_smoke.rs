//! Hello-window smoke test for the wgpu backend. Opens a window via
//! winit + wgpu and exits when closed.
//!
//! Real rendering wiring lands in subsequent commits — for now this
//! just confirms `WgpuSurface::new()` constructs without panic and
//! `Surface::poll_event()` produces a `Quit` when the user closes
//! the window.

use mirui::surface::Surface;
use mirui::surface::wgpu_surface::WgpuSurface;

fn main() {
    let mut surface = WgpuSurface::new("mirui wgpu smoke", 480, 320);

    println!(
        "WgpuSurface ready: {:?}",
        surface.display_info().viewport().logical_size()
    );

    // Drain events until Quit (user closed the window).
    loop {
        if let Some(event) = surface.poll_event() {
            println!("event: {event:?}");
            if matches!(event, mirui::surface::InputEvent::Quit) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }

    println!("done");
}
