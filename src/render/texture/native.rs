#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
use alloc::rc::Rc;
use alloc::{boxed::Box, vec::Vec};
use core::cell::{Cell, RefCell};

use super::access::CachedPixels;
use super::{AlphaMode, TexBuf, Texture, TexturePixels, TexturePixelsMut};
use crate::core::cache::handle::{CacheEntry, EntryRc};
use crate::core::cache::{Handle, HasSize};
use crate::render::budget::{Allocation, MemoryBudget, Reservation};
use crate::render::{
    BackendTarget, PlaneLayout, RenderError, RenderResource, Renderer, TargetPlan, TargetUpdate,
};
use crate::types::Viewport;

pub(super) struct NativeTexture {
    target: Box<dyn BackendTarget>,
    mirror: RefCell<Option<Mirror>>,
    storage: Allocation,
    readback: Reservation,
    cache_bytes: Cell<usize>,
    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    identity: Rc<()>,
    layout: PlaneLayout,
    workspace_bytes: usize,
    completed: bool,
}

struct Mirror {
    pixels: Option<Handle<CachedPixels>>,
    reservation: Option<Reservation>,
    revision: Option<u64>,
}

fn capacity_error() -> RenderError {
    RenderError::ResourceLimit(RenderResource::Texture)
}

impl Mirror {
    fn prepare(&mut self, layout: PlaneLayout) -> Result<(), RenderError> {
        if self.pixels.is_some() {
            return Ok(());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(layout.required_bytes())
            .map_err(|_| capacity_error())?;
        let reservation = self.reservation.take().ok_or_else(capacity_error)?;
        let allocation = match reservation.commit(bytes.capacity()) {
            Ok(allocation) => allocation,
            Err(reservation) => {
                self.reservation = Some(reservation);
                return Err(capacity_error());
            }
        };
        bytes.resize(layout.required_bytes(), 0);
        self.pixels = Some(Handle::from_rc(EntryRc::new(CacheEntry::new(
            CachedPixels {
                bytes,
                _allocation: allocation,
            },
        ))));
        Ok(())
    }

    fn bytes_mut(&mut self) -> Result<&mut Vec<u8>, RenderError> {
        let pixels = self.pixels.as_mut().ok_or(RenderError::InvalidTexture)?;
        let entry = EntryRc::get_mut(&mut pixels.inner).ok_or(RenderError::InvalidScope)?;
        Ok(&mut entry.payload.bytes)
    }
}

// Keep the cache unavailable during external readback, without retaining a RefMut.
// Drop restores reusable storage on errors and unwinding as well as success.
struct MirrorRead<'a> {
    slot: &'a RefCell<Option<Mirror>>,
    mirror: Option<Mirror>,
}

impl<'a> MirrorRead<'a> {
    fn take(slot: &'a RefCell<Option<Mirror>>) -> Result<Self, RenderError> {
        let mirror = slot
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?
            .take()
            .ok_or(RenderError::InvalidScope)?;
        Ok(Self {
            slot,
            mirror: Some(mirror),
        })
    }
}

impl Drop for MirrorRead<'_> {
    fn drop(&mut self) {
        *self.slot.borrow_mut() = self.mirror.take();
    }
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
            .reserve(layout.required_bytes())
            .ok_or_else(capacity_error)?;
        let readback = budget
            .reserve(plan.readback_bytes)
            .ok_or_else(capacity_error)?;
        let target = renderer.create_target(&plan)?;
        if target.descriptor() != plan.descriptor || !renderer.can_reuse_target(&*target, &plan)? {
            return Err(RenderError::InvalidTexture);
        }
        target.validate()?;
        let storage = storage
            .commit(target.allocated_bytes())
            .map_err(|_| capacity_error())?;
        let cache_bytes = storage.bytes() + layout.required_bytes() + readback.bytes();
        Ok(Self {
            target,
            mirror: RefCell::new(Some(Mirror {
                pixels: None,
                reservation: Some(mirror),
                revision: None,
            })),
            storage,
            readback,
            cache_bytes: Cell::new(cache_bytes),
            #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
            identity: Rc::new(()),
            layout,
            workspace_bytes: plan.workspace_bytes,
            completed: false,
        })
    }

    pub(super) fn layout(&self) -> PlaneLayout {
        self.layout
    }

    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    pub(super) fn identity(&self) -> Rc<()> {
        Rc::clone(&self.identity)
    }

    pub(super) fn validate(&self, revision: u64) -> Result<(), RenderError> {
        if !self.completed {
            return Err(RenderError::InvalidTexture);
        }
        let slot = self
            .mirror
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let mirror = slot.as_ref().ok_or(RenderError::InvalidScope)?;
        if mirror.revision == Some(revision) {
            return Ok(());
        }
        drop(slot);
        self.target.validate()
    }

    pub(super) fn pixels(&self, revision: u64) -> Result<Option<TexturePixels<'_>>, RenderError> {
        if !self.completed {
            return Err(RenderError::InvalidTexture);
        }
        let slot = self
            .mirror
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        let mirror = slot.as_ref().ok_or(RenderError::InvalidScope)?;
        if mirror.revision == Some(revision) {
            return Ok(Some(TexturePixels::cached(
                mirror
                    .pixels
                    .as_ref()
                    .ok_or(RenderError::InvalidTexture)?
                    .clone(),
                self.layout,
            )));
        }
        drop(slot);
        self.target.validate()?;
        Ok(None)
    }

    pub(super) fn read_pixels(&self, revision: u64) -> Result<TexturePixels<'_>, RenderError> {
        if let Some(pixels) = self.pixels(revision)? {
            return Ok(pixels);
        }
        {
            let mut read = MirrorRead::take(&self.mirror)?;
            let mirror = read.mirror.as_mut().expect("active mirror read");
            mirror.revision = None;
            mirror.prepare(self.layout)?;
            let bytes = mirror.bytes_mut()?;
            self.cache_bytes
                .set(self.storage.bytes() + bytes.capacity() + self.readback.bytes());
            let mut destination = TexturePixelsMut::new(bytes, self.layout, AlphaMode::Blend);
            self.target
                .read_pixels_into(self.target.descriptor().region(), &mut destination)?;
            mirror.revision = Some(revision);
        }
        self.pixels(revision)?.ok_or(RenderError::InvalidTexture)
    }
}

impl HasSize for NativeTexture {
    fn cache_size(&self) -> usize {
        self.cache_bytes.get()
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
    /// Inspect completed native storage without reading pixels, allocating or submitting work.
    ///
    /// CPU textures return `None`. Invalid native resources also return `None` when a current CPU copy remains readable; consumers can use the normal pixel upload path. Incomplete contents and access errors are rejected. Native consumers must check the concrete payload, destination device identity and active-target feedback before sampling. This borrow does not permit target writes.
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
            Err(error @ (RenderError::InvalidTexture | RenderError::BackendFailure)) => {
                let mirror = native
                    .mirror
                    .try_borrow()
                    .map_err(|_| RenderError::InvalidScope)?;
                if mirror.as_ref().ok_or(RenderError::InvalidScope)?.revision
                    == Some(self.cache_revision)
                {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn set_output_revision(&mut self, revision: u64) {
        if let TexBuf::Native(native) = &mut self.buf {
            let mirror = native
                .mirror
                .get_mut()
                .as_mut()
                .expect("exclusive mirror access");
            if native.completed && mirror.revision == Some(self.cache_revision) {
                mirror.revision = Some(revision);
            }
        }
        self.cache_revision = revision;
        self.transient = false;
    }

    pub(crate) fn invalidate_native(&mut self) {
        if let TexBuf::Native(native) = &mut self.buf {
            native.completed = false;
            native
                .mirror
                .get_mut()
                .as_mut()
                .expect("exclusive mirror access")
                .revision = None;
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
        Ok(renderer.can_reuse_target(&*native.target, plan)?
            && native.workspace_bytes == plan.workspace_bytes
            && native.readback.bytes() == plan.readback_bytes)
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
        native
            .mirror
            .get_mut()
            .as_mut()
            .expect("exclusive mirror access")
            .revision = None;
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
    use crate::types::Color;
    use alloc::rc::{Rc, Weak};

    #[derive(Default)]
    struct Device {
        source: RefCell<Weak<NativeTexture>>,
        reads: Cell<usize>,
        fail: Cell<bool>,
        panic: Cell<bool>,
    }

    struct Target(Rc<Device>, PlaneLayout);

    impl BackendTarget for Target {
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
            if let Some(source) = self.0.source.borrow().upgrade() {
                assert!(source.mirror.try_borrow_mut().is_ok());
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
            self.0.reads.set(self.0.reads.get() + 1);
            let source = self.0.source.borrow().upgrade().unwrap();
            // Re-entry observes an in-flight read, not a held cache borrow.
            assert!(source.mirror.try_borrow_mut().is_ok());
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
        let device = Rc::new(Device::default());
        let layout = PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap();
        let budget = MemoryBudget::new(32);
        let source = Rc::new(NativeTexture {
            target: Box::new(Target(device.clone(), layout)),
            mirror: RefCell::new(Some(Mirror {
                pixels: None,
                reservation: Some(budget.reserve(16).unwrap()),
                revision: None,
            })),
            storage: budget.reserve(16).unwrap().commit(16).unwrap(),
            readback: budget.reserve(0).unwrap(),
            cache_bytes: Cell::new(32),
            #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
            identity: Rc::new(()),
            layout,
            workspace_bytes: 0,
            completed: true,
        });
        *device.source.borrow_mut() = Rc::downgrade(&source);
        (source, device, budget)
    }

    #[test]
    fn cached_views_pin_pixels_without_holding_the_cache_borrow() {
        let (source, device, _) = source();
        let first = source.read_pixels(0).unwrap();
        let address = first.bytes().as_ptr();
        let allocations = crate::test_support::tracked_allocations(|| {
            let second = source.read_pixels(0).unwrap();
            assert!(source.mirror.try_borrow_mut().is_ok());
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
        assert_eq!(budget.allocated(), 32);
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
            .borrow()
            .as_ref()
            .unwrap()
            .pixels
            .as_ref()
            .unwrap()
            .clone();
        source.mirror.borrow_mut().as_mut().unwrap().revision = None;
        assert!(matches!(
            source.read_pixels(1),
            Err(RenderError::InvalidScope)
        ));
        assert_eq!(retained.bytes, [0x66; 16]);
        drop(source);
        assert_eq!(budget.allocated(), 16);
        drop(retained);
        assert_eq!(budget.allocated(), 0);
        assert_eq!(budget.available(), 32);
    }
}
