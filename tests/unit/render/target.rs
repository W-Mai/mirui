use super::TargetAlpha::{Premultiplied, Straight};

#[cfg(all(feature = "std", feature = "sync"))]
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
        crate::test_support::tracked_allocations(|| proxy.retain_storage(receipt.clone()).unwrap()),
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
