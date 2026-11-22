use super::*;
use crate::render::texture::{ColorFormat, Region};
use crate::render::{TargetAlpha, TargetDescriptor};
use crate::test_support::source_cells::{Cell, Rc, RefCell};
use crate::types::Color;
#[cfg(not(feature = "sync"))]
use alloc::rc::Weak;
#[cfg(feature = "sync")]
use std::sync::Weak;

#[derive(Default)]
struct Device {
    source: RefCell<Weak<NativeTexture>>,
    reads: Cell<usize>,
    fail: Cell<bool>,
    panic: Cell<bool>,
    invalid: Cell<bool>,
    validations: Cell<usize>,
    last_region: Cell<Option<Region>>,
}

struct Target(
    Rc<Device>,
    PlaneLayout,
    #[cfg(feature = "sync")] Option<TargetStorage>,
);

impl Target {
    fn new(device: Rc<Device>, layout: PlaneLayout) -> Self {
        Self(
            device,
            layout,
            #[cfg(feature = "sync")]
            None,
        )
    }
}

impl BackendTarget for Target {
    #[cfg(feature = "sync")]
    fn retain_storage(&mut self, storage: TargetStorage) -> Result<(), RenderError> {
        if self.2.is_some() {
            return Err(RenderError::InvalidScope);
        }
        self.2 = Some(storage);
        Ok(())
    }
    fn descriptor(&self) -> TargetDescriptor {
        TargetDescriptor {
            layout: self.1,
            alpha: TargetAlpha::Straight,
        }
    }
    fn allocated_bytes(&self) -> usize {
        self.1.required_bytes()
    }
    fn validate(&self) -> Result<(), RenderError> {
        self.0.validations.set(self.0.validations.get() + 1);
        if self.0.invalid.get() {
            return Err(RenderError::InvalidTexture);
        }
        if let Some(source) = self.0.source.borrow().upgrade() {
            assert!(source.mirror.metadata_available());
        }
        Ok(())
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn read_pixels_into(
        &self,
        region: Region,
        destination: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        self.descriptor().readback_region(region, destination)?;
        self.0.last_region.set(Some(region));
        self.0.reads.set(self.0.reads.get() + 1);
        let source = self.0.source.borrow().upgrade().unwrap();
        // Re-entry observes an in-flight read, not a held cache borrow.
        assert!(source.mirror.metadata_available());
        assert!(matches!(source.pixels(0), Err(RenderError::InvalidScope)));
        destination.bytes_mut().fill(0x66);
        if self.0.panic.replace(false) {
            panic!("readback failure");
        }
        if self.0.fail.get() {
            return Err(RenderError::BackendFailure);
        }
        Ok(())
    }
}

fn source() -> (Rc<NativeTexture>, Rc<Device>, MemoryBudget) {
    source_sized(2, 2)
}

fn source_sized(width: u16, height: u16) -> (Rc<NativeTexture>, Rc<Device>, MemoryBudget) {
    let device = Rc::new(Device::default());
    let layout = PlaneLayout::packed(width, height, ColorFormat::RGBA8888).unwrap();
    let storage_bytes = layout.required_bytes();
    let mirror_bytes = Texture::native_cpu_bytes(layout).unwrap();
    let budget = MemoryBudget::new(storage_bytes + mirror_bytes);
    let storage = budget
        .reserve(storage_bytes)
        .unwrap()
        .commit(storage_bytes)
        .unwrap();
    let readback = budget.reserve(0).unwrap();
    let target = Target::new(device.clone(), layout);
    #[cfg(feature = "sync")]
    let (storage, target) = {
        let owner = budget
            .reserve(TargetStorage::OWNER_BYTES)
            .unwrap()
            .commit(TargetStorage::OWNER_BYTES)
            .unwrap();
        let storage = TargetStorage::new(storage, readback, owner);
        let mut target = target;
        target.retain_storage(storage.clone()).unwrap();
        (storage, target)
    };
    let source = Rc::new(NativeTexture {
        target: Box::new(target),
        mirror: State::new(PixelCache::new(
            budget
                .reserve(Texture::native_pixel_bytes(layout).unwrap())
                .unwrap()
                .retain(),
            layout,
        )),
        #[cfg(feature = "sync")]
        _mirror_metadata: budget
            .reserve(Texture::native_control_bytes())
            .unwrap()
            .commit(Texture::native_control_bytes())
            .unwrap(),
        _storage: storage,
        #[cfg(not(feature = "sync"))]
        readback,
        cache_bytes: storage_bytes + mirror_bytes,
        #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
        identity: Shared::new(()),
        layout,
        workspace_bytes: 0,
        completed: true,
    });
    *device.source.borrow_mut() = Rc::downgrade(&source);
    (source, device, budget)
}

#[test]
fn regional_reads_pin_independent_storage_without_spending_full_read_reservations() {
    let (source, device, budget) = source_sized(8, 8);
    let first_area = Region::new(2, 3, 2, 2).unwrap();
    let second_area = Region::new(5, 4, 2, 2).unwrap();
    let first = source.read_pixels_in(0, first_area).unwrap();
    assert_eq!(device.last_region.get(), Some(first_area));
    assert_eq!(first.bytes().len(), 16);
    assert_eq!(budget.available(), 0);
    assert!(source.pixels(0).unwrap().is_none());
    let second = source.read_pixels_in(0, second_area).unwrap();
    assert_eq!(device.last_region.get(), Some(second_area));
    assert_ne!(first.bytes().as_ptr(), second.bytes().as_ptr());
    assert_eq!(first.bytes(), second.bytes());
    assert!(matches!(
        source.read_pixels(0),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert_eq!(device.reads.get(), 2);
    assert_eq!(budget.available(), 0);
    drop(first);
    drop(second);
    let allocations = crate::test_support::tracked_allocations(|| {
        for x in 0..7 {
            let area = Region::new(x, 1, 2, 2).unwrap();
            let view = source.read_pixels_in(0, area).unwrap();
            assert_eq!(device.last_region.get(), Some(area));
            assert_eq!(view.width(), 2);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(budget.available(), 0);
    let full = source.read_pixels(0).unwrap();
    assert_eq!(full.bytes().len(), 256);
    let reads = device.reads.get();
    let allocations = crate::test_support::tracked_allocations(|| {
        let view = source.read_pixels_in(0, first_area).unwrap();
        assert_eq!(view.bytes().as_ptr(), full.bytes()[104..].as_ptr());
        assert_eq!(view.stride(), full.stride());
    });
    assert_eq!(allocations, 0);
    assert_eq!(reads, device.reads.get());
}

#[test]
fn partial_cache_remains_readable_but_never_impersonates_a_complete_lost_source() {
    let (source, device, _) = source_sized(8, 8);
    let area = Region::new(2, 3, 2, 2).unwrap();
    drop(source.read_pixels_in(0, area).unwrap());
    device.invalid.set(true);
    assert!(source.pixels_in(0, area).unwrap().is_some());
    assert!(matches!(source.pixels(0), Err(RenderError::InvalidTexture)));
    assert!(matches!(
        source.pixels_in(1, area),
        Err(RenderError::InvalidTexture)
    ));
}

#[test]
fn empty_regions_validate_geometry_without_accessing_source_contents() {
    for completed in [true, false] {
        let (source, device, _) = source();
        let mut source = Rc::try_unwrap(source).ok().unwrap();
        source.completed = completed;
        let layout = source.layout;
        let texture = Texture::from_layout(TexBuf::Native(Box::new(source)), layout, true);
        device.invalid.set(true);
        let allocations = crate::test_support::tracked_allocations(|| {
            let region = texture
                .pixels_in(Region::new(2, 2, 0, 0).unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(region.tiles().count(), 0);
            assert!(matches!(
                texture.pixels_in(Region::new(3, 2, 0, 0).unwrap()),
                Err(RenderError::InvalidTexture)
            ));
        });
        assert_eq!(allocations, 0);
        assert_eq!(device.validations.get(), 0);
        assert_eq!(device.reads.get(), 0);
        assert!(matches!(
            texture.pixels_in(Region::new(0, 0, 1, 1).unwrap()),
            Err(RenderError::InvalidTexture)
        ));
        assert_eq!(device.validations.get(), usize::from(completed));
    }
}

#[test]
fn cached_views_pin_pixels_without_holding_the_cache_borrow() {
    let (source, device, _) = source();
    let first = source.read_pixels(0).unwrap();
    let address = first.bytes().as_ptr();
    let allocations = crate::test_support::tracked_allocations(|| {
        let second = source.read_pixels(0).unwrap();
        assert!(source.mirror.metadata_available());
        assert_eq!(second.bytes().as_ptr(), address);
        let region = second
            .into_region(Region::new(1, 1, 1, 1).unwrap())
            .unwrap();
        assert_eq!(region.bytes().as_ptr(), first.bytes()[12..].as_ptr());
        assert_eq!(region.bytes(), &[0x66; 4]);
    });
    assert_eq!(allocations, 0);
    assert_eq!(device.reads.get(), 1);
    assert_eq!(first.get_pixel(1, 1), Color::rgba(0x66, 0x66, 0x66, 0x66));
}

#[test]
fn failed_and_unwound_readback_never_publishes_partial_pixels() {
    let (source, device, budget) = source();
    device.fail.set(true);
    assert!(matches!(
        source.read_pixels(0),
        Err(RenderError::BackendFailure)
    ));
    assert!(source.pixels(0).unwrap().is_none());
    assert_eq!(
        budget.allocated(),
        32 + CachedPixels::OWNER_BYTES + Texture::native_state_bytes()
    );
    device.fail.set(false);
    device.panic.set(true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = source.read_pixels(0);
        }))
        .is_err()
    );
    assert!(source.pixels(0).unwrap().is_none());
    let allocations = crate::test_support::tracked_allocations(|| {
        assert_eq!(source.read_pixels(0).unwrap().bytes(), &[0x66; 16]);
    });
    assert_eq!(allocations, 0);
    assert_eq!(device.reads.get(), 3);
}

#[test]
fn pinned_storage_keeps_its_budget_until_the_last_owner_retires() {
    let (source, _, budget) = source();
    drop(source.read_pixels(0).unwrap());
    let retained = source
        .mirror
        .try_with(|mirror| mirror.view(0, source.full_region()))
        .unwrap()
        .unwrap()
        .unwrap();
    source
        .mirror
        .try_with(|mirror| mirror.invalidate())
        .unwrap();
    assert!(matches!(
        source.read_pixels(1),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert_eq!(retained.bytes(), [0x66; 16]);
    drop(source);
    assert_eq!(budget.allocated(), 16 + CachedPixels::OWNER_BYTES);
    drop(retained);
    assert_eq!(budget.allocated(), 0);
    assert_eq!(
        budget.available(),
        32 + CachedPixels::OWNER_BYTES + Texture::native_state_bytes()
    );
}

#[cfg(all(feature = "std", feature = "sync"))]
#[test]
fn native_preparation_registers_a_waiter_before_returning_pending() {
    use std::sync::Barrier;
    let (source, device, _) = source();
    let acquired = Barrier::new(2);
    let release = Barrier::new(2);
    let wake = crate::core::resource::wake::TaskWake::new();
    wake.take_ready();
    let mut cx = Context::from_waker(wake.waker());
    std::thread::scope(|scope| {
        let mirror = &source.mirror;
        scope.spawn(|| {
            let lease = mirror.try_take().unwrap();
            acquired.wait();
            release.wait();
            drop(lease);
        });
        acquired.wait();
        assert!(
            source
                .poll_pixels_in(0, source.full_region(), &mut cx)
                .is_pending()
        );
        assert_eq!(device.reads.get(), 0);
        release.wait();
    });
    assert!(wake.take_ready());
    let Poll::Ready(pixels) = source.poll_pixels_in(0, source.full_region(), &mut cx) else {
        panic!("released preparation must advance")
    };
    assert_eq!(pixels.unwrap().bytes(), &[0x66; 16]);
    assert_eq!(device.reads.get(), 1);
}

#[cfg(all(feature = "std", feature = "sync"))]
#[test]
fn public_region_geometry_is_checked_before_waiting_for_a_busy_mirror() {
    use std::sync::Barrier;
    let (source, device, _) = source();
    let source = Rc::try_unwrap(source).ok().unwrap();
    let layout = source.layout;
    let texture = Texture::from_layout(TexBuf::Native(Box::new(source)), layout, true);
    let TexBuf::Native(source) = &texture.buf else {
        unreachable!()
    };
    let mirror = &source.mirror;
    let acquired = Barrier::new(2);
    let release = Barrier::new(2);
    let wake = crate::core::resource::wake::TaskWake::new();
    wake.take_ready();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let lease = mirror.try_take().unwrap();
            acquired.wait();
            release.wait();
            drop(lease);
        });
        acquired.wait();
        let mut cx = Context::from_waker(wake.waker());
        assert!(matches!(
            texture.poll_pixels_in(Region::new(1, 1, 2, 2).unwrap(), &mut cx),
            Poll::Ready(Err(RenderError::InvalidTexture))
        ));
        assert!(matches!(
            texture.poll_pixels_in(Region::new(0, 0, 0, 0).unwrap(), &mut cx),
            Poll::Ready(Ok(_))
        ));
        assert!(
            texture
                .poll_pixels_in(source.full_region(), &mut cx)
                .is_pending()
        );
        assert!(texture.poll_read_pixels(&mut cx).is_pending());
        assert_eq!(device.reads.get(), 0);
        release.wait();
    });
    assert!(wake.take_ready());
}
