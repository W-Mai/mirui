//! Native output storage and source-owned pixel access.

use core::any::Any;

use super::renderer::RenderError;
use super::scratch::PlaneLayout;
use super::texture::{Region, TexturePixelsMut};
use crate::types::PhysicalRect;

#[doc(hidden)]
#[path = "target_scope.rs"]
pub mod __private;

#[cfg(all(feature = "std", feature = "sync-cache"))]
#[path = "target_owner.rs"]
mod thread_owner;
#[cfg(all(feature = "std", feature = "sync-cache"))]
pub use thread_owner::{ThreadOwner, ThreadResource};

/// Retains the storage charge and readback allowance until native resources are released.
///
/// Backend targets receive this receipt during creation. Store it beside the actual resources, including when their destruction is deferred beyond the transferable target value. Cloning a receipt retains the same charges without reserving additional capacity.
#[cfg(feature = "sync-cache")]
#[derive(Clone)]
pub struct TargetStorage(crate::core::threading::Shared<StorageCharge>);

#[cfg(feature = "sync-cache")]
struct StorageCharge {
    #[cfg(feature = "std")]
    allocation: crate::core::threading::Gate<super::budget::Allocation>,
    #[cfg(not(feature = "std"))]
    _allocation: super::budget::Allocation,
    readback: super::budget::Reservation,
    _owner: super::budget::Allocation,
}

#[cfg(feature = "sync-cache")]
impl TargetStorage {
    pub(crate) const OWNER_BYTES: usize =
        core::mem::size_of::<StorageCharge>() + 2 * core::mem::size_of::<usize>();

    pub(crate) fn new(
        allocation: super::budget::Allocation,
        readback: super::budget::Reservation,
        owner: super::budget::Allocation,
    ) -> Self {
        Self(crate::core::threading::Shared::new(StorageCharge {
            #[cfg(feature = "std")]
            allocation: crate::core::threading::Gate::new(allocation),
            #[cfg(not(feature = "std"))]
            _allocation: allocation,
            readback,
            _owner: owner,
        }))
    }

    pub(crate) fn readback_bytes(&self) -> usize {
        self.0.readback.bytes()
    }

    #[cfg(all(feature = "std", feature = "sync-cache"))]
    pub(crate) fn split_control(
        &self,
        bytes: usize,
    ) -> Result<super::budget::Allocation, RenderError> {
        self.0
            .allocation
            .with(|allocation| allocation.split(bytes))
            .map_err(|_| RenderError::InvalidScope)?
            .ok_or(RenderError::ResourceLimit(super::RenderResource::Target))
    }
}

#[cfg(feature = "sync-cache")]
impl core::fmt::Debug for TargetStorage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Formatting never borrows the mutable accounting receipt.
        f.debug_struct("TargetStorage")
            .field("readback_bytes", &self.readback_bytes())
            .finish_non_exhaustive()
    }
}

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

    /// Attach the existing storage charge to its actual native owner before publication.
    ///
    /// Retain the receipt through resource and staging destruction. Wrappers must forward it to the owner rather than retaining it only in a temporary proxy. Repeated attachment must return an error without replacing the first receipt.
    #[cfg(feature = "sync-cache")]
    fn retain_storage(&mut self, storage: TargetStorage) -> Result<(), RenderError>;

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

    #[cfg(all(feature = "std", feature = "sync-cache"))]
    #[test]
    fn retained_storage_keeps_native_readback_and_receipt_charges_until_actual_destruction() {
        use super::*;
        use crate::render::budget::MemoryBudget;
        use core::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, OnceLock};

        struct Raw {
            budget: MemoryBudget,
            drops: Arc<AtomicUsize>,
        }
        impl Drop for Raw {
            fn drop(&mut self) {
                assert_eq!(self.budget.allocated(), 32 + TargetStorage::OWNER_BYTES);
                assert_eq!(self.budget.available(), 0);
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
        struct Storage {
            _raw: Raw,
            receipt: OnceLock<TargetStorage>,
        }
        struct Proxy(Arc<Storage>);
        impl BackendTarget for Proxy {
            fn descriptor(&self) -> TargetDescriptor {
                TargetDescriptor {
                    layout: PlaneLayout::packed(4, 2, super::super::texture::ColorFormat::RGBA8888)
                        .unwrap(),
                    alpha: Straight,
                }
            }
            fn allocated_bytes(&self) -> usize {
                32
            }
            fn validate(&self) -> Result<(), RenderError> {
                Ok(())
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn retain_storage(&mut self, receipt: TargetStorage) -> Result<(), RenderError> {
                self.0
                    .receipt
                    .set(receipt)
                    .map_err(|_| RenderError::InvalidScope)
            }
            fn read_pixels_into(
                &self,
                _: Region,
                _: &mut TexturePixelsMut<'_>,
            ) -> Result<(), RenderError> {
                Err(RenderError::Unsupported(
                    super::super::RenderFeature::Readback,
                ))
            }
        }
        let budget = MemoryBudget::new(32 + 96 + TargetStorage::OWNER_BYTES);
        let charge = budget.reserve(32).unwrap().commit(32).unwrap();
        let readback = budget.reserve(96).unwrap();
        let owner = budget
            .reserve(TargetStorage::OWNER_BYTES)
            .unwrap()
            .commit(TargetStorage::OWNER_BYTES)
            .unwrap();
        let receipt = TargetStorage::new(charge, readback, owner);
        let drops = Arc::new(AtomicUsize::new(0));
        let retained = Arc::new(Storage {
            _raw: Raw {
                budget: budget.clone(),
                drops: drops.clone(),
            },
            receipt: OnceLock::new(),
        });
        let mut proxy = Proxy(retained.clone());
        assert_eq!(
            crate::test_support::tracked_allocations(|| proxy
                .retain_storage(receipt.clone())
                .unwrap()),
            0
        );
        assert_eq!(
            proxy.retain_storage(receipt.clone()),
            Err(RenderError::InvalidScope)
        );
        drop(receipt);
        std::thread::spawn(move || drop(proxy)).join().unwrap();
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert_eq!(budget.available(), 0);
        drop(retained);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(budget.allocated(), 0);
        assert_eq!(budget.available(), 32 + 96 + TargetStorage::OWNER_BYTES);
    }

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
