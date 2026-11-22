#[cfg(not(feature = "sync"))]
use core::cell::Cell;
use core::cell::{RefCell, RefMut};

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{BlendMode, Canvas, RenderTarget, Texture as SdlTexture, TextureCreator};
use sdl2::video::Window;

use super::target_state::{self, DriverState};
use crate::render::RenderError;

#[cfg(feature = "sync")]
#[path = "device_status.rs"]
mod shared;
#[cfg(feature = "sync")]
pub(super) use shared::DeviceStatus;

struct PersistentTarget {
    texture: SdlTexture<'static>,
    size: (u32, u32),
    initialized: bool,
}

pub(super) struct Device<T: RenderTarget = Window> {
    canvas: RefCell<Canvas<T>>,
    pub(super) creator: TextureCreator<T::Context>,
    #[cfg(not(feature = "sync"))]
    generation: Cell<u64>,
    #[cfg(not(feature = "sync"))]
    failure: Cell<Option<RenderError>>,
    #[cfg(feature = "sync")]
    pub(super) status: alloc::sync::Arc<DeviceStatus>,
    #[cfg(feature = "sync")]
    targets: RefCell<crate::render::target::ThreadOwner>,
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
        self.abort_frame();
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
            #[cfg(not(feature = "sync"))]
            generation: Cell::new(0),
            #[cfg(not(feature = "sync"))]
            failure: Cell::new(None),
            #[cfg(feature = "sync")]
            status: alloc::sync::Arc::new(DeviceStatus::new()),
            #[cfg(feature = "sync")]
            targets: RefCell::new(crate::render::target::ThreadOwner::new()),
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
        #[cfg(feature = "sync")]
        {
            self.status.check()
        }
        #[cfg(not(feature = "sync"))]
        self.failure.get().map_or(Ok(()), Err)
    }

    pub(super) fn poison(&self) {
        #[cfg(feature = "sync")]
        self.status.poison();
        #[cfg(not(feature = "sync"))]
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
        self.frame_active() && self.has_persistent_target()
    }

    pub(super) fn frame_active(&self) -> bool {
        self.frame_state.borrow().is_some()
    }

    pub(super) fn finish_draw(&self, canvas: &mut Canvas<T>) {
        if !self.frame_active() {
            canvas.set_clip_rect(None);
        }
    }

    pub(super) fn begin_frame(&self) -> Result<bool, RenderError> {
        if self.frame_state.borrow().is_some() {
            return Err(RenderError::InvalidScope);
        }
        let canvas = self.canvas()?;
        let mut target = self.persistent.borrow_mut();
        let driver = DriverState::capture(&canvas)?;
        if let Some(target) = target.as_mut() {
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
        }
        *self.frame_state.borrow_mut() = Some(driver);
        Ok(target.is_some())
    }

    pub(super) fn finish_frame(&self) -> Result<bool, RenderError> {
        crate::trace_span!("sdl.frame.compose");
        let mut canvas = self.canvas()?;
        let Some(driver) = self.frame_state.borrow_mut().take() else {
            return Ok(false);
        };
        let target = self.persistent.borrow();
        let Some(target) = target.as_ref() else {
            let result = driver.restore(&canvas);
            if result.is_err() {
                self.poison();
            }
            return result.map(|()| false);
        };
        let texture = &target.texture;
        let result = (|| {
            driver.restore_target(&canvas)?;
            let copy = (|| {
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
                })?;
                Ok(())
            })();
            let restored = driver.restore_properties(&canvas);
            copy.and(restored)
        })();
        if result.is_err() {
            self.poison();
        }
        result.map(|()| true)
    }

    pub(super) fn abort_frame(&self) {
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
        #[cfg(feature = "sync")]
        {
            self.status.generation()
        }
        #[cfg(not(feature = "sync"))]
        self.generation.get()
    }

    pub(super) fn invalidate(&self) {
        self.abort_frame();
        self.persistent.borrow_mut().take();
        #[cfg(feature = "sync")]
        {
            self.status.poison();
            if self.close_targets().is_err() {
                return;
            }
            *self.targets.borrow_mut() = crate::render::target::ThreadOwner::new();
            self.status.reset();
        }
        #[cfg(not(feature = "sync"))]
        {
            self.generation.set(self.generation.get().wrapping_add(1));
            self.failure.set(None);
        }
    }

    #[cfg(feature = "sync")]
    pub(super) fn target_metadata_bytes<R: 'static>(&self) -> Result<usize, RenderError> {
        self.targets
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?
            .metadata_bytes::<R>()?
            .checked_add(self.status.metadata_bytes())
            .ok_or(RenderError::InvalidGeometry)
    }

    #[cfg(feature = "sync")]
    pub(super) fn register_target<R: 'static>(
        &self,
        value: R,
    ) -> Result<crate::render::target::ThreadResource<R>, RenderError> {
        self.targets
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?
            .insert(value)
    }

    #[cfg(feature = "sync")]
    pub(super) fn collect_targets(&self) -> Result<(), RenderError> {
        drop(
            self.canvas
                .try_borrow_mut()
                .map_err(|_| RenderError::InvalidScope)?,
        );
        self.targets
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?
            .collect()
    }

    #[cfg(feature = "sync")]
    pub(super) fn close_targets(&self) -> Result<(), RenderError> {
        drop(
            self.canvas
                .try_borrow_mut()
                .map_err(|_| RenderError::InvalidScope)?,
        );
        self.targets
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?
            .close()
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/device.rs"]
mod tests;
