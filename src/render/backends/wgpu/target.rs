use alloc::{boxed::Box, rc::Rc, sync::Arc};
use core::{
    any::Any,
    cell::{Cell, RefCell},
};

use super::{Frame, FrameTarget, WgpuRenderer, readback::Readback};
use crate::render::texture::{ColorFormat, TexturePixelsMut};
use crate::render::{
    BackendTarget, PlaneLayout, RenderError, RenderFeature, RenderResource, TargetAlpha,
    TargetDescriptor, TargetPlan, TargetUpdate,
};
use crate::surface::wgpu_surface::{WgpuDeviceContext, WgpuTarget};
use crate::types::{PhysicalRect, Viewport};

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;

pub(super) struct Storage {
    pub(super) context: Arc<WgpuDeviceContext>,
    pub(super) frame: FrameTarget,
    descriptor: TargetDescriptor,
    bytes: usize,
    readback: RefCell<Readback>,
    pub(super) completed: Cell<bool>,
    pub(super) active: Cell<bool>,
}

pub(super) struct Target {
    pub(super) storage: Rc<Storage>,
}

impl Target {
    pub(super) fn sample_view(
        &self,
        context: &Arc<WgpuDeviceContext>,
    ) -> Result<Option<wgpu::TextureView>, RenderError> {
        self.validate()?;
        if self.storage.active.get() || !self.storage.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        Ok(Arc::ptr_eq(&self.storage.context, context).then(|| self.storage.frame.view.clone()))
    }
}

/// Saved attachment and viewport for one native target scope.
pub struct TargetState {
    pub(super) child: Rc<Storage>,
    pub(super) parent: Option<FrameTarget>,
    pub(super) viewport: Viewport,
}

impl Drop for TargetState {
    fn drop(&mut self) {
        if self.child.active.replace(false) {
            self.child.completed.set(false);
            self.child.context.invalidate();
        }
    }
}

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        self.storage.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.storage.bytes
    }
    fn validate(&self) -> Result<(), RenderError> {
        self.storage.context.check()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read_pixels_into(&self, destination: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        self.validate()?;
        if self.storage.active.get() || !self.storage.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        let layout = self.descriptor().layout;
        self.storage
            .readback
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?
            .read_into(
                &self.storage.context.device,
                &self.storage.context.queue,
                self.storage.frame.texture(),
                PhysicalRect::from_size(layout.width(), layout.height()),
                destination,
            )?;
        self.storage.context.check()
    }
}

pub(super) fn plan(
    context: &WgpuDeviceContext,
    samples: u32,
    layout: PlaneLayout,
) -> Result<Option<TargetPlan>, RenderError> {
    context.check()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let features = context.format_features(format);
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC;
    if layout.format() != ColorFormat::RGBA8888
        || PlaneLayout::packed(layout.width(), layout.height(), layout.format()).ok()
            != Some(layout)
        || u32::from(layout.width()) > context.device.limits().max_texture_dimension_2d
        || u32::from(layout.height()) > context.device.limits().max_texture_dimension_2d
        || !features.allowed_usages.contains(usage)
        || !features.flags.sample_count_supported(samples)
        || (samples > 1
            && !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
    {
        return Ok(None);
    }
    let planes = if samples == 1 {
        1
    } else {
        samples as usize + 1
    };
    Ok(Some(TargetPlan {
        descriptor: TargetDescriptor {
            layout,
            alpha: TargetAlpha::Premultiplied,
        },
        storage_bytes: layout
            .required_bytes()
            .checked_mul(planes)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?,
        workspace_bytes: 0,
        readback_bytes: Readback::required_bytes(&context.device, layout.width(), layout.height())?,
    }))
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    pub(super) fn bind_native_target(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        update: TargetUpdate,
    ) -> Result<TargetState, RenderError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self
            .frame
            .as_ref()
            .is_some_and(|frame| !frame.ops.is_empty())
        {
            return Err(RenderError::InvalidScope);
        }
        let context = self
            .surface
            .state()
            .ok_or(RenderError::BackendFailure)?
            .context();
        context.check()?;
        let child = &target
            .as_any()
            .downcast_ref::<Target>()
            .ok_or(RenderError::InvalidTexture)?
            .storage;
        if !Arc::ptr_eq(context, &child.context) {
            return Err(RenderError::InvalidTexture);
        }
        if child.active.get() {
            return Err(RenderError::InvalidScope);
        }
        if update == TargetUpdate::Incremental && !child.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        let layout = child.descriptor.layout;
        if viewport.physical_size() != (layout.width(), layout.height()) {
            return Err(RenderError::InvalidGeometry);
        }
        let mut attachment = child.frame.clone();
        attachment.has_committed_pass = update == TargetUpdate::Incremental;
        let parent = if let Some(frame) = self.frame.as_mut() {
            frame.replace_target(Some(attachment), viewport, &child.context.queue)
        } else {
            self.frame = Some(Frame::new(&child.context.device, attachment, viewport));
            None
        };
        let state = TargetState {
            child: Rc::clone(child),
            parent,
            viewport: core::mem::replace(&mut self.viewport, viewport),
        };
        child.active.set(true);
        child.completed.set(false);
        if let Err(error) = self.flush_ops() {
            let _ = self.restore_native_target(state, false);
            return Err(error);
        }
        Ok(state)
    }

    pub(super) fn restore_native_target(
        &mut self,
        mut state: TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        let valid = self.surface.state().is_some_and(|context| {
            Arc::ptr_eq(context.context(), &state.child.context) && context.check().is_ok()
        }) && state.child.active.get()
            && self.frame.as_ref().is_some_and(|frame| {
                frame
                    .target
                    .as_ref()
                    .is_some_and(|target| target.texture() == state.child.frame.texture())
                    && (!completed || frame.ops.is_empty())
            });
        if !valid {
            state.child.context.invalidate();
            if let Some(context) = self.surface.state() {
                context.invalidate();
            }
            self.failure.get_or_insert(RenderError::BackendFailure);
            return Err(RenderError::BackendFailure);
        }
        let successful = completed && self.failure.is_none();
        let frame = self.frame.as_mut().ok_or(RenderError::InvalidScope)?;
        if !successful {
            frame.ops.clear();
            frame.encoder = state.child.context.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("mirui-wgpu-encoder"),
                },
            );
        }
        frame.replace_target(
            state.parent.take(),
            state.viewport,
            &state.child.context.queue,
        );
        self.viewport = state.viewport;
        self.failure = None;
        state.child.active.set(false);
        state.child.completed.set(successful);
        Ok(())
    }
}

pub(super) fn create(
    context: &Arc<WgpuDeviceContext>,
    samples: u32,
    requested: &TargetPlan,
) -> Result<Box<dyn BackendTarget>, RenderError> {
    if plan(context, samples, requested.descriptor.layout)? != Some(*requested) {
        return Err(RenderError::Unsupported(RenderFeature::Scope));
    }
    let layout = requested.descriptor.layout;
    let errors = context
        .device
        .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = context
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let internal = context.device.push_error_scope(wgpu::ErrorFilter::Internal);
    let mut descriptor = wgpu::TextureDescriptor {
        label: Some("mirui-native-target"),
        size: wgpu::Extent3d {
            width: u32::from(layout.width()),
            height: u32::from(layout.height()),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let view = context
        .device
        .create_texture(&descriptor)
        .create_view(&Default::default());
    descriptor.sample_count = samples;
    descriptor.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
    let msaa_view = (samples > 1).then(|| {
        context
            .device
            .create_texture(&descriptor)
            .create_view(&Default::default())
    });
    let internal = pollster::block_on(internal.pop());
    let validation = pollster::block_on(validation.pop());
    let allocation = pollster::block_on(errors.pop());
    if allocation.is_some() {
        return Err(RenderError::ResourceLimit(RenderResource::Target));
    }
    if internal.is_some() || validation.is_some() {
        return Err(RenderError::BackendFailure);
    }
    context.check()?;
    Ok(Box::new(Target {
        storage: Rc::new(Storage {
            context: Arc::clone(context),
            frame: FrameTarget {
                view,
                msaa_view,
                has_committed_pass: false,
                native: true,
            },
            descriptor: requested.descriptor,
            bytes: requested.storage_bytes,
            readback: Default::default(),
            completed: Cell::new(false),
            active: Cell::new(false),
        }),
    }))
}
