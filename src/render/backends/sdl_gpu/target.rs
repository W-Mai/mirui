use alloc::{boxed::Box, rc::Rc};
#[cfg(not(feature = "sync-cache"))]
use core::cell::Cell;
use core::{any::Any, cell::RefCell};
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
    #[cfg(not(feature = "sync-cache"))]
    sampleable: bool,
    device: Rc<Device<T>>,
    #[cfg(not(feature = "sync-cache"))]
    descriptor: TargetDescriptor,
    #[cfg(not(feature = "sync-cache"))]
    generation: u64,
    #[cfg(not(feature = "sync-cache"))]
    completed: Cell<bool>,
    #[cfg(not(feature = "sync-cache"))]
    active: Cell<bool>,
    #[cfg(feature = "sync-cache")]
    source: alloc::sync::Arc<SourceState>,
}

#[cfg(not(feature = "sync-cache"))]
type StorageRef<T> = Rc<Storage<T>>;
#[cfg(feature = "sync-cache")]
type StorageRef<T> = crate::render::target::ThreadResource<Storage<T>>;

#[cfg(feature = "sync-cache")]
struct SourceState {
    descriptor: TargetDescriptor,
    sampleable: bool,
    device: alloc::sync::Arc<super::device::DeviceStatus>,
    device_identity: usize,
    generation: u64,
    completed: core::sync::atomic::AtomicBool,
    active: core::sync::atomic::AtomicBool,
    bytes: usize,
    charge: std::sync::OnceLock<crate::render::target::TargetStorage>,
}

impl<T: RenderTarget> Storage<T> {
    fn completed(&self) -> bool {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.completed.get()
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source
                .completed
                .load(core::sync::atomic::Ordering::Acquire)
        }
    }
    fn set_completed(&self, value: bool) {
        #[cfg(not(feature = "sync-cache"))]
        self.completed.set(value);
        #[cfg(feature = "sync-cache")]
        self.source
            .completed
            .store(value, core::sync::atomic::Ordering::Release);
    }
    fn active(&self) -> bool {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.active.get()
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source
                .active
                .load(core::sync::atomic::Ordering::Acquire)
        }
    }
    fn set_active(&self, value: bool) {
        #[cfg(not(feature = "sync-cache"))]
        self.active.set(value);
        #[cfg(feature = "sync-cache")]
        self.source
            .active
            .store(value, core::sync::atomic::Ordering::Release);
    }
    fn generation(&self) -> u64 {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.generation
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source.generation
        }
    }
    fn descriptor(&self) -> TargetDescriptor {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.descriptor
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source.descriptor
        }
    }
}

fn with_storage<T: RenderTarget + 'static, R>(
    storage: &StorageRef<T>,
    body: impl FnOnce(&Storage<T>) -> Result<R, RenderError>,
) -> Result<R, RenderError> {
    #[cfg(not(feature = "sync-cache"))]
    {
        body(storage)
    }
    #[cfg(feature = "sync-cache")]
    {
        storage.with(|storage| body(storage))
    }
}

pub(super) struct Target<T: RenderTarget + 'static = Window> {
    storage: StorageRef<T>,
    #[cfg(feature = "sync-cache")]
    source: alloc::sync::Arc<SourceState>,
}

impl<T: RenderTarget + 'static> Target<T> {
    pub(super) fn with_sample_texture<R>(
        &self,
        device: &Rc<Device<T>>,
        direct: bool,
        draw: impl FnOnce(&mut SdlTexture<'_>) -> R,
    ) -> Result<Option<R>, RenderError> {
        self.validate()?;
        #[cfg(not(feature = "sync-cache"))]
        let (active, completed, sampleable, same_device) = (
            self.storage.active(),
            self.storage.completed(),
            self.storage.sampleable,
            Rc::ptr_eq(device, &self.storage.device),
        );
        #[cfg(feature = "sync-cache")]
        let (active, completed, sampleable, same_device) = (
            self.source
                .active
                .load(core::sync::atomic::Ordering::Acquire),
            self.source
                .completed
                .load(core::sync::atomic::Ordering::Acquire),
            self.source.sampleable,
            self.source.device_identity == Rc::as_ptr(device) as usize,
        );
        if active || !completed {
            return Err(RenderError::InvalidTexture);
        }
        if !direct || !sampleable || !same_device {
            return Ok(None);
        }
        with_storage(&self.storage, |storage| {
            let mut texture = storage
                .texture
                .try_borrow_mut()
                .map_err(|_| RenderError::InvalidScope)?;
            Ok(Some(draw(&mut texture)))
        })
    }
}

pub struct TargetState {
    binding: Binding<Window>,
    pub(super) viewport: Viewport,
    pub(super) failure: Option<RenderError>,
    pub(super) native: bool,
}

struct Binding<T: RenderTarget + 'static> {
    child: StorageRef<T>,
    #[cfg(feature = "sync-cache")]
    source: alloc::sync::Arc<SourceState>,
    #[cfg(feature = "sync-cache")]
    device: Rc<Device<T>>,
    driver: DriverState,
    selected: core::ptr::NonNull<sdl2_sys::SDL_Texture>,
}

impl<T: RenderTarget + 'static> BackendTarget for Target<T> {
    fn descriptor(&self) -> TargetDescriptor {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.storage.descriptor
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source.descriptor
        }
    }
    fn allocated_bytes(&self) -> usize {
        #[cfg(not(feature = "sync-cache"))]
        {
            self.storage.descriptor.layout.required_bytes()
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source.bytes
        }
    }
    #[cfg(feature = "sync-cache")]
    fn retain_storage(
        &mut self,
        storage: crate::render::target::TargetStorage,
    ) -> Result<(), RenderError> {
        self.storage.retain_storage(storage.clone())?;
        self.source.device.retain_storage(&storage)?;
        self.source
            .charge
            .set(storage)
            .map_err(|_| RenderError::InvalidScope)
    }
    fn validate(&self) -> Result<(), RenderError> {
        #[cfg(feature = "sync-cache")]
        {
            self.storage.validate()?;
            self.source.device.check_generation(self.source.generation)
        }
        #[cfg(not(feature = "sync-cache"))]
        {
            self.storage.device.check()?;
            if self.storage.generation != self.storage.device.generation() {
                return Err(RenderError::InvalidTexture);
            }
            Ok(())
        }
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
        with_storage(&self.storage, |storage| {
            if !storage.completed() || storage.active() {
                return Err(RenderError::InvalidTexture);
            }
            let device = &storage.device;
            let texture = storage
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
        })
    }
}

fn ownership_bytes<T: RenderTarget + 'static>(device: &Device<T>) -> Result<usize, RenderError> {
    #[cfg(feature = "sync-cache")]
    {
        device
            .target_metadata_bytes::<Storage<T>>()?
            .checked_add(core::mem::size_of::<SourceState>() + 2 * core::mem::size_of::<usize>())
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Target<T>>()))
            .ok_or(RenderError::InvalidGeometry)
    }
    #[cfg(not(feature = "sync-cache"))]
    {
        let _ = device;
        Ok(0)
    }
}

pub(super) fn plan<T: RenderTarget + 'static>(
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
        storage_bytes: layout
            .required_bytes()
            .checked_add(ownership_bytes(device)?)
            .ok_or(RenderError::ResourceLimit(
                crate::render::RenderResource::Target,
            ))?,
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
    #[cfg(not(feature = "sync-cache"))]
    let (same_device, generation) = (
        Rc::ptr_eq(device, &target.storage.device),
        target.storage.generation(),
    );
    #[cfg(feature = "sync-cache")]
    let (same_device, generation) = (
        target.source.device_identity == Rc::as_ptr(device) as usize,
        target.source.generation,
    );
    if !same_device || generation != device.generation() {
        return Ok(None);
    }
    #[cfg(feature = "sync-cache")]
    if target.storage.validate().is_err() {
        return Ok(None);
    }
    with_storage(&target.storage, |child| {
        if child.active() {
            return Err(RenderError::InvalidScope);
        }
        let _texture = child
            .texture
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let _canvas = device.canvas()?;
        if child.descriptor() != requested.descriptor || !compatible_plan(device, requested)? {
            return Ok(None);
        }
        Ok(Some(target))
    })
}

fn compatible_plan<T: RenderTarget + 'static>(
    device: &Device<T>,
    requested: &TargetPlan,
) -> Result<bool, RenderError> {
    let Some(current) = plan(device, requested.descriptor.layout)? else {
        return Ok(false);
    };
    #[cfg(not(feature = "sync-cache"))]
    {
        Ok(current == *requested)
    }
    #[cfg(feature = "sync-cache")]
    {
        // Earlier plans may include owner metadata admitted by another target in the same batch.
        Ok(current.descriptor == requested.descriptor
            && current.storage_bytes <= requested.storage_bytes
            && current.workspace_bytes == requested.workspace_bytes
            && current.readback_bytes == requested.readback_bytes)
    }
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
    #[cfg(feature = "sync-cache")]
    device.collect_targets()?;
    if !compatible_plan(device, requested)? {
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
    #[cfg(feature = "sync-cache")]
    let source = alloc::sync::Arc::new(SourceState {
        descriptor: requested.descriptor,
        sampleable,
        device: device.status.clone(),
        device_identity: Rc::as_ptr(device) as usize,
        generation: device.generation(),
        completed: core::sync::atomic::AtomicBool::new(false),
        active: core::sync::atomic::AtomicBool::new(false),
        bytes: requested
            .descriptor
            .layout
            .required_bytes()
            .checked_add(ownership_bytes(device)?)
            .ok_or(RenderError::InvalidGeometry)?,
        charge: std::sync::OnceLock::new(),
    });
    let storage = Storage {
        texture: RefCell::new(texture),
        #[cfg(not(feature = "sync-cache"))]
        sampleable,
        device: Rc::clone(device),
        #[cfg(not(feature = "sync-cache"))]
        descriptor: requested.descriptor,
        #[cfg(not(feature = "sync-cache"))]
        generation: device.generation(),
        #[cfg(not(feature = "sync-cache"))]
        completed: Cell::new(false),
        #[cfg(not(feature = "sync-cache"))]
        active: Cell::new(false),
        #[cfg(feature = "sync-cache")]
        source: source.clone(),
    };
    #[cfg(not(feature = "sync-cache"))]
    let storage = Rc::new(storage);
    #[cfg(feature = "sync-cache")]
    let storage = device.register_target(storage)?;
    Ok(Box::new(Target {
        storage,
        #[cfg(feature = "sync-cache")]
        source,
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
        let (driver, selected) = with_storage(&target.storage, |child| {
            if update == TargetUpdate::Incremental && !child.completed() {
                return Err(RenderError::InvalidTexture);
            }
            let texture = child
                .texture
                .try_borrow()
                .map_err(|_| RenderError::InvalidScope)?;
            let canvas = device.canvas()?;
            let driver = DriverState::capture(&canvas)?;
            child.set_completed(false);
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
            child.set_active(true);
            Ok((driver, selected))
        })?;
        Ok(Self {
            child: target.storage.clone(),
            #[cfg(feature = "sync-cache")]
            source: target.source.clone(),
            #[cfg(feature = "sync-cache")]
            device: device.clone(),
            driver,
            selected,
        })
    }

    fn finish(self, device: &Rc<Device<T>>, completed: bool) -> Result<(), RenderError> {
        #[cfg(not(feature = "sync-cache"))]
        let same_device = Rc::ptr_eq(device, &self.child.device);
        #[cfg(feature = "sync-cache")]
        let same_device = Rc::ptr_eq(device, &self.device);
        if !same_device {
            device.poison();
            return Err(RenderError::InvalidScope);
        }
        let result = device
            .canvas()
            .and_then(|canvas| {
                with_storage(&self.child, |child| {
                    // Validate the source generation before using any saved raw target handle.
                    if child.generation() != device.generation() {
                        return Err(RenderError::InvalidScope);
                    }
                    let _texture = child
                        .texture
                        .try_borrow()
                        .map_err(|_| RenderError::InvalidScope)?;
                    if !target_state::is_current(&canvas, self.selected) {
                        return Err(RenderError::InvalidScope);
                    }
                    self.driver.restore(&canvas)
                })
            })
            .map_err(|error| {
                if error == RenderError::InvalidTexture {
                    RenderError::InvalidScope
                } else {
                    error
                }
            });
        #[cfg(not(feature = "sync-cache"))]
        {
            self.child.active.set(false);
            self.child.completed.set(completed && result.is_ok());
        }
        #[cfg(feature = "sync-cache")]
        {
            self.source
                .active
                .store(false, core::sync::atomic::Ordering::Release);
            self.source.completed.store(
                completed && result.is_ok(),
                core::sync::atomic::Ordering::Release,
            );
        }
        if result.is_err() {
            device.poison();
        }
        result
    }
}

impl<T: RenderTarget + 'static> Drop for Binding<T> {
    fn drop(&mut self) {
        #[cfg(not(feature = "sync-cache"))]
        let active = self.child.active.replace(false);
        #[cfg(feature = "sync-cache")]
        let active = self
            .source
            .active
            .swap(false, core::sync::atomic::Ordering::AcqRel);
        if active {
            // An abandoned binding may invalidate raw parent handles saved by nested scopes.
            #[cfg(not(feature = "sync-cache"))]
            {
                self.child.completed.set(false);
                self.child.device.poison();
            }
            #[cfg(feature = "sync-cache")]
            {
                self.source
                    .completed
                    .store(false, core::sync::atomic::Ordering::Release);
                self.device.poison();
            }
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
