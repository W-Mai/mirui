use alloc::{boxed::Box, rc::Rc};
use core::{
    any::Any,
    cell::{Cell, RefCell},
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
use crate::types::Viewport;

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
    #[cfg(feature = "sync-cache")]
    charge: core::cell::OnceCell<crate::render::target::TargetStorage>,
}

pub(super) struct Target<T: RenderTarget = Window> {
    storage: Rc<Storage<T>>,
}

impl<T: RenderTarget + 'static> Target<T> {
    pub(super) fn with_sample_texture<R>(
        &self,
        device: &Rc<Device<T>>,
        direct: bool,
        draw: impl FnOnce(&mut SdlTexture<'_>) -> R,
    ) -> Result<Option<R>, RenderError> {
        self.validate()?;
        if self.storage.active.get() || !self.storage.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        if !direct || !self.storage.sampleable || !Rc::ptr_eq(device, &self.storage.device) {
            return Ok(None);
        }
        let mut texture = self
            .storage
            .texture
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?;
        Ok(Some(draw(&mut texture)))
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
    selected: core::ptr::NonNull<sdl2_sys::SDL_Texture>,
}

impl<T: RenderTarget + 'static> BackendTarget for Target<T> {
    fn descriptor(&self) -> TargetDescriptor {
        self.storage.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.storage.descriptor.layout.required_bytes() + receipt_bytes()
    }
    #[cfg(feature = "sync-cache")]
    fn retain_storage(
        &mut self,
        storage: crate::render::target::TargetStorage,
    ) -> Result<(), RenderError> {
        self.storage
            .charge
            .set(storage)
            .map_err(|_| RenderError::InvalidScope)
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
    fn read_pixels_into(
        &self,
        region: crate::render::texture::Region,
        dst: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        let area = self.descriptor().readback_region(region, dst)?;
        self.validate()?;
        if !self.storage.completed.get() || self.storage.active.get() {
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
        let result = target_state::select(&canvas, &texture).and_then(|_| {
            let stride = dst.stride();
            readback::rgba8(&canvas, area, dst.bytes_mut(), stride)?;
            readback::convert_alpha(
                dst.bytes_mut(),
                area.width(),
                area.height(),
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

const fn receipt_bytes() -> usize {
    #[cfg(feature = "sync-cache")]
    {
        core::mem::size_of::<core::cell::OnceCell<crate::render::target::TargetStorage>>()
    }
    #[cfg(not(feature = "sync-cache"))]
    {
        0
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
        storage_bytes: layout.required_bytes().checked_add(receipt_bytes()).ok_or(
            RenderError::ResourceLimit(crate::render::RenderResource::Target),
        )?,
        workspace_bytes: 0,
        readback_bytes: 0,
    }))
}

fn matching_target<'a, T: RenderTarget + 'static>(
    device: &Rc<Device<T>>,
    target: &'a dyn BackendTarget,
    requested: &TargetPlan,
) -> Result<Option<&'a Target<T>>, RenderError> {
    device.check()?;
    let Some(target) = target.as_any().downcast_ref::<Target<T>>() else {
        return Ok(None);
    };
    let child = &target.storage;
    if !Rc::ptr_eq(device, &child.device) || child.generation != device.generation() {
        return Ok(None);
    }
    if child.active.get() {
        return Err(RenderError::InvalidScope);
    }
    let _texture = child
        .texture
        .try_borrow()
        .map_err(|_| RenderError::InvalidScope)?;
    let _canvas = device.canvas()?;
    if child.descriptor != requested.descriptor
        || plan(device, requested.descriptor.layout)? != Some(*requested)
    {
        return Ok(None);
    }
    Ok(Some(target))
}

pub(super) fn can_reuse<T: RenderTarget + 'static>(
    device: &Rc<Device<T>>,
    target: &dyn BackendTarget,
    plan: &TargetPlan,
) -> Result<bool, RenderError> {
    Ok(matching_target(device, target, plan)?.is_some())
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
            #[cfg(feature = "sync-cache")]
            charge: Default::default(),
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
        let requested =
            plan(device, target.descriptor().layout)?.ok_or(RenderError::InvalidTexture)?;
        let target =
            matching_target(device, target, &requested)?.ok_or(RenderError::InvalidTexture)?;
        let child = &target.storage;
        if update == TargetUpdate::Incremental && !child.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        let texture = child
            .texture
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let canvas = device.canvas()?;
        let driver = DriverState::capture(&canvas)?;
        child.completed.set(false);
        let result = target_state::select(&canvas, &texture).and_then(|selected| {
            if update == TargetUpdate::Rebuild {
                target_state::clear(&canvas)?;
            }
            Ok(selected)
        });
        let selected = match result {
            Ok(selected) => selected,
            Err(error) => {
                if driver.restore(&canvas).is_err() {
                    device.poison();
                }
                return Err(error);
            }
        };
        child.active.set(true);
        Ok(Self {
            child: Rc::clone(child),
            driver,
            selected,
        })
    }

    fn finish(self, device: &Rc<Device<T>>, completed: bool) -> Result<(), RenderError> {
        if !Rc::ptr_eq(device, &self.child.device) {
            device.poison();
            return Err(RenderError::InvalidScope);
        }
        let result = device.canvas().and_then(|canvas| {
            let _texture = self
                .child
                .texture
                .try_borrow()
                .map_err(|_| RenderError::InvalidScope)?;
            if self.child.generation != device.generation()
                || !target_state::is_current(&canvas, self.selected)
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
