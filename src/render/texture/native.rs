#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
use alloc::rc::Rc;
use alloc::{boxed::Box, vec::Vec};
use core::cell::{Cell, Ref, RefCell};

use super::{AlphaMode, TexBuf, Texture, TexturePixels, TexturePixelsMut};
use crate::core::cache::HasSize;
use crate::render::budget::{Allocation, MemoryBudget, Reservation};
use crate::render::{
    BackendTarget, PlaneLayout, RenderError, RenderResource, Renderer, TargetPlan, TargetUpdate,
};
use crate::types::Viewport;

pub(super) struct NativeTexture {
    target: Box<dyn BackendTarget>,
    mirror: RefCell<Mirror>,
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
    bytes: Vec<u8>,
    allocation: Option<Allocation>,
    reservation: Option<Reservation>,
    revision: Option<u64>,
}

fn capacity_error() -> RenderError {
    RenderError::ResourceLimit(RenderResource::Texture)
}

impl Mirror {
    fn prepare(&mut self, layout: PlaneLayout) -> Result<(), RenderError> {
        if self.allocation.is_some() {
            return Ok(());
        }
        self.bytes
            .try_reserve_exact(layout.required_bytes())
            .map_err(|_| capacity_error())?;
        let reservation = self.reservation.take().ok_or_else(capacity_error)?;
        match reservation.commit(self.bytes.capacity()) {
            Ok(allocation) => self.allocation = Some(allocation),
            Err(reservation) => {
                self.bytes = Vec::new();
                self.reservation = Some(reservation);
                return Err(capacity_error());
            }
        }
        self.bytes.resize(layout.required_bytes(), 0);
        Ok(())
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
        if target.descriptor() != plan.descriptor {
            return Err(RenderError::InvalidTexture);
        }
        target.validate()?;
        let storage = storage
            .commit(target.allocated_bytes())
            .map_err(|_| capacity_error())?;
        let cache_bytes = storage.bytes() + layout.required_bytes() + readback.bytes();
        Ok(Self {
            target,
            mirror: RefCell::new(Mirror {
                bytes: Vec::new(),
                allocation: None,
                reservation: Some(mirror),
                revision: None,
            }),
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
        let mirror = self
            .mirror
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        if mirror.revision == Some(revision) {
            return Ok(());
        }
        self.target.validate()
    }

    pub(super) fn pixels(&self, revision: u64) -> Result<Option<TexturePixels<'_>>, RenderError> {
        if !self.completed {
            return Err(RenderError::InvalidTexture);
        }
        let mirror = self
            .mirror
            .try_borrow()
            .map_err(|_| RenderError::InvalidScope)?;
        if mirror.revision == Some(revision) {
            return Ok(Some(TexturePixels::cached(
                Ref::map(mirror, |mirror| mirror.bytes.as_slice()),
                self.layout,
            )));
        }
        self.target.validate()?;
        Ok(None)
    }

    pub(super) fn read_pixels(&self, revision: u64) -> Result<TexturePixels<'_>, RenderError> {
        if let Some(pixels) = self.pixels(revision)? {
            return Ok(pixels);
        }
        {
            let mut mirror = self
                .mirror
                .try_borrow_mut()
                .map_err(|_| RenderError::InvalidScope)?;
            mirror.revision = None;
            mirror.prepare(self.layout)?;
            self.cache_bytes
                .set(self.storage.bytes() + mirror.bytes.capacity() + self.readback.bytes());
            let mut destination =
                TexturePixelsMut::new(&mut mirror.bytes, self.layout, AlphaMode::Blend);
            self.target.read_pixels_into(&mut destination)?;
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
                if mirror.revision == Some(self.cache_revision) {
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
            let mirror = native.mirror.get_mut();
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
            native.mirror.get_mut().revision = None;
        }
    }
    pub(crate) fn is_native(&self) -> bool {
        matches!(self.buf, TexBuf::Native(_))
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
        native.mirror.get_mut().revision = None;
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
