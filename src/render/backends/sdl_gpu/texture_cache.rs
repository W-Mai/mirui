use alloc::rc::Rc;
use core::cell::RefCell;

use sdl2::render::{RenderTarget, Texture as SdlTexture};
use sdl2::video::Window;

use super::SdlGpuRenderer;
use super::device::Device;
use crate::core::cache::{CacheInspect, HasSize, LruCache, MaxSize};
use crate::render::renderer::RenderError;
use crate::render::texture::{Region, Texture, TextureKey, UploadCoverage};
use crate::render::{TargetAlpha, command::CompositeMode};

pub(super) const DEFAULT_BUDGET: usize = 16 * 1024 * 1024;

struct CachedTexture {
    texture: SdlTexture<'static>,
    bytes: usize,
    coverage: UploadCoverage,
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

    pub(super) fn can_sample(
        &self,
        src: &Texture,
        composite: CompositeMode,
    ) -> Result<bool, RenderError> {
        self.device.check()?;
        if let Some(target) = src.backend_target()?
            && let Some(target) = target.as_any().downcast_ref::<super::target::Target<T>>()
        {
            return Ok(target
                .with_sample_texture(&self.device, composite == CompositeMode::SourceOver, |_| ())?
                .is_some());
        }
        Ok(false)
    }

    pub(super) fn with_texture<R>(
        &mut self,
        src: &Texture,
        composite: CompositeMode,
        prepared: Option<&crate::render::texture::TextureRegion<'_>>,
        draw: impl FnOnce(&mut SdlTexture<'_>, TargetAlpha) -> R,
    ) -> Result<R, RenderError> {
        self.device.check()?;
        let mut draw = Some(draw);
        if let Some(target) = src.backend_target()?
            && let Some(target) = target.as_any().downcast_ref::<super::target::Target<T>>()
            && let Some(result) = target.with_sample_texture(
                &self.device,
                composite == CompositeMode::SourceOver,
                |texture| {
                    draw.take().expect("single native draw")(texture, TargetAlpha::Premultiplied)
                },
            )?
        {
            return Ok(result);
        }
        let draw = draw.expect("unconsumed upload draw");
        let area = prepared.map(|region| region.region()).unwrap_or(
            Region::new(0, 0, u32::from(src.width), u32::from(src.height))
                .map_err(|_| RenderError::InvalidTexture)?,
        );
        if area.is_empty() {
            return Err(RenderError::InvalidTexture);
        }
        let bytes = usize::from(src.width) * usize::from(src.height) * 4
            + core::mem::size_of::<UploadCoverage>();
        let key = TextureKey::cacheable(src)
            .filter(|_| matches!(self.pool.max_size(), MaxSize::Bytes(limit) if bytes <= *limit));
        let Some(key) = key else {
            let mut texture = create(&self.device, src)?;
            upload(&self.device, &mut texture, src, area, prepared)?;
            return Ok(draw(&mut texture, TargetAlpha::Straight));
        };
        let handle = self.pool.entry(key).or_try_insert_with(|| {
            let texture = create(&self.device, src)?;
            // Only this pool retains the erased lifetime; no handle escapes this module.
            // Its creator owns the renderer and is dropped after every cached texture.
            let texture =
                unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
            Ok::<_, RenderError>(RefCell::new(CachedTexture {
                texture,
                bytes,
                coverage: UploadCoverage::default(),
            }))
        })?;
        let mut cached = handle
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?;
        if !cached.coverage.contains(area) {
            upload(&self.device, &mut cached.texture, src, area, prepared)?;
            cached.coverage.uploaded(area);
        }
        Ok(draw(&mut cached.texture, TargetAlpha::Straight))
    }
}

fn create<'a, T: RenderTarget>(
    device: &'a Device<T>,
    src: &Texture,
) -> Result<SdlTexture<'a>, RenderError> {
    let _lease = device.canvas()?;
    let texture = device
        .creator
        .create_texture_streaming(
            SdlGpuRenderer::<Box<[u8]>>::texture_format(src.format()),
            u32::from(src.width),
            u32::from(src.height),
        )
        .map_err(|_| RenderError::BackendFailure)?;
    nearest(&texture)?;
    Ok(texture)
}

fn upload<T: RenderTarget>(
    device: &Device<T>,
    target: &mut SdlTexture<'_>,
    src: &Texture,
    area: Region,
    prepared: Option<&crate::render::texture::TextureRegion<'_>>,
) -> Result<(), RenderError> {
    let resident;
    let region = match prepared {
        Some(region) => region,
        None => {
            resident = src.pixels_in(area)?.ok_or(RenderError::ResourcePending)?;
            &resident
        }
    };
    let _lease = device.canvas()?;
    for (area, pixels) in region.tiles() {
        SdlGpuRenderer::<Box<[u8]>>::upload_texture(
            target,
            &pixels,
            Some(sdl2::rect::Rect::new(
                area.x() as i32,
                area.y() as i32,
                area.width(),
                area.height(),
            )),
        )?;
    }
    Ok(())
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
