use super::*;
use wgpu::util::DeviceExt;

#[test]
fn readback_reuses_mapping_storage_and_preserves_destination_layout() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        std::eprintln!("SKIP readback checks: no GPU adapter");
        return;
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).expect("readback device");
    let mut readback = Readback::default();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let mut source = [0u8; 24];
        for (index, pixel) in source.chunks_exact_mut(4).enumerate() {
            let mut value = [index as u8 * 8, 32, 64, 128];
            if is_bgra(format) {
                value.swap(0, 2);
            }
            pixel.copy_from_slice(&value);
        }
        let target = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("mirui-readback-source"),
                size: wgpu::Extent3d {
                    width: 3,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &source,
        );
        for (x, y, w, h) in [(0, 0, 0, 1), (0, 0, 1, 0), (2, 0, 2, 1), (0, 2, 1, 1)] {
            assert!(matches!(
                readback.read(
                    &device,
                    &queue,
                    &target,
                    PhysicalRect::new(x, y, w, h).unwrap()
                ),
                Err(RenderError::InvalidGeometry),
            ));
        }
        let rect = PhysicalRect::new(1, 0, 2, 2).unwrap();
        let expected = [
            16, 64, 128, 128, 32, 64, 128, 128, 64, 64, 128, 128, 80, 64, 128, 128,
        ];
        readback
            .read(&device, &queue, &target, PhysicalRect::from_size(1, 1))
            .unwrap();
        let small_buffer = readback.staging.as_ref().unwrap().buffer.clone();
        let pixels = readback.read(&device, &queue, &target, rect).unwrap();
        assert_ne!(readback.staging.as_ref().unwrap().buffer, small_buffer);
        assert_eq!(pixels.read_pixels().unwrap().bytes(), expected);
        let buffer = readback.staging.as_ref().unwrap().buffer.clone();
        let state = Arc::downgrade(&readback.staging.as_ref().unwrap().mapped);
        let mut bytes = [0xabu8; 20];
        let mut dst = Texture::new(&mut bytes, 2, 2, ColorFormat::RGBA8888)
            .unwrap()
            .with_stride(12)
            .unwrap();
        for _ in 0..3 {
            readback
                .read_into(
                    &device,
                    &queue,
                    &target,
                    rect,
                    &mut dst.pixels_mut().unwrap(),
                )
                .unwrap();
            let pixels = dst.read_pixels().unwrap();
            assert_eq!(&pixels.bytes()[..8], &expected[..8]);
            assert_eq!(&pixels.bytes()[8..12], &[0xab; 4]);
            assert_eq!(&pixels.bytes()[12..], &expected[8..]);
            assert_eq!(readback.staging.as_ref().unwrap().buffer, buffer);
            assert!(Arc::ptr_eq(
                &readback.staging.as_ref().unwrap().mapped,
                &state.upgrade().unwrap()
            ));
        }
        let smaller = readback
            .read(&device, &queue, &target, PhysicalRect::from_size(1, 1))
            .unwrap();
        assert_eq!(smaller.read_pixels().unwrap().bytes(), [0, 64, 128, 128]);
        assert_eq!(readback.staging.as_ref().unwrap().buffer, buffer);
        let mut wrong_size = Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap();
        assert_eq!(
            readback.read_into(
                &device,
                &queue,
                &target,
                rect,
                &mut wrong_size.pixels_mut().unwrap()
            ),
            Err(RenderError::InvalidTexture),
        );
        assert_eq!(readback.staging.as_ref().unwrap().buffer, buffer);
        let before = dst.read_pixels().unwrap().bytes().to_vec();
        buffer.destroy();
        let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
        assert_eq!(
            readback.read_into(
                &device,
                &queue,
                &target,
                rect,
                &mut dst.pixels_mut().unwrap()
            ),
            Err(RenderError::BackendFailure),
        );
        assert!(pollster::block_on(errors.pop()).is_some());
        assert!(readback.staging.is_none());
        assert_eq!(dst.read_pixels().unwrap().bytes(), before);
        readback
            .read_into(
                &device,
                &queue,
                &target,
                rect,
                &mut dst.pixels_mut().unwrap(),
            )
            .unwrap();
        assert_ne!(readback.staging.as_ref().unwrap().buffer, buffer);
        drop(buffer);
        readback = Readback::default();
        assert!(state.upgrade().is_none());
    }

    let mut descriptor = wgpu::TextureDescriptor {
        label: Some("mirui-unreadable-source"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    let rect = PhysicalRect::from_size(1, 1);
    assert!(matches!(
        readback.read(&device, &queue, &device.create_texture(&descriptor), rect),
        Err(RenderError::InvalidTexture)
    ));
    descriptor.format = wgpu::TextureFormat::Rgba16Float;
    descriptor.usage = wgpu::TextureUsages::COPY_SRC;
    assert!(matches!(
        readback.read(&device, &queue, &device.create_texture(&descriptor), rect),
        Err(RenderError::Unsupported(RenderFeature::Readback))
    ));
    descriptor.format = wgpu::TextureFormat::Rgba8Unorm;
    readback
        .read(&device, &queue, &device.create_texture(&descriptor), rect)
        .unwrap();
    let first_buffer = readback.staging.as_ref().unwrap().buffer.clone();
    let (other_device, other_queue) =
        pollster::block_on(adapter.request_device(&Default::default())).expect("second device");
    let pixels = readback
        .read(
            &other_device,
            &other_queue,
            &other_device.create_texture(&descriptor),
            rect,
        )
        .unwrap();
    assert_eq!(pixels.read_pixels().unwrap().bytes(), [0; 4]);
    assert_eq!(readback.staging.as_ref().unwrap().device, other_device);
    assert_ne!(readback.staging.as_ref().unwrap().buffer, first_buffer);
    std::eprintln!("readback storage and pixel checks completed");
}
