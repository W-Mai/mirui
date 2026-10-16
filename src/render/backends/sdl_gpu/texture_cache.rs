use alloc::rc::Rc;
use core::cell::RefCell;

use sdl2::render::{RenderTarget, Texture as SdlTexture};
use sdl2::video::Window;

use super::SdlGpuRenderer;
use super::device::Device;
use crate::core::cache::{CacheInspect, HasSize, LruCache, MaxSize};
use crate::render::renderer::RenderError;
use crate::render::texture::{Texture, TextureKey};
use crate::render::{TargetAlpha, command::CompositeMode};

pub(super) const DEFAULT_BUDGET: usize = 16 * 1024 * 1024;

struct CachedTexture {
    texture: SdlTexture<'static>,
    bytes: usize,
}

impl HasSize for CachedTexture {
    fn cache_size(&self) -> usize {
        self.bytes
    }
}

pub(super) struct TextureCache<T: RenderTarget = Window> {
    // Cached textures are private and dropped before their owning renderer context.
    pool: LruCache<TextureKey, RefCell<CachedTexture>>,
    device: Rc<Device<T>>,
    generation: u64,
}

impl<T: RenderTarget + 'static> TextureCache<T> {
    pub(super) fn new(device: Rc<Device<T>>, budget: usize) -> Self {
        Self {
            pool: LruCache::builder()
                .max_size(MaxSize::Bytes(budget))
                .name("sdl_gpu/texture")
                .build(),
            generation: device.generation(),
            device,
        }
    }

    pub(super) fn matches(&self, device: &Rc<Device<T>>) -> bool {
        Rc::ptr_eq(&self.device, device) && self.generation == device.generation()
    }

    pub(super) fn inspect(&self) -> &dyn CacheInspect {
        &self.pool
    }

    pub(super) fn with_texture<R>(
        &mut self,
        src: &Texture,
        composite: CompositeMode,
        draw: impl FnOnce(&mut SdlTexture<'_>, TargetAlpha) -> R,
    ) -> Result<R, RenderError> {
        self.device.check()?;
        if let Some(target) = src.backend_target()?
            && let Some(target) = target.as_any().downcast_ref::<super::target::Target<T>>()
            && let Some(mut texture) =
                target.sample_texture(&self.device, composite == CompositeMode::SourceOver)?
        {
            return Ok(draw(&mut texture, TargetAlpha::Premultiplied));
        }
        let bytes = usize::from(src.width) * usize::from(src.height) * 4;
        let key = TextureKey::cacheable(src)
            .filter(|_| matches!(self.pool.max_size(), MaxSize::Bytes(limit) if bytes <= *limit));
        let Some(key) = key else {
            let mut texture = upload(&self.device, src)?;
            return Ok(draw(&mut texture, TargetAlpha::Straight));
        };
        let handle = self.pool.entry(key).or_try_insert_with(|| {
            let texture = upload(&self.device, src)?;
            // Only this pool retains the erased lifetime; no handle escapes this module.
            // Its creator owns the renderer and is dropped after every cached texture.
            let texture =
                unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
            Ok::<_, RenderError>(RefCell::new(CachedTexture { texture, bytes }))
        })?;
        let mut cached = handle
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?;
        Ok(draw(&mut cached.texture, TargetAlpha::Straight))
    }
}

fn upload<'a, T: RenderTarget>(
    device: &'a Device<T>,
    src: &Texture,
) -> Result<SdlTexture<'a>, RenderError> {
    let pixels = src.read_pixels()?;
    let _lease = device.canvas()?;
    let mut texture = device
        .creator
        .create_texture_streaming(
            SdlGpuRenderer::<Box<[u8]>>::texture_format(pixels.format()),
            u32::from(pixels.width()),
            u32::from(pixels.height()),
        )
        .map_err(|_| RenderError::BackendFailure)?;
    SdlGpuRenderer::<Box<[u8]>>::upload_texture(&mut texture, &pixels)?;
    nearest(&texture)?;
    Ok(texture)
}

pub(super) fn nearest(texture: &SdlTexture<'_>) -> Result<(), RenderError> {
    // SDL retains no Rust references; the texture is live for this call.
    if unsafe {
        sdl2_sys::SDL_SetTextureScaleMode(
            texture.raw(),
            sdl2_sys::SDL_ScaleMode::SDL_ScaleModeNearest,
        )
    } != 0
    {
        return Err(RenderError::BackendFailure);
    }
    Ok(())
}
