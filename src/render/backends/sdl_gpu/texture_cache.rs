use alloc::rc::Rc;
use core::cell::RefCell;

use sdl2::render::{RenderTarget, Texture as SdlTexture};
use sdl2::video::Window;

use super::SdlGpuRenderer;
use super::device::Device;
use super::gradient::radial::{Key as RadialKey, Radial};
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

impl CachedTexture {
    fn new(texture: SdlTexture<'_>, bytes: usize) -> Self {
        // Only this pool retains the erased lifetime. Its device outlives all entries.
        let texture =
            unsafe { core::mem::transmute::<SdlTexture<'_>, SdlTexture<'static>>(texture) };
        Self {
            texture,
            bytes,
            coverage: UploadCoverage::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CacheKey {
    Image(TextureKey),
    Radial(RadialKey),
}

impl HasSize for CachedTexture {
    fn cache_size(&self) -> usize {
        self.bytes
    }
}

pub(super) struct TextureCache<T: RenderTarget = Window> {
    // Cached textures are private and dropped before their owning renderer context.
    pool: LruCache<CacheKey, RefCell<CachedTexture>>,
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

    pub(super) fn radial_fits(&self, key: &RadialKey) -> bool {
        let bytes =
            key.bytes() + core::mem::size_of::<CacheKey>() + core::mem::size_of::<UploadCoverage>();
        (self.device.max_texture_size.0 == 0
            || u32::from(key.size) <= self.device.max_texture_size.0)
            && (self.device.max_texture_size.1 == 0
                || u32::from(key.size) <= self.device.max_texture_size.1)
            && matches!(self.pool.max_size(),MaxSize::Bytes(limit) if bytes<=*limit)
    }

    pub(super) fn with_radial<R>(
        &mut self,
        plan: &Radial<'_>,
        draw: impl FnOnce(&mut SdlTexture<'_>) -> R,
    ) -> Result<R, RenderError> {
        self.device.check()?;
        if !self.radial_fits(&plan.key) {
            return Err(RenderError::MissingWorkspace);
        }
        let bytes = plan.key.bytes()
            + core::mem::size_of::<CacheKey>()
            + core::mem::size_of::<UploadCoverage>();
        let handle = self
            .pool
            .entry(CacheKey::Radial(plan.key.clone()))
            .or_try_insert_with(|| {
                let _lease = self.device.canvas()?;
                let size = usize::from(plan.key.size);
                let mut texture = self
                    .device
                    .creator
                    .create_texture_streaming(
                        sdl2::pixels::PixelFormatEnum::RGBA32,
                        size as u32,
                        size as u32,
                    )
                    .map_err(|_| RenderError::BackendFailure)?;
                texture
                    .with_lock(None, |pixels, pitch| {
                        if pitch < size * 4 || pixels.len() < pitch * (size - 1) + size * 4 {
                            return Err(RenderError::InvalidTexture);
                        }
                        for y in 0..size {
                            for x in 0..size {
                                let color = plan.texel(x, y);
                                let offset = y * pitch + x * 4;
                                pixels[offset..offset + 4]
                                    .copy_from_slice(&[color.r, color.g, color.b, color.a]);
                            }
                        }
                        Ok(())
                    })
                    .map_err(|_| RenderError::BackendFailure)??;
                // Filtering is local to this generated palette; ordinary images stay nearest sampled.
                if unsafe {
                    sdl2_sys::SDL_SetTextureScaleMode(
                        texture.raw(),
                        sdl2_sys::SDL_ScaleMode::SDL_ScaleModeLinear,
                    )
                } != 0
                {
                    return Err(RenderError::BackendFailure);
                }
                texture.set_blend_mode(sdl2::render::BlendMode::Blend);
                Ok::<_, RenderError>(RefCell::new(CachedTexture::new(texture, bytes)))
            })?;
        let mut cached = handle
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?;
        Ok(draw(&mut cached.texture))
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
            + core::mem::size_of::<UploadCoverage>()
            + core::mem::size_of::<CacheKey>();
        let key = TextureKey::cacheable(src)
            .map(CacheKey::Image)
            .filter(|_| matches!(self.pool.max_size(), MaxSize::Bytes(limit) if bytes <= *limit));
        let Some(key) = key else {
            let mut texture = create(&self.device, src)?;
            upload(&self.device, &mut texture, src, area, prepared)?;
            return Ok(draw(&mut texture, TargetAlpha::Straight));
        };
        let handle = self.pool.entry(key).or_try_insert_with(|| {
            let texture = create(&self.device, src)?;
            Ok::<_, RenderError>(RefCell::new(CachedTexture::new(texture, bytes)))
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

#[cfg(test)]
mod tests {
    use super::super::gradient::radial::tests::with_plan;
    use super::*;
    use crate::types::Color;

    #[test]
    fn radial_entries_share_image_budget_own_palette_values_and_reuse_after_source_drop() {
        let canvas = sdl2::surface::Surface::new(16, 16, sdl2::pixels::PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        let creator = canvas.texture_creator();
        let device = Rc::new(Device::new(canvas, creator).unwrap());
        let color = Color::rgba(0, 0, 0, 0);
        let bytes = with_plan(color, |plan| {
            plan.key.bytes()
                + core::mem::size_of::<CacheKey>()
                + core::mem::size_of::<UploadCoverage>()
        });
        let mut cache = TextureCache::new(device.clone(), bytes);
        with_plan(color, |plan| cache.with_radial(plan, |_| ()).unwrap());
        assert_eq!(cache.pool.len(), 1);
        with_plan(color, |plan| {
            assert_eq!(
                crate::test_support::tracked_allocations(|| cache
                    .with_radial(plan, |_| ())
                    .unwrap()),
                0
            );
        });
        assert_eq!(cache.pool.stats().hit_count, 1);
        let old = with_plan(color, |plan| plan.key.clone());
        with_plan(Color::rgba(9, 8, 7, 0), |plan| {
            assert_ne!(old, plan.key);
            cache.with_radial(plan, |_| ()).unwrap();
        });
        assert_eq!(cache.pool.len(), 1);
        assert_eq!(cache.pool.stats().evict_count, 1);
        let short = TextureCache::new(device.clone(), bytes - 1);
        with_plan(color, |plan| assert!(!short.radial_fits(&plan.key)));
        let pixels = [7_u8; 4 * 4 * 4];
        let texture =
            Texture::from_ref(&pixels, 4, 4, crate::render::texture::ColorFormat::RGBA8888)
                .unwrap()
                .with_transient(false);
        cache
            .with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ())
            .unwrap();
        assert_eq!(cache.pool.len(), 1);
        assert_eq!(cache.pool.stats().evict_count, 2);
        assert!(cache.matches(&device));
        device.invalidate();
        assert!(!cache.matches(&device));
        std::println!(
            "gradient_key_bytes={} image_key_bytes={} batch_bytes={}",
            core::mem::size_of::<CacheKey>(),
            core::mem::size_of::<TextureKey>(),
            core::mem::size_of::<sdl2_sys::SDL_Vertex>() * super::super::gradient::BATCH_VERTICES
        );
    }
}
