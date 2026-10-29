use core::cell::{Cell, RefCell, RefMut};

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{BlendMode, Canvas, RenderTarget, Texture as SdlTexture, TextureCreator};
use sdl2::video::Window;

use super::target_state::{self, DriverState};
use crate::render::RenderError;

struct PersistentTarget {
    texture: SdlTexture<'static>,
    size: (u32, u32),
    initialized: bool,
}

pub(super) struct Device<T: RenderTarget = Window> {
    canvas: RefCell<Canvas<T>>,
    pub(super) creator: TextureCreator<T::Context>,
    generation: Cell<u64>,
    failure: Cell<Option<RenderError>>,
    persistent: RefCell<Option<PersistentTarget>>,
    frame_state: RefCell<Option<DriverState>>,
    persistent_supported: bool,
    pub(super) target_supported: bool,
    pub(super) max_texture_size: (u32, u32),
    name: &'static str,
}

impl<T: RenderTarget> Drop for Device<T> {
    fn drop(&mut self) {
        // The retained texture borrows the renderer represented by `canvas`.
        // Drop it before Canvas releases that renderer; the field order cannot
        // express this because TextureCreator is only a lifetime witness.
        self.abort_persistent_frame();
        self.persistent.get_mut().take();
    }
}

impl<T: RenderTarget> Device<T> {
    pub(super) fn new(
        canvas: Canvas<T>,
        creator: TextureCreator<T::Context>,
    ) -> Result<Self, RenderError> {
        assert_eq!(canvas.raw(), creator.raw());
        let mut info = core::mem::MaybeUninit::uninit();
        // SDL can report formats newer than the Rust binding's enum. Only the
        // static name and extent limits are needed; leave format IDs opaque.
        if unsafe { sdl2::sys::SDL_GetRendererInfo(canvas.raw(), info.as_mut_ptr()) } != 0 {
            return Err(RenderError::BackendFailure);
        }
        let info = unsafe { info.assume_init() };
        // SDL driver names are static strings owned by the loaded library.
        let name = unsafe { core::ffi::CStr::from_ptr(info.name) }
            .to_str()
            .map_err(|_| RenderError::BackendFailure)?;
        let target_supported = canvas.render_target_supported();
        Ok(Self {
            canvas: RefCell::new(canvas),
            creator,
            generation: Cell::new(0),
            failure: Cell::new(None),
            persistent: RefCell::new(None),
            frame_state: RefCell::new(None),
            persistent_supported: target_supported && super::config::window_is_single_sample(name),
            target_supported,
            max_texture_size: (
                info.max_texture_width.max(0) as u32,
                info.max_texture_height.max(0) as u32,
            ),
            name,
        })
    }

    pub(super) fn canvas(&self) -> Result<RefMut<'_, Canvas<T>>, RenderError> {
        self.check()?;
        self.canvas
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)
    }

    pub(super) fn check(&self) -> Result<(), RenderError> {
        self.failure.get().map_or(Ok(()), Err)
    }

    pub(super) fn poison(&self) {
        self.failure.set(Some(RenderError::BackendFailure));
    }

    pub(super) fn prepare_persistent_target(&self, size: (u32, u32)) -> Result<bool, RenderError> {
        self.check()?;
        if !self.persistent_supported {
            return Ok(false);
        }
        if self
            .persistent
            .borrow()
            .as_ref()
            .is_some_and(|target| target.size == size)
        {
            return Ok(true);
        }
        if self.frame_state.borrow().is_some() {
            return Err(RenderError::InvalidScope);
        }
        self.persistent.borrow_mut().take();
        let mut texture = self
            .creator
            .create_texture_target(PixelFormatEnum::RGBA32, size.0, size.1)
            .map_err(|_| RenderError::BackendFailure)?;
        // Presentation replaces window pixels, including transparent pixels.
        texture.set_blend_mode(BlendMode::None);
        // TextureCreator is owned by this Device for the entire texture lifetime.
        let texture =
            unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
        *self.persistent.borrow_mut() = Some(PersistentTarget {
            texture,
            size,
            initialized: false,
        });
        Ok(true)
    }

    pub(super) fn has_persistent_target(&self) -> bool {
        self.persistent.borrow().is_some()
    }

    pub(super) fn persistent_frame_active(&self) -> bool {
        self.frame_state.borrow().is_some()
    }

    pub(super) fn finish_draw(&self, canvas: &mut Canvas<T>) {
        if !self.persistent_frame_active() {
            canvas.set_clip_rect(None);
        }
    }

    pub(super) fn begin_persistent_frame(&self) -> Result<bool, RenderError> {
        if self.persistent.borrow().is_none() {
            return Ok(false);
        }
        if self.frame_state.borrow().is_some() {
            return Err(RenderError::InvalidScope);
        }
        let canvas = self.canvas()?;
        let mut target = self.persistent.borrow_mut();
        let target = target.as_mut().ok_or(RenderError::InvalidScope)?;
        let driver = DriverState::capture(&canvas)?;
        if let Err(error) = target_state::select(&canvas, &target.texture) {
            if driver.restore(&canvas).is_err() {
                self.poison();
            }
            return Err(error);
        }
        if !target.initialized {
            if let Err(error) = target_state::clear(&canvas) {
                if driver.restore(&canvas).is_err() {
                    self.poison();
                }
                return Err(error);
            }
            target.initialized = true;
        }
        *self.frame_state.borrow_mut() = Some(driver);
        Ok(true)
    }

    pub(super) fn finish_persistent_frame(&self) -> Result<bool, RenderError> {
        crate::trace_span!("sdl.frame.compose");
        let mut canvas = self.canvas()?;
        let Some(driver) = self.frame_state.borrow_mut().take() else {
            return Ok(false);
        };
        let target = self.persistent.borrow();
        let texture = &target.as_ref().unwrap().texture;
        let result = (|| {
            driver.restore(&canvas)?;
            canvas.set_viewport(None);
            canvas.set_clip_rect(None);
            canvas
                .set_scale(1.0, 1.0)
                .map_err(|_| RenderError::BackendFailure)?;
            canvas
                .set_logical_size(0, 0)
                .map_err(|_| RenderError::BackendFailure)?;
            crate::trace_span!("sdl.frame.copy", {
                canvas
                    .copy(texture, None, None)
                    .map_err(|_| RenderError::BackendFailure)
            })?
            // SDL_RenderCopy does not mutate the target or draw state restored above.
        })();
        if result.is_err() {
            self.poison();
        }
        result.map(|()| true)
    }

    pub(super) fn abort_persistent_frame(&self) {
        let Some(driver) = self.frame_state.borrow_mut().take() else {
            return;
        };
        match self.canvas.try_borrow_mut() {
            Ok(canvas) => {
                if driver.restore(&canvas).is_err() {
                    self.poison();
                }
            }
            Err(_) => self.poison(),
        }
    }

    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation.get()
    }

    pub(super) fn invalidate(&self) {
        self.abort_persistent_frame();
        self.persistent.borrow_mut().take();
        self.generation.set(self.generation.get().wrapping_add(1));
        self.failure.set(None);
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
        Rc::new(Device::new(canvas, creator).unwrap())
    }

    fn read_pixel(device: &Device<sdl2::surface::Surface<'static>>, x: u16, y: u16) -> [u8; 4] {
        let mut pixel = [0; 4];
        super::super::readback::rgba8(
            &*device.canvas().unwrap(),
            PhysicalRect::new(x, y, 1, 1).unwrap(),
            &mut pixel,
            4,
        )
        .unwrap();
        pixel
    }

    #[test]
    fn command_clip_reuse_is_confined_to_owned_frames_and_restores_on_abort() {
        let device = device();
        let clip = sdl2::rect::Rect::new(0, 0, 1, 1);
        {
            let mut canvas = device.canvas().unwrap();
            canvas.set_clip_rect(clip);
            device.finish_draw(&mut canvas);
            assert_eq!(canvas.clip_rect(), None);
            canvas.set_clip_rect(clip);
        }
        device.prepare_persistent_target((4, 3)).unwrap();
        device.begin_persistent_frame().unwrap();
        let changed = sdl2::rect::Rect::new(1, 1, 2, 2);
        {
            let mut canvas = device.canvas().unwrap();
            canvas.set_clip_rect(changed);
            device.finish_draw(&mut canvas);
            assert_eq!(canvas.clip_rect(), Some(changed));
            let outer = DriverState::capture(&canvas).unwrap();
            let inner = device
                .creator
                .create_texture_target(PixelFormatEnum::RGBA32, 2, 2)
                .unwrap();
            target_state::select(&canvas, &inner).unwrap();
            assert_eq!(canvas.clip_rect(), None);
            canvas.set_clip_rect(clip);
            device.finish_draw(&mut canvas);
            assert_eq!(canvas.clip_rect(), Some(clip));
            outer.restore(&canvas).unwrap();
            assert_eq!(canvas.clip_rect(), Some(changed));
        }
        device.abort_persistent_frame();
        assert_eq!(device.canvas().unwrap().clip_rect(), Some(clip));
        device.begin_persistent_frame().unwrap();
        {
            let mut canvas = device.canvas().unwrap();
            canvas.set_clip_rect(changed);
            device.finish_draw(&mut canvas);
        }
        device.finish_persistent_frame().unwrap();
        assert_eq!(device.canvas().unwrap().clip_rect(), Some(clip));
    }

    #[test]
    fn persistent_frames_retain_undamaged_pixels_and_replace_window_alpha() {
        let device = device();
        assert!(device.prepare_persistent_target((4, 3)).unwrap());
        for (x, color) in [(0, [120, 30, 60, 128]), (2, [7, 19, 83, 255])] {
            assert!(device.begin_persistent_frame().unwrap());
            {
                let mut canvas = device.canvas().unwrap();
                canvas.set_blend_mode(BlendMode::None);
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                    color[0], color[1], color[2], color[3],
                ));
                canvas.fill_rect(sdl2::rect::Rect::new(x, 0, 1, 1)).unwrap();
            }
            assert!(device.finish_persistent_frame().unwrap());
            assert_eq!(read_pixel(&device, 0, 0), [120, 30, 60, 128]);
            assert_eq!(read_pixel(&device, 3, 2), [0; 4]);
            // A discarded window backbuffer cannot erase retained pixels.
            if x == 0 {
                let mut canvas = device.canvas().unwrap();
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 255));
                canvas.clear();
            }
        }
        assert_eq!(read_pixel(&device, 2, 0), [7, 19, 83, 255]);
        assert!(!device.finish_persistent_frame().unwrap());
    }

    #[test]
    fn persistent_abort_restores_driver_state_and_reset_recreates_storage() {
        let device = device();
        assert!(device.prepare_persistent_target((4, 3)).unwrap());
        let expected = {
            let mut canvas = device.canvas().unwrap();
            canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 2, 2));
            canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
            canvas.set_scale(1.5, 1.5).unwrap();
            canvas.set_blend_mode(BlendMode::Add);
            (
                canvas.viewport(),
                canvas.clip_rect(),
                canvas.scale(),
                canvas.blend_mode(),
            )
        };
        assert!(device.begin_persistent_frame().unwrap());
        assert_eq!(
            device.begin_persistent_frame(),
            Err(RenderError::InvalidScope)
        );
        assert_eq!(
            device.prepare_persistent_target((8, 6)),
            Err(RenderError::InvalidScope)
        );
        device.abort_persistent_frame();
        {
            let canvas = device.canvas().unwrap();
            assert_eq!(
                (
                    canvas.viewport(),
                    canvas.clip_rect(),
                    canvas.scale(),
                    canvas.blend_mode()
                ),
                expected
            );
        }
        device.invalidate();
        assert!(!device.has_persistent_target());
        assert!(device.prepare_persistent_target((8, 6)).unwrap());
        assert!(device.begin_persistent_frame().unwrap());
        assert_eq!(read_pixel(&device, 7, 5), [0; 4]);
        assert!(device.finish_persistent_frame().unwrap());
        let canvas = device.canvas().unwrap();
        assert_eq!(
            (
                canvas.viewport(),
                canvas.clip_rect(),
                canvas.scale(),
                canvas.blend_mode()
            ),
            expected
        );
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
