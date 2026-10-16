use super::*;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed};

#[path = "target_tests/source.rs"]
mod source;

fn context() -> Option<Arc<WgpuDeviceContext>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        std::eprintln!("SKIP native target checks: no GPU adapter");
        return None;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(WgpuDeviceContext::new(instance, adapter, device, queue))
}

fn completed(target: &dyn BackendTarget, color: wgpu::Color) {
    let storage = &target.as_any().downcast_ref::<Target>().unwrap().storage;
    let mut encoder = storage
        .context
        .device
        .create_command_encoder(&Default::default());
    {
        let frame = &storage.frame;
        let (view, resolve_target) = match &frame.msaa_view {
            Some(view) => (view, Some(&frame.view)),
            None => (&frame.view, None),
        };
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    storage.context.queue.submit(Some(encoder.finish()));
    storage.completed.set(true);
}

#[test]
fn native_source_owns_device_and_checks_completed_contents() {
    let Some(context) = context() else {
        return;
    };
    let weak = Arc::downgrade(&context);
    let layout = PlaneLayout::packed(3, 2, ColorFormat::RGBA8888).unwrap();
    let plan = plan(&context, 1, layout).unwrap().unwrap();
    assert_eq!(plan.storage_bytes, 24);
    assert_eq!(plan.readback_bytes, 512);
    let target = create(&context, 1, &plan).unwrap();
    assert_eq!(target.allocated_bytes(), plan.storage_bytes);
    assert_eq!(target.descriptor().alpha, TargetAlpha::Premultiplied);
    let mut bytes = [0xabu8; 28];
    let mut image = Texture::new(&mut bytes, 3, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(16)
        .unwrap();
    assert_eq!(
        target.read_pixels_into(&mut image.pixels_mut().unwrap()),
        Err(RenderError::InvalidTexture)
    );
    completed(
        &*target,
        wgpu::Color {
            r: 0.25,
            g: 0.0,
            b: 0.0,
            a: 0.5,
        },
    );
    let storage = &target.as_any().downcast_ref::<Target>().unwrap().storage;
    storage.active.set(true);
    assert_eq!(
        target.read_pixels_into(&mut image.pixels_mut().unwrap()),
        Err(RenderError::InvalidTexture)
    );
    storage.active.set(false);
    {
        let _lease = storage.readback.borrow_mut();
        assert_eq!(
            target.read_pixels_into(&mut image.pixels_mut().unwrap()),
            Err(RenderError::InvalidScope)
        );
    }
    drop(context);
    assert!(weak.upgrade().is_some());
    for _ in 0..3 {
        target
            .read_pixels_into(&mut image.pixels_mut().unwrap())
            .unwrap();
        let pixels = image.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(0, 0), Color::rgba(128, 0, 0, 128));
        assert_eq!(pixels.get_pixel(2, 1), Color::rgba(128, 0, 0, 128));
        assert_eq!(&pixels.bytes()[12..16], &[0xab; 4]);
    }
    drop(target);
    assert!(weak.upgrade().is_none());
    std::eprintln!("native source lifetime and checked readback passed");
}

#[test]
fn native_target_plan_matches_attachments_and_rejects_invalid_requests() {
    let Some(context) = context() else {
        return;
    };
    let layout = PlaneLayout::packed(8, 6, ColorFormat::RGBA8888).unwrap();
    assert!(plan(&context, 0, layout).unwrap().is_none());
    assert!(plan(&context, 3, layout).unwrap().is_none());
    assert!(
        plan(
            &context,
            1,
            PlaneLayout::packed(8, 6, ColorFormat::RGB565).unwrap()
        )
        .unwrap()
        .is_none()
    );
    for samples in [1, 4] {
        let Some(plan) = plan(&context, samples, layout).unwrap() else {
            continue;
        };
        let target = create(&context, samples, &plan).unwrap();
        assert_eq!(
            plan.storage_bytes,
            layout.required_bytes() * if samples == 1 { 1 } else { 5 }
        );
        let frame = &target
            .as_any()
            .downcast_ref::<Target>()
            .unwrap()
            .storage
            .frame;
        assert_eq!(frame.texture().sample_count(), 1);
        assert_eq!(
            frame
                .pipeline_key(
                    super::super::ShaderKind::QuadSdf,
                    super::super::CompositeMode::SourceOver
                )
                .samples,
            samples
        );
        assert_eq!(
            frame
                .viewport_uniform(Viewport::new(8, 6, Fixed::ONE / Fixed::from_int(2)))
                .size,
            [16.0, 12.0]
        );
        completed(&*target, wgpu::Color::TRANSPARENT);
        let mut image = Texture::owned(8, 6, ColorFormat::RGBA8888).unwrap();
        target
            .read_pixels_into(&mut image.pixels_mut().unwrap())
            .unwrap();
        assert!(
            image
                .read_pixels()
                .unwrap()
                .bytes()
                .iter()
                .all(|byte| *byte == 0)
        );
        let mut wrong_plan = plan;
        wrong_plan.storage_bytes += 1;
        assert!(matches!(
            create(&context, samples, &wrong_plan),
            Err(RenderError::Unsupported(RenderFeature::Scope))
        ));
        let mut wrong_plan = plan;
        wrong_plan.readback_bytes = 0;
        assert!(matches!(
            create(&context, samples, &wrong_plan),
            Err(RenderError::Unsupported(RenderFeature::Scope))
        ));
    }
    std::eprintln!("native target attachment planning passed");
}

#[test]
fn abandoned_binding_invalidates_source_device_without_owner_cycles() {
    let Some(context) = context() else {
        return;
    };
    let weak = Arc::downgrade(&context);
    let plan = plan(
        &context,
        1,
        PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap(),
    )
    .unwrap()
    .unwrap();
    let target = create(&context, 1, &plan).unwrap();
    let storage = &target.as_any().downcast_ref::<Target>().unwrap().storage;
    storage.active.set(true);
    drop(TargetState {
        child: Rc::clone(storage),
        parent: None,
        viewport: Viewport::new(2, 2, Fixed::ONE),
    });
    assert!(!storage.active.get());
    assert!(!storage.completed.get());
    assert_eq!(target.validate(), Err(RenderError::BackendFailure));
    drop(context);
    drop(target);
    assert!(weak.upgrade().is_none());
}

#[test]
fn device_loss_rejects_retained_native_source() {
    let Some(context) = context() else {
        return;
    };
    let plan = plan(
        &context,
        1,
        PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap(),
    )
    .unwrap()
    .unwrap();
    let target = create(&context, 1, &plan).unwrap();
    completed(&*target, wgpu::Color::GREEN);
    context.device.destroy();
    let _ = context.device.poll(wgpu::PollType::wait_indefinitely());
    assert_eq!(target.validate(), Err(RenderError::BackendFailure));
    let mut image = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
    assert_eq!(
        target.read_pixels_into(&mut image.pixels_mut().unwrap()),
        Err(RenderError::BackendFailure)
    );
    assert!(
        image
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn target_switches_reuse_frame_workspace_without_a_window_attachment() {
    let Some(context) = context() else {
        return;
    };
    let plan = plan(
        &context,
        1,
        PlaneLayout::packed(8, 8, ColorFormat::RGBA8888).unwrap(),
    )
    .unwrap()
    .unwrap();
    let target = create(&context, 1, &plan).unwrap();
    let attachment = &target
        .as_any()
        .downcast_ref::<Target>()
        .unwrap()
        .storage
        .frame;
    let viewport = Viewport::new(8, 8, Fixed::ONE);
    let mut frame = Frame::new(&context.device, attachment.clone(), viewport);
    let viewport_buffer = frame.viewport_buf.clone();
    let uniforms = frame.uniform_arena.clone();
    frame.uniform_cursor = 256;
    frame.ops.reserve(8);
    let capacity = frame.ops.capacity();
    for _ in 0..3 {
        let previous = frame
            .replace_target(None, viewport, &context.queue)
            .unwrap();
        assert_eq!(previous.texture(), attachment.texture());
        assert!(frame.target.is_none());
        assert!(
            frame
                .replace_target(Some(attachment.clone()), viewport, &context.queue)
                .is_none()
        );
        assert_eq!(frame.viewport_buf, viewport_buffer);
        assert_eq!(frame.uniform_arena, uniforms);
        assert_eq!(frame.uniform_cursor, 256);
        assert_eq!(frame.ops.capacity(), capacity);
    }
}
