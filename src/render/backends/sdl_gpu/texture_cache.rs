use core::cell::RefCell;

use sdl2::render::{Canvas, Texture as SdlTexture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::SdlGpuRenderer;
use crate::core::cache::{CacheInspect, HasSize, LruCache, MaxSize};
use crate::render::renderer::RenderError;
use crate::render::texture::{Texture, TextureKey};

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

pub(super) struct TextureCache {
    // Cached textures are private and dropped before their owning renderer context.
    pool: LruCache<TextureKey, RefCell<CachedTexture>>,
    creator: TextureCreator<WindowContext>,
    generation: u64,
}

impl TextureCache {
    pub(super) fn new(canvas: &Canvas<Window>, generation: u64, budget: usize) -> Self {
        Self {
            pool: LruCache::builder()
                .max_size(MaxSize::Bytes(budget))
                .name("sdl_gpu/texture")
                .build(),
            creator: canvas.texture_creator(),
            generation,
        }
    }

    pub(super) fn matches(&self, canvas: &Canvas<Window>, generation: u64) -> bool {
        self.creator.raw() == canvas.raw() && self.generation == generation
    }

    pub(super) fn inspect(&self) -> &dyn CacheInspect {
        &self.pool
    }

    pub(super) fn with_texture<R>(
        &mut self,
        src: &Texture,
        draw: impl FnOnce(&mut SdlTexture<'_>) -> R,
    ) -> Result<R, RenderError> {
        if !src.valid_storage() {
            return Err(RenderError::InvalidTexture);
        }
        let bytes = usize::from(src.width) * usize::from(src.height) * 4;
        let key = TextureKey::cacheable(src)
            .filter(|_| matches!(self.pool.max_size(), MaxSize::Bytes(limit) if bytes <= *limit));
        let Some(key) = key else {
            let mut texture = upload(&self.creator, src)?;
            return Ok(draw(&mut texture));
        };
        let handle = self.pool.entry(key).or_try_insert_with(|| {
            let texture = upload(&self.creator, src)?;
            // Only this pool retains the erased lifetime; no handle escapes this module.
            // Its creator owns the renderer and is dropped after every cached texture.
            let texture =
                unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
            Ok::<_, RenderError>(RefCell::new(CachedTexture { texture, bytes }))
        })?;
        Ok(draw(&mut handle.borrow_mut().texture))
    }
}

fn upload<'a>(
    creator: &'a TextureCreator<WindowContext>,
    src: &Texture,
) -> Result<SdlTexture<'a>, RenderError> {
    let mut texture = creator
        .create_texture_streaming(
            SdlGpuRenderer::<Box<[u8]>>::texture_format(src.format),
            u32::from(src.width),
            u32::from(src.height),
        )
        .map_err(|_| RenderError::BackendFailure)?;
    if !SdlGpuRenderer::<Box<[u8]>>::upload_texture(&mut texture, src) {
        return Err(RenderError::BackendFailure);
    }
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
    Ok(texture)
}
