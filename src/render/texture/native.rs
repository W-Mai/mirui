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
#[path = "../../../tests/unit/render/texture/native.rs"]
mod tests;
