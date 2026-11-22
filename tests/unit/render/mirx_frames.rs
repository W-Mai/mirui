use super::*;
use mirx::{
    Document,
    frames::{FrameEncodingSet, FrameSequence, FramesEncoder},
    image::{ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements},
};

#[repr(align(64))]
struct Aligned<const N: usize>([u8; N]);

fn encoded_frames() -> alloc::vec::Vec<u8> {
    let surface =
        SurfaceDescriptor::new(2, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let sequence = FrameSequence::new(2, 1_000, 40).unwrap();
    let mut encoder = FramesEncoder::new(sequence, surface)
        .unwrap()
        .with_profiles(FrameEncodingSet::lossless())
        .unwrap();
    encoder.push(&[10, 20, 30, 40, 50, 60]).unwrap();
    encoder
        .push_with_duration(&[10, 20, 30, 41, 52, 63], 75)
        .unwrap();
    let mut document = Document::new();
    let id = document.push_frames(encoder.finish().unwrap()).unwrap();
    document.set_primary(id).unwrap();
    document.encode(&Default::default()).unwrap()
}

#[test]
fn plan_binds_aligned_storage_and_presents_textures_without_reallocation() {
    let bytes = encoded_frames();
    let options = MirxTextureOptions::new()
        .with_requirements(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                .with_width_multiple(64)
                .with_stride_multiple(64),
        )
        .with_input_memory(mirx::image::MemoryPlacement::Flash)
        .with_output_memory(mirx::image::MemoryPlacement::SharedNoncoherent)
        .with_workspace_memory(mirx::image::MemoryPlacement::SharedCoherent)
        .with_workspace_alignment(mirx::types::ByteAlignment::new(64).unwrap());
    let mut slots = [None];
    let plan = MirxFramesPlan::open(&bytes, options, &mut slots).unwrap();
    assert_eq!(plan.meta().width, 2);
    assert_eq!(plan.frame_count(), 2);
    assert_eq!(plan.timescale_hz(), 1_000);
    assert_eq!(plan.duration_ticks(0), Some(40));
    assert_eq!(plan.duration_ticks(1), Some(75));
    assert_eq!(plan.timeline().cycle_duration_ticks(), 115);
    assert_eq!(plan.frame_at_ticks(39).unwrap().frame(), 0);
    assert_eq!(plan.frame_at_ticks(40).unwrap().frame(), 1);
    assert_eq!(plan.frame_at_ticks(115).unwrap().play(), 1);
    assert_eq!(plan.canvas_requirements().byte_len(), 192);
    assert_eq!(plan.canvas_requirements().base_alignment(), 64);
    assert_eq!(plan.workspace_requirements().base_alignment(), 64);
    assert_eq!(plan.decode_request(), options.decode_request());
    assert_eq!(plan.input_sync(), mirx::image::CacheSync::None);
    assert_eq!(plan.output_sync(), mirx::image::CacheSync::CleanAfterWrite);
    assert!(plan.input_addresses_are_aligned());

    let mut canvas = Aligned([0; 256]);
    let mut workspace = Aligned([0; 64]);
    let mut session = plan
        .bind(MirxFramesStorage {
            groups: &mut slots,
            canvas: &mut canvas.0,
            workspace: &mut workspace.0,
            backup: &mut [],
        })
        .unwrap();
    assert_eq!(session.decode_request(), options.decode_request());
    assert_eq!(
        session.output_sync(),
        mirx::image::CacheSync::CleanAfterWrite
    );
    let first_ptr = {
        let texture = session.present(0).unwrap();
        assert_eq!(texture.stride, 192);
        assert_eq!(
            texture.read_pixels().unwrap().bytes().as_ptr() as usize % 64,
            0
        );
        assert_eq!(
            &texture.read_pixels().unwrap().bytes()[..6],
            &[10, 20, 30, 40, 50, 60]
        );
        texture.read_pixels().unwrap().bytes().as_ptr()
    };
    let (position, second) = session.present_at(40).unwrap().unwrap();
    assert_eq!(position.frame(), 1);
    assert_eq!(position.elapsed_ticks(), 0);
    assert_eq!(second.read_pixels().unwrap().bytes().as_ptr(), first_ptr);
    assert_eq!(
        &second.read_pixels().unwrap().bytes()[..6],
        &[10, 20, 30, 41, 52, 63]
    );
    assert_eq!(session.current_frame(), Some(1));
}

#[test]
fn successive_indexed_frames_borrow_one_canvas_with_the_original_palette() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::I2, ColorDescription::SRGB).unwrap();
    let palette = [
        5, 7, 9, 255, 10, 20, 30, 128, 40, 50, 60, 0, 70, 80, 90, 255,
    ];
    let mut encoder = FramesEncoder::new(FrameSequence::new(2, 1_000, 40).unwrap(), surface)
        .unwrap()
        .with_color_table(&palette)
        .unwrap();
    encoder.push(&[0b00_01_10_11]).unwrap();
    encoder.push(&[0b11_10_01_00]).unwrap();
    let mut document = Document::new();
    let id = document.push_frames(encoder.finish().unwrap()).unwrap();
    document.set_primary(id).unwrap();
    let bytes = document.encode(&Default::default()).unwrap();
    let mut groups = [None; 4];
    let plan = MirxFramesPlan::open(&bytes, MirxTextureOptions::new(), &mut groups).unwrap();
    let mut canvas = Aligned([0; 128]);
    let mut workspace = Aligned([0; 512]);
    let mut backup = Aligned([0; 128]);
    let mut session = plan
        .bind(MirxFramesStorage {
            groups: &mut groups,
            canvas: &mut canvas.0,
            workspace: &mut workspace.0,
            backup: &mut backup.0,
        })
        .unwrap();
    for (frame, index) in [(0, 1usize), (1, 2)] {
        let texture = session.present(frame).unwrap();
        assert!(texture.transient);
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.sample_format(), mirx::image::ColorFormat::I2);
        let i = index * 4;
        assert_eq!(
            pixels.get_pixel(1, 0),
            crate::types::Color::rgba(palette[i], palette[i + 1], palette[i + 2], palette[i + 3])
        );
    }
}

#[test]
fn plan_rejects_static_images_and_unsupported_render_layouts() {
    let flat = Document::new_flat(mirx::image::ImageAsset::new(
        1,
        1,
        mirx::image::ColorFormat::RGB565,
        2,
        alloc::borrow::Cow::Borrowed(&[0, 0]),
    ))
    .unwrap()
    .finish()
    .unwrap()
    .into_owned();
    assert!(matches!(
        MirxFramesPlan::open(&flat, MirxTextureOptions::new(), &mut []),
        Err(MirxFramesError::NoFramesChunk)
    ));

    let surface = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::I420,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
    let profiles = FrameEncodingSet::lossless()
        .with_rle(false)
        .with_pixel(false)
        .with_lz4(false)
        .with_reversible_frequency(false)
        .with_delta(false);
    let mut encoder = FramesEncoder::new(sequence, surface)
        .unwrap()
        .with_profiles(profiles)
        .unwrap();
    encoder.push(&[1, 2, 3, 4, 5, 6]).unwrap();
    let mut document = Document::new();
    let id = document.push_frames(encoder.finish().unwrap()).unwrap();
    document.set_primary(id).unwrap();
    let bytes = document.encode(&Default::default()).unwrap();
    assert!(matches!(
        MirxFramesPlan::open(&bytes, MirxTextureOptions::new(), &mut [None]),
        Err(MirxFramesError::UnsupportedLayout(SampleLayout::I420))
    ));
}
