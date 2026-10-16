use core::cell::{Cell, RefCell, RefMut};

use sdl2::render::{Canvas, RenderTarget, TextureCreator};
use sdl2::video::Window;

use crate::render::RenderError;

pub(super) struct Device<T: RenderTarget = Window> {
    canvas: RefCell<Canvas<T>>,
    pub(super) creator: TextureCreator<T::Context>,
    generation: Cell<u64>,
    name: &'static str,
}

impl<T: RenderTarget> Device<T> {
    pub(super) fn new(canvas: Canvas<T>, creator: TextureCreator<T::Context>) -> Self {
        assert_eq!(canvas.raw(), creator.raw());
        let name = canvas.info().name;
        Self {
            canvas: RefCell::new(canvas),
            creator,
            generation: Cell::new(0),
            name,
        }
    }

    pub(super) fn canvas(&self) -> Result<RefMut<'_, Canvas<T>>, RenderError> {
        self.canvas
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)
    }

    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation.get()
    }

    pub(super) fn invalidate(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;
    use crate::types::PhysicalRect;
    use alloc::rc::Rc;

    fn device() -> Rc<Device<sdl2::surface::Surface<'static>>> {
        let canvas = sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        let creator = canvas.texture_creator();
        Rc::new(Device::new(canvas, creator))
    }

    #[test]
    fn short_leases_reject_reentry_and_release_after_unwind() {
        let device = device();
        let lease = device.canvas().unwrap();
        assert!(matches!(device.canvas(), Err(RenderError::InvalidScope)));
        drop(lease);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lease = device.canvas().unwrap();
            panic!("abort driver operation");
        }));
        assert!(result.is_err());
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..16 {
                    let canvas = device.canvas().unwrap();
                    assert_eq!(canvas.output_size().unwrap(), (4, 3));
                }
            }),
            0
        );
    }

    #[test]
    fn retained_owner_keeps_canvas_and_shared_generation_alive() {
        let surface_owner = device();
        let resource_owner = Rc::clone(&surface_owner);
        let weak = Rc::downgrade(&surface_owner);
        surface_owner.invalidate();
        assert_eq!(resource_owner.generation(), 1);
        drop(surface_owner);
        {
            let mut canvas = resource_owner.canvas().unwrap();
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(17, 39, 83, 127));
            canvas.clear();
        }
        let mut pixels = [0; 4];
        super::super::readback::rgba8(
            &*resource_owner.canvas().unwrap(),
            PhysicalRect::new(0, 0, 1, 1).unwrap(),
            &mut pixels,
            4,
        )
        .unwrap();
        assert_eq!(pixels, [17, 39, 83, 127]);
        resource_owner.invalidate();
        assert_eq!(resource_owner.generation(), 2);
        drop(resource_owner);
        assert!(weak.upgrade().is_none());
    }
}
