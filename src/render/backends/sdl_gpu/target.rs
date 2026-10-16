use alloc::{boxed::Box, rc::Rc};
use core::{
    any::Any,
    cell::{Cell, RefCell, RefMut},
};
use sdl2::render::{RenderTarget, Texture as SdlTexture};
use sdl2::video::Window;

use super::{
    device::Device,
    readback,
    target_state::{self, DriverState},
};
use crate::render::texture::{ColorFormat, TexturePixelsMut};
use crate::render::{
    BackendTarget, PlaneLayout, RenderError, RenderFeature, TargetAlpha, TargetDescriptor,
    TargetPlan, TargetUpdate,
};
use crate::types::{PhysicalRect, Viewport};

#[cfg(test)]
#[path = "target_tests.rs"]
pub(crate) mod tests;

struct Storage<T: RenderTarget> {
    texture: RefCell<SdlTexture<'static>>,
    sampleable: bool,
    device: Rc<Device<T>>,
    descriptor: TargetDescriptor,
    generation: u64,
    completed: Cell<bool>,
    active: Cell<bool>,
}

pub(super) struct Target<T: RenderTarget = Window> {
    storage: Rc<Storage<T>>,
}

impl<T: RenderTarget + 'static> Target<T> {
    pub(super) fn sample_texture(
        &self,
        device: &Rc<Device<T>>,
        direct: bool,
    ) -> Result<Option<RefMut<'_, SdlTexture<'static>>>, RenderError> {
        self.validate()?;
        if self.storage.active.get() || !self.storage.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        if !direct || !self.storage.sampleable || !Rc::ptr_eq(device, &self.storage.device) {
            return Ok(None);
        }
        self.storage
            .texture
            .try_borrow_mut()
            .map(Some)
            .map_err(|_| RenderError::InvalidScope)
    }
}

pub struct TargetState {
    binding: Binding<Window>,
    pub(super) viewport: Viewport,
    pub(super) failure: Option<RenderError>,
    pub(super) native: bool,
}

struct Binding<T: RenderTarget> {
    child: Rc<Storage<T>>,
    driver: DriverState,
}

impl<T: RenderTarget + 'static> BackendTarget for Target<T> {
    fn descriptor(&self) -> TargetDescriptor {
        self.storage.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.storage.descriptor.layout.required_bytes()
    }
    fn validate(&self) -> Result<(), RenderError> {
        self.storage.device.check()?;
        if self.storage.generation != self.storage.device.generation() {
            return Err(RenderError::InvalidTexture);
        }
        Ok(())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read_pixels_into(&self, dst: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        self.validate()?;
        let layout = self.descriptor().layout;
        if !self.storage.completed.get()
            || self.storage.active.get()
            || dst.width() != layout.width()
            || dst.height() != layout.height()
            || dst.format() != layout.format()
        {
            return Err(RenderError::InvalidTexture);
        }
        let device = &self.storage.device;
        let texture = self
            .storage
            .texture
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let canvas = device.canvas()?;
        let saved = DriverState::capture(&canvas)?;
        let result = target_state::select(&canvas, &texture).and_then(|()| {
            let stride = dst.stride();
            readback::rgba8(
                &canvas,
                PhysicalRect::from_size(layout.width(), layout.height()),
                dst.bytes_mut(),
                stride,
            )?;
            readback::convert_alpha(
                dst.bytes_mut(),
                layout.width(),
                layout.height(),
                stride,
                TargetAlpha::Premultiplied,
                TargetAlpha::Straight,
            )
        });
        let restored = saved.restore(&canvas);
        if restored.is_err() {
            device.poison();
        }
        result.and(restored)
    }
}

pub(super) fn plan<T: RenderTarget>(
    device: &Device<T>,
    layout: PlaneLayout,
) -> Result<Option<TargetPlan>, RenderError> {
    device.check()?;
    if !device.target_supported
        || layout.format() != ColorFormat::RGBA8888
        || PlaneLayout::packed(layout.width(), layout.height(), layout.format()).ok()
            != Some(layout)
        || (device.max_texture_size.0 != 0 && u32::from(layout.width()) > device.max_texture_size.0)
        || (device.max_texture_size.1 != 0
            && u32::from(layout.height()) > device.max_texture_size.1)
    {
        return Ok(None);
    }
    Ok(Some(TargetPlan {
        descriptor: TargetDescriptor {
            layout,
            alpha: TargetAlpha::Premultiplied,
        },
        storage_bytes: layout.required_bytes(),
        workspace_bytes: 0,
        readback_bytes: 0,
    }))
}

pub(super) fn create<T: RenderTarget + 'static>(
    device: &Rc<Device<T>>,
    requested: &TargetPlan,
) -> Result<Box<dyn BackendTarget>, RenderError> {
    if plan(device, requested.descriptor.layout)? != Some(*requested) {
        return Err(RenderError::Unsupported(RenderFeature::Scope));
    }
    let _lease = device.canvas()?;
    let texture = device
        .creator
        .create_texture_target(
            sdl2::pixels::PixelFormatEnum::RGBA32,
            u32::from(requested.descriptor.layout.width()),
            u32::from(requested.descriptor.layout.height()),
        )
        .map_err(|_| RenderError::BackendFailure)?;
    super::texture_cache::nearest(&texture)?;
    let sampleable = premultiplied_sampling(&texture);
    // Storage drops the private texture before its retained creator/renderer owner.
    let texture = unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
    Ok(Box::new(Target {
        storage: Rc::new(Storage {
            texture: RefCell::new(texture),
            sampleable,
            device: Rc::clone(device),
            descriptor: requested.descriptor,
            generation: device.generation(),
            completed: Cell::new(false),
            active: Cell::new(false),
        }),
    }))
}

fn premultiplied_sampling(texture: &SdlTexture<'_>) -> bool {
    use sdl2_sys::{SDL_BlendFactor, SDL_BlendOperation};
    // SDL custom modes are u32 bitfields, not valid variants of sdl2-sys's closed Rust enum.
    unsafe extern "C" {
        #[link_name = "SDL_ComposeCustomBlendMode"]
        fn compose(
            src_color: SDL_BlendFactor,
            dst_color: SDL_BlendFactor,
            color_op: SDL_BlendOperation,
            src_alpha: SDL_BlendFactor,
            dst_alpha: SDL_BlendFactor,
            alpha_op: SDL_BlendOperation,
        ) -> u32;
        #[link_name = "SDL_SetTextureBlendMode"]
        fn set(texture: *mut sdl2_sys::SDL_Texture, mode: u32) -> core::ffi::c_int;
    }
    // Both signatures retain SDL's C ABI; the live texture belongs to the borrowed device.
    unsafe {
        use SDL_BlendFactor::*;
        use SDL_BlendOperation::*;
        let mode = compose(
            SDL_BLENDFACTOR_ONE,
            SDL_BLENDFACTOR_ONE_MINUS_SRC_ALPHA,
            SDL_BLENDOPERATION_ADD,
            SDL_BLENDFACTOR_ONE,
            SDL_BLENDFACTOR_ONE_MINUS_SRC_ALPHA,
            SDL_BLENDOPERATION_ADD,
        );
        set(texture.raw(), mode) == 0
    }
}

impl<T: RenderTarget + 'static> Binding<T> {
    fn begin(
        device: &Rc<Device<T>>,
        target: &dyn BackendTarget,
        update: TargetUpdate,
    ) -> Result<Self, RenderError> {
        let target = target
            .as_any()
            .downcast_ref::<Target<T>>()
            .ok_or(RenderError::InvalidTexture)?;
        target.validate()?;
        let child = &target.storage;
        if !Rc::ptr_eq(device, &child.device)
            || child.active.get()
            || (update == TargetUpdate::Incremental && !child.completed.get())
        {
            return Err(RenderError::InvalidTexture);
        }
        let texture = child
            .texture
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let canvas = device.canvas()?;
        let driver = DriverState::capture(&canvas)?;
        child.completed.set(false);
        let result = target_state::select(&canvas, &texture).and_then(|()| {
            if update == TargetUpdate::Rebuild {
                target_state::clear(&canvas)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            if driver.restore(&canvas).is_err() {
                device.poison();
            }
            return Err(error);
        }
        child.active.set(true);
        Ok(Self {
            child: Rc::clone(child),
            driver,
        })
    }

    fn finish(self, device: &Rc<Device<T>>, completed: bool) -> Result<(), RenderError> {
        if !Rc::ptr_eq(device, &self.child.device) {
            device.poison();
            return Err(RenderError::InvalidScope);
        }
        let result = device.canvas().and_then(|canvas| {
            let texture = self
                .child
                .texture
                .try_borrow()
                .map_err(|_| RenderError::InvalidScope)?;
            if self.child.generation != device.generation()
                || !target_state::is_current(&canvas, &texture)
            {
                return Err(RenderError::InvalidScope);
            }
            self.driver.restore(&canvas)
        });
        self.child.active.set(false);
        self.child.completed.set(completed && result.is_ok());
        if result.is_err() {
            device.poison();
        }
        result
    }
}

impl<T: RenderTarget> Drop for Binding<T> {
    fn drop(&mut self) {
        if self.child.active.replace(false) {
            // An abandoned binding may invalidate raw parent handles saved by nested scopes.
            self.child.completed.set(false);
            self.child.device.poison();
        }
    }
}

pub(super) fn bind(
    device: &Rc<Device>,
    target: &dyn BackendTarget,
    update: TargetUpdate,
    viewport: Viewport,
    parent_viewport: Viewport,
    failure: Option<RenderError>,
    native: bool,
) -> Result<TargetState, RenderError> {
    let layout = target.descriptor().layout;
    if viewport.physical_size() != (layout.width(), layout.height()) {
        return Err(RenderError::InvalidGeometry);
    }
    Ok(TargetState {
        binding: Binding::begin(device, target, update)?,
        viewport: parent_viewport,
        failure,
        native,
    })
}

pub(super) fn restore(
    device: &Rc<Device>,
    state: TargetState,
    completed: bool,
) -> Result<(), RenderError> {
    state.binding.finish(device, completed)
}
