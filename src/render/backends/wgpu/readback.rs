use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::render::texture::{ColorFormat, Texture, TexturePixelsMut};
use crate::render::{
    PlaneLayout, PlaneRequirements, RenderError, RenderFeature, RenderResource, TargetAlpha,
};
use crate::types::PhysicalRect;

#[cfg(test)]
#[path = "readback_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct Readback {
    staging: Option<Staging>,
}

struct Staging {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    mapped: Arc<AtomicBool>,
}

struct Mapping<'a>(&'a wgpu::Buffer);

impl Drop for Mapping<'_> {
    fn drop(&mut self) {
        self.0.unmap();
    }
}

fn is_bgra(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

fn layout(texture: &wgpu::Texture, rect: PhysicalRect) -> Result<PlaneLayout, RenderError> {
    if texture.dimension() != wgpu::TextureDimension::D2
        || texture.sample_count() != 1
        || !texture.usage().contains(wgpu::TextureUsages::COPY_SRC)
    {
        return Err(RenderError::InvalidTexture);
    }
    if !matches!(
        texture.format(),
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        return Err(RenderError::Unsupported(RenderFeature::Readback));
    }
    if rect.is_empty()
        || u32::from(rect.right()) > texture.width()
        || u32::from(rect.bottom()) > texture.height()
    {
        return Err(RenderError::InvalidGeometry);
    }
    PlaneLayout::aligned(
        rect.width(),
        rect.height(),
        ColorFormat::RGBA8888,
        PlaneRequirements::new(1, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
            .map_err(|_| RenderError::InvalidTexture)?,
    )
    .map_err(|_| RenderError::InvalidTexture)
}

impl Staging {
    fn map(
        &self,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        rect: PhysicalRect,
        aligned: PlaneLayout,
    ) -> Result<Mapping<'_>, RenderError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mirui-wgpu-readback-encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: u32::from(rect.x()),
                    y: u32::from(rect.y()),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned.stride_bytes() as u32),
                    rows_per_image: Some(u32::from(rect.height())),
                },
            },
            wgpu::Extent3d {
                width: u32::from(rect.width()),
                height: u32::from(rect.height()),
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit(Some(encoder.finish()));
        let slice = self.buffer.slice(..aligned.required_bytes() as u64);
        self.mapped.store(false, Ordering::Release);
        let mapped = Arc::clone(&self.mapped);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            mapped.store(result.is_ok(), Ordering::Release);
        });
        let polled = self.device.poll(wgpu::PollType::Wait {
            timeout: None,
            submission_index: Some(submission),
        });
        if polled.is_err() || !self.mapped.load(Ordering::Acquire) {
            // Failed mappings never return their buffer or callback state to the cache.
            self.buffer.destroy();
            return Err(RenderError::BackendFailure);
        }
        Ok(Mapping(&self.buffer))
    }
}

impl Readback {
    pub(super) fn read(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        rect: PhysicalRect,
    ) -> Result<Texture<'static>, RenderError> {
        let aligned = layout(texture, rect)?;
        Self::check_size(device, aligned)?;
        let mut result = Texture::owned(rect.width(), rect.height(), ColorFormat::RGBA8888)?;
        self.read_into(device, queue, texture, rect, &mut result.pixels_mut()?)?;
        Ok(result)
    }

    fn check_size(device: &wgpu::Device, layout: PlaneLayout) -> Result<(), RenderError> {
        if layout.required_bytes() as u64 > device.limits().max_buffer_size {
            return Err(RenderError::ResourceLimit(RenderResource::Target));
        }
        Ok(())
    }

    pub(super) fn read_into(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        rect: PhysicalRect,
        destination: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        let aligned = layout(texture, rect)?;
        Self::check_size(device, aligned)?;
        if destination.width() != rect.width()
            || destination.height() != rect.height()
            || destination.format() != ColorFormat::RGBA8888
        {
            return Err(RenderError::InvalidTexture);
        }
        let row_bytes = PlaneLayout::packed(rect.width(), rect.height(), ColorFormat::RGBA8888)
            .map_err(|_| RenderError::InvalidTexture)?
            .stride_bytes();
        let size = aligned.required_bytes() as u64;
        let staging = match self.staging.take() {
            Some(staging) if staging.device == *device && staging.buffer.size() >= size => staging,
            _ => Staging {
                device: device.clone(),
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("mirui-wgpu-readback"),
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                mapped: Arc::new(AtomicBool::new(false)),
            },
        };
        let mapping = staging.map(queue, texture, rect, aligned)?;
        let slice = staging.buffer.slice(..size);
        let data = slice.get_mapped_range();
        let swap_rb = is_bgra(texture.format());
        let dst_stride = destination.stride();
        let dst = destination.bytes_mut();
        for row in 0..usize::from(rect.height()) {
            let start = row * aligned.stride_bytes();
            let dst_start = row * dst_stride;
            for (source, target) in data[start..start + row_bytes]
                .chunks_exact(4)
                .zip(dst[dst_start..dst_start + row_bytes].chunks_exact_mut(4))
            {
                let mut pixel = [source[0], source[1], source[2], source[3]];
                if swap_rb {
                    pixel.swap(0, 2);
                }
                target.copy_from_slice(
                    &TargetAlpha::Premultiplied.convert_rgba8(TargetAlpha::Straight, pixel),
                );
            }
        }
        drop(data);
        drop(mapping);
        self.staging = Some(staging);
        Ok(())
    }
}
