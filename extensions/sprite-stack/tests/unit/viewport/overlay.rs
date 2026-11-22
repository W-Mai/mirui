use super::*;
use crate::allocations;
use crate::raster::{CameraWorkspace, Frame, RollMapping};
use mirui::{
    render::texture::ColorFormat,
    types::{Color, Fixed},
};

struct Panel(u32);

impl FrameOverlay<u32> for Panel {
    fn prepare(&mut self, _: OverlayFrame, color: &u32) -> Result<bool, RenderError> {
        self.0 = *color;
        Ok(true)
    }

    fn covers(&self, _: [i32; 4], _: &mut [u8]) -> bool {
        true
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        let [left, top, right, bottom] = canvas.clip;
        for y in top..bottom {
            for x in left..right {
                canvas.point(x, y, self.0);
            }
        }
    }
}

#[test]
fn attached_overlay_borrows_the_compact_target_without_allocating() {
    let size = [128, 128];
    let mut workspace = CameraWorkspace::new(size, Fixed::ZERO, true).unwrap();
    let mut output = Frame::with_format(size[0], size[1], ColorFormat::RGB565).unwrap();
    let mapping = RollMapping::new(size, Fixed::ZERO, None);
    let overlay = ViewportOverlay::new(0xff00_00ff, Panel(0));
    let storage = workspace.bytes() + output.bytes();

    let allocations = allocations::tracked_allocations(|| {
        let overlay = overlay
            .prepare(OverlayFrame { size, camera: None })
            .unwrap()
            .unwrap();
        workspace
            .render(
                &mut output,
                &mapping,
                Some(&*overlay),
                |_| panic!("covered"),
                |_| Ok(()),
            )
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(workspace.bytes() + output.bytes(), storage);
    let pixels = output.texture.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(126, 126), Color::rgb(255, 0, 0));
}
