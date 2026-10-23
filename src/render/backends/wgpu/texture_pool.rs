//! Native source sampling and uploads keyed by source identity, revision and layout.

use crate::core::cache::{Cache, HasSize, HashLookup, Lru, MaxSize};
use crate::render::font::scalar::ScalarSurfaceKey;
use crate::render::texture::Texture;
use crate::render::texture::TextureKey;
use crate::render::{RenderError, TargetAlpha};
use crate::surface::wgpu_surface::WgpuDeviceContext;
use alloc::sync::Arc;
use wgpu::util::DeviceExt;

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
    let key = TextureKey::cacheable(src)
        .filter(|_| usize::from(src.width) * usize::from(src.height) * 4 <= budget);
    let upload = || {
        let pixels = src.pixels()?.ok_or(RenderError::ResourcePending)?;
        let rgba = pixels.rgba8_upload_pixels()?;
        Ok(CachedTexture(upload_blit_source(
            &context.device,
            &context.queue,
            src,
            &rgba,
        )))
    };
    let view = match key {
        Some(key) => pool
            .entry(key)
            .or_try_insert_with::<_, RenderError>(upload)?
            .0
            .create_view(&wgpu::TextureViewDescriptor::default()),
        None => upload()?
            .0
            .create_view(&wgpu::TextureViewDescriptor::default()),
    };
    Ok(BlitSource {
        view,
        alpha: TargetAlpha::Straight,
    })
}

fn upload_blit_source(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &Texture,
    rgba: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
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
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

/// Cache value newtype so `HasSize` reports real GPU bytes (RGBA8 × w
/// × h) for the byte budget — `wgpu::Texture` is upstream so we can't
/// `impl HasSize` on it directly.
pub struct CachedTexture(pub wgpu::Texture);

impl HasSize for CachedTexture {
    fn cache_size(&self) -> usize {
        let size = self.0.size();
        (size.width as usize) * (size.height as usize) * 4
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
