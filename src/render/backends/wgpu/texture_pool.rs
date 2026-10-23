//! Native source sampling and uploads keyed by source identity, revision and layout.

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::font::scalar::ScalarSurfaceKey;
use crate::render::texture::TextureKey;
use crate::render::texture::{ColorFormat, Region, Texture, UploadCoverage};
use crate::render::{RenderError, TargetAlpha};
use crate::surface::wgpu_surface::WgpuDeviceContext;
use alloc::sync::Arc;
use core::cell::Cell;

pub(super) struct BlitSource {
    pub(super) view: wgpu::TextureView,
    pub(super) alpha: TargetAlpha,
}

pub(super) fn can_sample(
    context: &Arc<WgpuDeviceContext>,
    src: &Texture,
) -> Result<bool, RenderError> {
    context.check()?;
    let native = src.backend_target()?;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(target) =
        native.and_then(|target| target.as_any().downcast_ref::<super::target::Target>())
    {
        return Ok(target.sample_view(context)?.is_some());
    }
    #[cfg(target_arch = "wasm32")]
    let _ = native;
    Ok(false)
}

pub(super) fn source_view(
    pool: &mut TexturePool,
    budget: usize,
    context: &Arc<WgpuDeviceContext>,
    src: &Texture,
    prepared: Option<&crate::render::texture::TextureRegion<'_>>,
) -> Result<BlitSource, RenderError> {
    context.check()?;
    let native = src.backend_target()?;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(target) =
        native.and_then(|target| target.as_any().downcast_ref::<super::target::Target>())
        && let Some(view) = target.sample_view(context)?
    {
        return Ok(BlitSource {
            view,
            alpha: crate::render::BackendTarget::descriptor(target).alpha,
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = native;
    let area = prepared.map(|region| region.region()).unwrap_or(
        Region::new(0, 0, u32::from(src.width), u32::from(src.height))
            .map_err(|_| RenderError::InvalidTexture)?,
    );
    if area.is_empty() {
        return Err(RenderError::InvalidTexture);
    }
    let key = TextureKey::cacheable(src).filter(|_| {
        usize::from(src.width) * usize::from(src.height) * 4
            + core::mem::size_of::<UploadCoverage>()
            <= budget
    });
    let create = || {
        Ok(CachedTexture(
            create_blit_source(&context.device, src),
            Cell::default(),
        ))
    };
    let view = match key {
        Some(key) => {
            let handle = pool
                .entry(key)
                .or_try_insert_with::<_, RenderError>(create)?;
            upload_region(context, &handle, src, area, prepared)?;
            handle
                .0
                .create_view(&wgpu::TextureViewDescriptor::default())
        }
        None => {
            let texture = create()?;
            upload_region(context, &texture, src, area, prepared)?;
            texture
                .0
                .create_view(&wgpu::TextureViewDescriptor::default())
        }
    };
    Ok(BlitSource {
        view,
        alpha: TargetAlpha::Straight,
    })
}

fn create_blit_source(device: &wgpu::Device, src: &Texture) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-blit-source"),
        size: wgpu::Extent3d {
            width: src.width as u32,
            height: src.height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn upload_region(
    context: &WgpuDeviceContext,
    target: &CachedTexture,
    src: &Texture,
    area: Region,
    prepared: Option<&crate::render::texture::TextureRegion<'_>>,
) -> Result<(), RenderError> {
    let mut coverage = target.1.get();
    if coverage.contains(area) {
        return Ok(());
    }
    let resident;
    let region = match prepared {
        Some(region) => region,
        None => {
            resident = src.pixels_in(area)?.ok_or(RenderError::ResourcePending)?;
            &resident
        }
    };
    for (area, pixels) in region.tiles() {
        let converted;
        let (bytes, stride) =
            if pixels.format() == ColorFormat::RGBA8888 && pixels.stride() % 4 == 0 {
                (pixels.bytes(), pixels.stride())
            } else {
                converted = pixels.rgba8_upload_pixels()?;
                (converted.as_ref(), usize::from(pixels.width()) * 4)
            };
        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &target.0,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: area.x(),
                    y: area.y(),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(
                    u32::try_from(stride).map_err(|_| RenderError::InvalidTexture)?,
                ),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: area.width(),
                height: area.height(),
                depth_or_array_layers: 1,
            },
        );
    }
    // Queue writes copy source bytes before returning; no CPU lease escapes.
    coverage.uploaded(area);
    target.1.set(coverage);
    Ok(())
}

/// Cache value newtype so `HasSize` reports real GPU bytes (RGBA8 × w
/// × h) for the byte budget — `wgpu::Texture` is upstream so we can't
/// `impl HasSize` on it directly.
pub struct CachedTexture(pub wgpu::Texture, Cell<UploadCoverage>);

impl HasSize for CachedTexture {
    fn cache_size(&self) -> usize {
        let size = self.0.size();
        (size.width as usize) * (size.height as usize) * 4 + core::mem::size_of::<UploadCoverage>()
    }
}

/// 16 MiB GPU texture cache. Roughly 64 ARGB icons of 256×256 each, or
/// a couple of 1024-wide screenshots; eviction is LRU once full.
pub(super) const TEXTURE_BUDGET: usize = 16 * 1024 * 1024;

pub type TexturePool = Cache<TextureKey, CachedTexture, Lru, HashLookup<TextureKey>>;

pub fn new_pool(bytes: usize) -> TexturePool {
    Cache::builder().max_size(MaxSize::Bytes(bytes)).build()
}

pub struct CachedScalarSurface(pub wgpu::Texture);

impl HasSize for CachedScalarSurface {
    fn cache_size(&self) -> usize {
        let size = self.0.size();
        (size.width as usize) * (size.height as usize)
    }
}

pub type ScalarSurfacePool =
    Cache<ScalarSurfaceKey, CachedScalarSurface, Lru, HashLookup<ScalarSurfaceKey>>;

pub fn new_scalar_surface_pool() -> ScalarSurfacePool {
    Cache::builder()
        .max_size(MaxSize::Bytes(TEXTURE_BUDGET))
        .build()
}
