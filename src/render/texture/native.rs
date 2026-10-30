#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
use crate::core::threading::Shared;
use crate::core::threading::State;
use alloc::boxed::Box;
use core::task::{Context, Poll, ready};

use super::access::CachedPixels;
use super::pixel_cache::PixelCache;
use super::{AlphaMode, Region, TexBuf, Texture, TexturePixels, TexturePixelsMut, state_error};
use crate::core::cache::HasSize;
use crate::render::budget::{Allocation, MemoryBudget, Reservation};
#[cfg(feature = "sync")]
use crate::render::target::TargetStorage;

#[cfg(feature = "sync")]
type StorageReceipt = TargetStorage;
#[cfg(not(feature = "sync"))]
type StorageReceipt = Allocation;
use crate::render::{
    BackendTarget, PlaneLayout, RenderError, RenderResource, Renderer, TargetPlan, TargetUpdate,
};
use crate::types::Viewport;

pub(super) struct NativeTexture {
    target: Box<dyn BackendTarget>,
    mirror: State<PixelCache>,
    #[cfg(feature = "sync")]
    _mirror_metadata: Allocation,
    _storage: StorageReceipt,
    #[cfg(not(feature = "sync"))]
    readback: Reservation,
    cache_bytes: usize,
    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    identity: Shared<()>,
    layout: PlaneLayout,
    workspace_bytes: usize,
    completed: bool,
}

fn capacity_error() -> RenderError {
    RenderError::ResourceLimit(RenderResource::Texture)
}

impl NativeTexture {
    fn new(
        renderer: &mut dyn Renderer,
        plan: TargetPlan,
        budget: &MemoryBudget,
    ) -> Result<Self, RenderError> {
        let layout = plan.descriptor.layout;
        let storage = budget
            .reserve(plan.storage_bytes)
            .ok_or_else(capacity_error)?;
        let mirror = budget
            .reserve(Texture::native_pixel_bytes(layout).ok_or_else(capacity_error)?)
            .ok_or_else(capacity_error)?;
        #[cfg(feature = "sync")]
        let metadata = budget
            .reserve(Texture::native_control_bytes())
            .ok_or_else(capacity_error)?
            .commit(Texture::native_control_bytes())
            .map_err(|_| capacity_error())?;
        #[cfg(feature = "sync")]
        let owner = budget
            .reserve(TargetStorage::OWNER_BYTES)
            .ok_or_else(capacity_error)?
            .commit(TargetStorage::OWNER_BYTES)
            .map_err(|_| capacity_error())?;
        let readback = budget
            .reserve(plan.readback_bytes)
            .ok_or_else(capacity_error)?;
        let target = renderer.create_target(&plan)?;
        if target.descriptor() != plan.descriptor || !renderer.can_reuse_target(&*target, &plan)? {
            return Err(RenderError::InvalidTexture);
        }
        target.validate()?;
        let storage = match storage.commit(target.allocated_bytes()) {
            Ok(storage) => storage,
            Err(reservation) => {
                drop(target);
                drop(reservation);
                return Err(capacity_error());
            }
        };
        let cache_bytes = storage.bytes() + mirror.bytes() + readback.bytes();
        #[cfg(feature = "sync")]
        let cache_bytes = cache_bytes + metadata.bytes() + owner.bytes();
        #[cfg(feature = "sync")]
        let (target, storage) = {
            let storage = TargetStorage::new(storage, readback, owner);
            let mut target = target;
            target.retain_storage(storage.clone())?;
            (target, storage)
        };
        Ok(Self {
            target,
            mirror: State::new(PixelCache::new(mirror.retain(), layout)),
            #[cfg(feature = "sync")]
            _mirror_metadata: metadata,
            _storage: storage,
            #[cfg(not(feature = "sync"))]
            readback,
            cache_bytes,
            #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
            identity: Shared::new(()),
            layout,
            workspace_bytes: plan.workspace_bytes,
            completed: false,
        })
    }

    pub(super) fn layout(&self) -> PlaneLayout {
        self.layout
    }

    fn full_region(&self) -> Region {
        Region::new(
            0,
            0,
            u32::from(self.layout.width()),
            u32::from(self.layout.height()),
        )
        .expect("u16 plane dimensions fit a region")
    }

    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    pub(super) fn identity(&self) -> Shared<()> {
        Shared::clone(&self.identity)
    }

    pub(super) fn validate(&self, revision: u64) -> Result<(), RenderError> {
        if !self.completed {
            return Err(RenderError::InvalidTexture);
        }
        if self
            .mirror
            .try_with(|mirror| mirror.contains(revision, self.full_region()))
            .map_err(state_error)?
        {
            return Ok(());
        }
        self.target.validate()
    }

    pub(super) fn pixels(&self, revision: u64) -> Result<Option<TexturePixels<'_>>, RenderError> {
        self.pixels_in(revision, self.full_region())
    }

    pub(super) fn pixels_in(
        &self,
        revision: u64,
        region: Region,
    ) -> Result<Option<TexturePixels<'_>>, RenderError> {
        if !self.completed {
            return Err(RenderError::InvalidTexture);
        }
        if let Some(pixels) = self
            .mirror
            .try_with(|mirror| mirror.view(revision, region))
            .map_err(state_error)??
        {
            return Ok(Some(pixels));
        }
        self.target.validate()?;
        Ok(None)
    }

    pub(super) fn read_pixels(&self, revision: u64) -> Result<TexturePixels<'_>, RenderError> {
        self.read_pixels_in(revision, self.full_region())
    }

    pub(super) fn read_pixels_in(
        &self,
        revision: u64,
        region: Region,
    ) -> Result<TexturePixels<'_>, RenderError> {
        if let Some(pixels) = self.pixels_in(revision, region)? {
            return Ok(pixels);
        }
        let mut mirror = self.mirror.try_take().map_err(state_error)?;
        self.prepare_pixels(&mut mirror, revision, region)
    }

    pub(super) fn poll_pixels_in(
        &self,
        revision: u64,
        region: Region,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TexturePixels<'_>, RenderError>> {
        match self.pixels_in(revision, region) {
            Ok(Some(pixels)) => return Poll::Ready(Ok(pixels)),
            Ok(None) => {}
            #[cfg(feature = "sync")]
            Err(RenderError::ResourcePending) => {}
            Err(error) => return Poll::Ready(Err(error)),
        }
        let mut mirror = ready!(self.mirror.poll_take(cx)).map_err(state_error)?;
        Poll::Ready(self.prepare_pixels(&mut mirror, revision, region))
    }

    fn prepare_pixels(
        &self,
        mirror: &mut PixelCache,
        revision: u64,
        region: Region,
    ) -> Result<TexturePixels<'static>, RenderError> {
        if let Some(pixels) = mirror.view(revision, region)? {
            return Ok(pixels);
        }
        self.target.validate()?;
        let layout = if region == self.full_region() {
            self.layout
        } else {
            PlaneLayout::packed(
                region.width() as u16,
                region.height() as u16,
                self.layout.format(),
            )
            .map_err(|_| RenderError::InvalidTexture)?
        };
        mirror.prepare(layout)?;
        let bytes = mirror.bytes_mut()?;
        let mut destination = TexturePixelsMut::new(bytes, layout, AlphaMode::Blend);
        self.target.read_pixels_into(region, &mut destination)?;
        mirror.publish(revision, region)?;
        mirror
            .view(revision, region)?
            .ok_or(RenderError::InvalidTexture)
    }
}

impl HasSize for NativeTexture {
    fn cache_size(&self) -> usize {
        self.cache_bytes
    }
}

impl Texture<'static> {
    pub(crate) fn native(
        renderer: &mut dyn Renderer,
        plan: TargetPlan,
        budget: &MemoryBudget,
    ) -> Result<Self, RenderError> {
        let native = NativeTexture::new(renderer, plan, budget)?;
        Ok(Self::from_layout(
            TexBuf::Native(Box::new(native)),
            plan.descriptor.layout,
            true,
        ))
    }
}

impl Texture<'_> {
    pub(crate) fn native_cpu_bytes(layout: PlaneLayout) -> Option<usize> {
        Self::native_pixel_bytes(layout)?.checked_add(Self::native_state_bytes())
    }

    fn native_pixel_bytes(layout: PlaneLayout) -> Option<usize> {
        layout
            .required_bytes()
            .checked_add(CachedPixels::OWNER_BYTES)
    }

    pub(crate) const fn native_state_bytes() -> usize {
        #[cfg(feature = "sync")]
        {
            Self::native_control_bytes() + TargetStorage::OWNER_BYTES
        }
        #[cfg(not(feature = "sync"))]
        {
            State::<PixelCache>::EXTRA_BYTES
        }
    }

    #[cfg(feature = "sync")]
    const fn native_control_bytes() -> usize {
        let previous = core::mem::size_of::<Allocation>() + core::mem::size_of::<Reservation>();
        let current = core::mem::size_of::<Allocation>() + core::mem::size_of::<StorageReceipt>();
        State::<PixelCache>::EXTRA_BYTES + current - previous
    }
    /// Inspect completed native storage without reading pixels, allocating or submitting work.
    ///
    /// CPU textures and unavailable native resources return `None`; routed preparation checks the requested CPU region before submission. Incomplete outputs and access conflicts are rejected. Native consumers must check the concrete payload, destination device identity and active-target feedback before sampling. This borrow does not permit target writes.
    pub fn backend_target(&self) -> Result<Option<&dyn BackendTarget>, RenderError> {
        let TexBuf::Native(native) = &self.buf else {
            self.pixel_layout()?;
            return Ok(None);
        };
        if !native.completed {
            return Err(RenderError::InvalidTexture);
        }
        match native.target.validate() {
            Ok(()) => Ok(Some(&*native.target)),
            Err(RenderError::InvalidTexture | RenderError::BackendFailure) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn set_output_revision(&mut self, revision: u64) {
        if let TexBuf::Native(native) = &mut self.buf {
            let mirror = native.mirror.get_mut();
            if native.completed {
                mirror.retag(self.cache_revision, revision);
            }
        }
        self.cache_revision = revision;
        self.transient = false;
    }

    pub(crate) fn invalidate_native(&mut self) {
        if let TexBuf::Native(native) = &mut self.buf {
            native.completed = false;
            native.mirror.get_mut().invalidate();
        }
    }
    pub(crate) fn is_native(&self) -> bool {
        matches!(self.buf, TexBuf::Native(_))
    }

    pub(crate) fn can_reuse_native(
        &self,
        renderer: &dyn Renderer,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        let TexBuf::Native(native) = &self.buf else {
            return Ok(false);
        };
        #[cfg(feature = "sync")]
        let readback_bytes = native._storage.readback_bytes();
        #[cfg(not(feature = "sync"))]
        let readback_bytes = native.readback.bytes();
        Ok(renderer.can_reuse_target(&*native.target, plan)?
            && native.workspace_bytes == plan.workspace_bytes
            && readback_bytes == plan.readback_bytes)
    }

    pub(crate) fn target_workspace_bytes(&self) -> usize {
        match &self.buf {
            TexBuf::Native(native) => native.workspace_bytes,
            _ => 0,
        }
    }

    pub(crate) fn render_native(
        &mut self,
        renderer: &mut dyn Renderer,
        viewport: Viewport,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let TexBuf::Native(native) = &mut self.buf else {
            return Err(RenderError::InvalidTexture);
        };
        if workspace.len() < native.workspace_bytes {
            return Err(RenderError::MissingWorkspace);
        }
        native.completed = false;
        native.mirror.get_mut().invalidate();
        renderer.with_target(
            &mut *native.target,
            viewport,
            TargetUpdate::Rebuild,
            workspace,
            body,
        )?;
        native.completed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
}
