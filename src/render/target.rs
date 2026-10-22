//! Native output storage and source-owned pixel access.

use core::any::Any;

use super::renderer::RenderError;
use super::scratch::PlaneLayout;
use super::texture::{Region, TexturePixelsMut};
use crate::types::PhysicalRect;

#[doc(hidden)]
#[path = "target_scope.rs"]
pub mod __private;

/// Content policy for one independent target drawing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetUpdate {
    /// Initialize the entire target to zero before drawing, regardless of parent clipping or transforms. Alpha formats start transparent; opaque formats start black.
    Rebuild,
    /// Continue drawing over successfully completed contents. Newly allocated targets and targets with failed writes must be rebuilt first.
    Incremental,
}

/// Encoding of color channels relative to stored alpha.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetAlpha {
    Straight,
    Premultiplied,
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "wgpu", not(target_arch = "wasm32")),
    test
))]
impl TargetAlpha {
    pub(crate) fn convert_rgba8(self, to: Self, mut pixel: [u8; 4]) -> [u8; 4] {
        if self == to {
            return pixel;
        }
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = match to {
                Self::Premultiplied => ((u32::from(*channel) * alpha + 127) / 255) as u8,
                Self::Straight if alpha == 0 => 0,
                Self::Straight => ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8,
            };
        }
        pixel
    }
}

/// Checked pixel geometry and native color encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetDescriptor {
    pub layout: PlaneLayout,
    pub alpha: TargetAlpha,
}

impl TargetDescriptor {
    /// The complete source extent in texture pixel coordinates.
    pub fn region(self) -> Region {
        Region::new(
            0,
            0,
            u32::from(self.layout.width()),
            u32::from(self.layout.height()),
        )
        .expect("u16 plane dimensions fit a region")
    }

    /// Check a source read and its destination before any device access or writes.
    pub fn readback_region(
        self,
        region: Region,
        destination: &TexturePixelsMut<'_>,
    ) -> Result<PhysicalRect, RenderError> {
        if region.is_empty()
            || region.right() > u32::from(self.layout.width())
            || region.bottom() > u32::from(self.layout.height())
            || region.width() != u32::from(destination.width())
            || region.height() != u32::from(destination.height())
            || destination.format() != self.layout.format()
        {
            return Err(RenderError::InvalidTexture);
        }
        PhysicalRect::new(
            region.x() as u16,
            region.y() as u16,
            region.width() as u16,
            region.height() as u16,
        )
        .ok_or(RenderError::InvalidTexture)
    }
}

/// Storage requirements for an independent render target.
///
/// Storage includes native attachments such as multisample and resolve planes. Workspace is additional scratch required while binding and drawing. Readback bytes bound backend-owned staging used while reading the complete source; the destination CPU pixels are accounted for separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetPlan {
    pub descriptor: TargetDescriptor,
    pub storage_bytes: usize,
    pub workspace_bytes: usize,
    pub readback_bytes: usize,
}

/// Backend-owned storage whose source device remains available independently of a temporary renderer.
///
/// A newly allocated target has no completed contents. Its producer must initialize and submit it successfully before publishing it. Implementations retain the device resources needed for sampling, reading and destruction, without borrowing the producing renderer.
pub trait BackendTarget: Any {
    fn descriptor(&self) -> TargetDescriptor;

    /// Actual native storage allocation, including attachments and padding.
    fn allocated_bytes(&self) -> usize;

    /// Check device and resource validity without allocating, reading pixels or submitting work.
    fn validate(&self) -> Result<(), RenderError>;

    /// Inspect the concrete payload; native consumers must also check device identity and resource generation.
    fn as_any(&self) -> &dyn Any;

    /// Read completed source contents into prepared CPU storage as straight-alpha pixels.
    ///
    /// The destination must match the requested region dimensions and source format. Validate it with [`TargetDescriptor::readback_region`] before reading, then use its checked stride when writing. Complete source writes before reading and restore any device state on success or error. A failed restoration must prevent further device submissions. Do not allocate another persistent CPU copy or retain the destination borrow.
    fn read_pixels_into(
        &self,
        region: Region,
        destination: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError>;
}

#[cfg(test)]
mod tests {
    use super::TargetAlpha::{Premultiplied, Straight};

    #[test]
    fn alpha_conversion_preserves_valid_premultiplied_channels() {
        for alpha in 0..=255u8 {
            for channel in 0..=alpha {
                let pixel = [channel, channel, channel, alpha];
                let straight = Premultiplied.convert_rgba8(Straight, pixel);
                assert_eq!(Straight.convert_rgba8(Premultiplied, straight), pixel);
                assert_eq!(Straight.convert_rgba8(Straight, straight), straight);
                assert_eq!(Premultiplied.convert_rgba8(Premultiplied, pixel), pixel);
            }
        }
    }

    #[test]
    fn alpha_conversion_handles_transparent_opaque_and_saturated_channels() {
        assert_eq!(
            Premultiplied.convert_rgba8(Straight, [64, 32, 0, 128]),
            [128, 64, 0, 128]
        );
        assert_eq!(
            Premultiplied.convert_rgba8(Straight, [255, 20, 10, 0]),
            [0; 4]
        );
        assert_eq!(
            Premultiplied.convert_rgba8(Straight, [255, 20, 10, 1]),
            [255, 255, 255, 1]
        );
        assert_eq!(
            Premultiplied.convert_rgba8(Straight, [12, 34, 56, 255]),
            [12, 34, 56, 255]
        );
        assert_eq!(
            Straight.convert_rgba8(Premultiplied, [12, 34, 56, 0]),
            [0; 4]
        );
    }
}
