use super::{ColorFormat, Region, Texture, TexturePixels};
use crate::render::RenderError;
use crate::types::Color;
use core::task::{Context, Poll};

/// A ready, read-only source region. Sampling and iteration never perform I/O or allocate.
///
/// Region coordinates refer to the original texture. Pixel views returned by [`Self::pixels`] and [`Self::tiles`] use local coordinates and retain their backing row stride. Empty regions contain no pixel views.
///
/// ```compile_fail,E0502
/// use mirui::render::texture::{ColorFormat, Region, Texture};
/// let mut texture = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
/// let region = texture.pixels_in(Region::new(0, 0, 1, 1).unwrap()).unwrap().unwrap();
/// let _write = texture.pixels_mut().unwrap();
/// assert!(region.get_pixel(0, 0).is_some());
/// ```
pub struct TextureRegion<'a> {
    region: Region,
    format: ColorFormat,
    pixels: Option<TexturePixels<'a>>,
}

impl TextureRegion<'_> {
    pub fn region(&self) -> Region {
        self.region
    }

    pub fn format(&self) -> ColorFormat {
        self.format
    }

    /// Borrow a continuous view when available, without joining or copying pixel storage.
    pub fn pixels(&self) -> Option<TexturePixels<'_>> {
        self.pixels.as_ref().map(TexturePixels::reborrow)
    }

    pub(crate) fn borrowed_pixels(&self) -> Option<&TexturePixels<'_>> {
        self.pixels.as_ref()
    }

    /// Visit non-overlapping source rectangles covering this region and their local pixel views.
    pub fn tiles(&self) -> impl Iterator<Item = (Region, TexturePixels<'_>)> {
        self.pixels()
            .map(|pixels| (self.region, pixels))
            .into_iter()
    }

    /// Read an absolute source coordinate, returning `None` outside the prepared region.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x < self.region.x()
            || x >= self.region.right()
            || y < self.region.y()
            || y >= self.region.bottom()
        {
            return None;
        }
        Some(
            self.pixels
                .as_ref()?
                .get_pixel((x - self.region.x()) as i32, (y - self.region.y()) as i32),
        )
    }
}

impl Texture<'_> {
    pub(crate) fn direct_region(&self) -> Result<Option<TextureRegion<'_>>, RenderError> {
        if matches!(
            self.buf,
            super::TexBuf::Native(_) | super::TexBuf::Decoded(_)
        ) {
            return Ok(None);
        }
        let layout = self.storage_layout()?;
        let region = Region::new(0, 0, u32::from(layout.width()), u32::from(layout.height()))
            .map_err(|_| RenderError::InvalidTexture)?;
        Ok(Some(TextureRegion {
            region,
            format: layout.format(),
            pixels: Some(self.direct_pixels(layout)?),
        }))
    }

    /// Borrow a resident source region without allocation, readback or decoding.
    ///
    /// `Ok(None)` means the requested data is not resident. Invalid ranges are rejected, not clipped. An in-bounds empty region is immediately ready and does not access the source device.
    pub fn pixels_in(&self, region: Region) -> Result<Option<TextureRegion<'_>>, RenderError> {
        let layout = self.storage_layout()?;
        if region.right() > u32::from(layout.width())
            || region.bottom() > u32::from(layout.height())
        {
            return Err(RenderError::InvalidTexture);
        }
        let pixels = if region.is_empty() {
            None
        } else {
            let pixels = match &self.buf {
                super::TexBuf::Native(native) => native.pixels_in(self.cache_revision, region)?,
                super::TexBuf::Decoded(source) => source.pixels_in(region)?,
                _ => Some(self.direct_pixels(layout)?.into_region(region)?),
            };
            let Some(pixels) = pixels else {
                return Ok(None);
            };
            Some(pixels)
        };
        Ok(Some(TextureRegion {
            region,
            format: layout.format(),
            pixels,
        }))
    }

    /// Prepare a complete source region. Native device reads may synchronously wait for their device.
    ///
    /// Resident storage is borrowed immediately. Source preparation retains the requested pixels until the returned view is dropped. Capacity failures do not overwrite other live views.
    pub fn poll_pixels_in(
        &self,
        region: Region,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TextureRegion<'_>, RenderError>> {
        let resident = self.pixels_in(region);
        #[cfg(feature = "sync")]
        let resident = match resident {
            Err(RenderError::ResourcePending) => Ok(None),
            result => result,
        };
        if let Some(ready) = resident? {
            return Poll::Ready(Ok(ready));
        }
        if let super::TexBuf::Decoded(source) = &self.buf {
            return source.poll_pixels_in(region, cx).map(|result| {
                result.map(|pixels| TextureRegion {
                    region,
                    format: self.format(),
                    pixels: Some(pixels),
                })
            });
        }
        let super::TexBuf::Native(native) = &self.buf else {
            return Poll::Ready(Err(RenderError::InvalidTexture));
        };
        native
            .poll_pixels_in(self.cache_revision, region, cx)
            .map(|result| {
                result.map(|pixels| TextureRegion {
                    region,
                    format: self.format(),
                    pixels: Some(pixels),
                })
            })
    }

    /// Prepare a complete contiguous CPU view, sharing the source's regional readback budget.
    ///
    /// Native readback is synchronous; this method does not make driver waits asynchronous.
    pub fn poll_read_pixels(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TexturePixels<'_>, RenderError>> {
        if let super::TexBuf::Decoded(source) = &self.buf {
            return source.poll_pixels_in(
                Region::new(0, 0, u32::from(self.width), u32::from(self.height))
                    .map_err(|_| RenderError::InvalidTexture)?,
                cx,
            );
        }
        if let super::TexBuf::Native(native) = &self.buf {
            return native.poll_pixels_in(
                self.cache_revision,
                Region::new(0, 0, u32::from(self.width), u32::from(self.height))
                    .map_err(|_| RenderError::InvalidTexture)?,
                cx,
            );
        }
        Poll::Ready(self.read_pixels())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/texture/region.rs"]
mod tests;
