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
                    .with_lock(None, |pixels, pitch| plan.write_pixels(pixels, pitch))
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
    crate::trace_span!("sdl.texture.create");
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
    fn cropped_indexed_layers_keep_bit_offsets_palette_alpha_and_cached_uploads() {
        use mirx::{image::ColorFormat::I2, palette::ColorTableView};
        use sdl2::{pixels::PixelFormatEnum, render::BlendMode};

        let canvas = sdl2::surface::Surface::new(3, 2, PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        let creator = canvas.texture_creator();
        let device = Rc::new(Device::new(canvas, creator).unwrap());
        let mut cache = TextureCache::new(device.clone(), 4096);
        let bytes = [0b00_01_10_11, 0b01_00_00_00, 0b11_10_01_00, 0b10_00_00_00];
        let a = [
            [1, 2, 3, 0],
            [5, 6, 7, 255],
            [9, 10, 11, 128],
            [13, 14, 15, 255],
        ];
        let mut b = a;
        b[1] = [47, 53, 59, 42];
        let mut read = |x, palette: &[[u8; 4]; 4]| {
            let table = ColorTableView::from_rgba_bytes(palette.as_flattened()).unwrap();
            let atlas = Texture::from_indexed(&bytes, 5, 2, I2, table).unwrap();
            let slice = atlas
                .pixels_in(Region::new(x, 0, 3, 2).unwrap())
                .unwrap()
                .unwrap();
            let pixels = slice.pixels().unwrap();
            let texture = pixels.as_texture().with_cache_revision(19);
            cache
                .with_texture(&texture, CompositeMode::SourceOver, None, |source, _| {
                    source.set_blend_mode(BlendMode::None);
                    let mut canvas = device.canvas().unwrap();
                    canvas.copy(source, None, None).unwrap();
                    canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap()
                })
                .unwrap()
        };
        assert_eq!(
            read(1, &a),
            [a[1], a[2], a[3], a[2], a[1], a[0]].as_flattened()
        );
        assert_eq!(
            read(1, &a),
            [a[1], a[2], a[3], a[2], a[1], a[0]].as_flattened()
        );
        assert_eq!(
            read(0, &a),
            [a[0], a[1], a[2], a[3], a[2], a[1]].as_flattened()
        );
        assert_eq!(
            read(1, &b),
            [b[1], b[2], b[3], b[2], b[1], b[0]].as_flattened()
        );
        assert_eq!(cache.pool.stats().hit_count, 1);
    }

    #[test]
    fn strided_sprite_layers_reuse_completed_uploads_and_distinguish_revisions() {
        use crate::render::texture::{AlphaMode, ColorFormat};
        use sdl2::pixels::PixelFormatEnum;
        let canvas = sdl2::surface::Surface::new(1, 2, PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        let creator = canvas.texture_creator();
        let device = Rc::new(Device::new(canvas, creator).unwrap());
        let mut cache = TextureCache::new(device.clone(), 4096);
        let mut bytes = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255];
        let mut read = |bytes: &[u8; 16], x, revision| {
            let parent = Texture::from_ref(bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
            let slice = parent
                .pixels_in(Region::new(x, 0, 1, 2).unwrap())
                .unwrap()
                .unwrap();
            let pixels = slice.pixels().unwrap();
            let mut texture = pixels.as_texture().with_cache_revision(revision);
            texture.alpha_mode = AlphaMode::Blend;
            cache
                .with_texture(&texture, CompositeMode::SourceOver, None, |source, _| {
                    let mut canvas = device.canvas().unwrap();
                    canvas.copy(source, None, None).unwrap();
                    canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap()
                })
                .unwrap()
        };
        assert_eq!(read(&bytes, 1, 3), [4, 5, 6, 255, 10, 11, 12, 255]);
        assert_eq!(read(&bytes, 0, 1), [1, 2, 3, 255, 7, 8, 9, 255]);
        bytes[0] = 122;
        // Deliberately retain the old test revision. A second upload would
        // expose the changed byte, while a cache hit preserves the original.
        assert_eq!(read(&bytes, 0, 1)[0], 1);
        assert_eq!(read(&bytes, 0, 2)[0], 122);
        assert_eq!(cache.pool.stats().hit_count, 1);
    }

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
