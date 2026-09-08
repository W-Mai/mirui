use mirx::image::{
    ColorDescription, ImageSource, RawImageAsset, RawImageView, SampleLayout, SurfaceDescriptor,
    SurfaceRequirements,
};
use mirx::{
    ChunkFlags, ChunkType, Document, EditError, EncodeOptions, Meta, PayloadLimits, Reader,
};

#[repr(align(64))]
struct AlignedFile([u8; 4096]);

#[test]
fn planar_images_preserve_geometry_hints_alignment_and_noop_edits() {
    for layout in [
        SampleLayout::NV12,
        SampleLayout::NV21,
        SampleLayout::P010,
        SampleLayout::I420,
    ] {
        let surface =
            SurfaceDescriptor::new(5, 3, layout, ColorDescription::BT709_YUV_LIMITED).unwrap();
        let plan = surface
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_plane_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_width_multiple(64)
                    .with_stride_multiple(64),
            )
            .unwrap();
        let memory: Vec<_> = plan.planes().collect();
        let buffers: Vec<_> = memory
            .iter()
            .enumerate()
            .map(|(index, plane)| vec![index as u8 + 17; plane.byte_len() as usize])
            .collect();
        let planes: Vec<_> = buffers.iter().map(Vec::as_slice).collect();
        let image = RawImageAsset::new(surface, &planes).with_memory_layouts(&memory);
        let source: &dyn ImageSource = &image;
        let mut document = Document::new();
        let id = document
            .push_image_with_flags(source, ChunkFlags::CRITICAL)
            .unwrap();
        document.set_primary(id).unwrap();
        let meta = document.push_meta(&Meta::default()).unwrap();
        document.move_before(meta, id).unwrap();
        assert_eq!(document.primary_hints().sample_layout(), layout);
        assert_eq!(document.primary_hints().stride(), memory[0].stride());
        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let mut aligned = AlignedFile([0; 4096]);
        aligned.0[..encoded.len()].copy_from_slice(&encoded);
        let reader = Reader::open(&aligned.0[..encoded.len()]).unwrap();
        reader
            .validate_known_payloads(&PayloadLimits::EMBEDDED)
            .unwrap();
        let chunk = reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == ChunkType::IMAGE)
            .unwrap();
        let raw = RawImageView::open_at(chunk.payload(), chunk.payload_offset()).unwrap();
        let observed = chunk.image().unwrap().unwrap().raw().unwrap();
        assert_eq!(observed.surface(), surface);
        assert!(observed.data_addresses_are_aligned());
        assert!(raw.packed().is_none());
        for (index, plane) in observed.planes().enumerate() {
            assert_eq!(plane.memory(), memory[index]);
            assert_eq!(plane.bytes(), buffers[index]);
            assert_eq!(plane.bytes().as_ptr() as usize % 64, 0);
            assert_eq!(plane.memory().stride() % 64, 0);
        }
        // A valid flash/file offset is not proof of actual in-memory alignment.
        let mut shifted = AlignedFile([0; 4096]);
        shifted.0[1..1 + encoded.len()].copy_from_slice(&encoded);
        let shifted_reader = Reader::open(&shifted.0[1..1 + encoded.len()]).unwrap();
        let shifted_view = shifted_reader
            .chunks()
            .find_map(|chunk| chunk.image().unwrap())
            .and_then(|image| image.raw())
            .unwrap();
        assert!(!shifted_view.data_addresses_are_aligned());

        let mut reopened = Document::open(&encoded).unwrap();
        let id = reopened
            .chunks_of_type(ChunkType::IMAGE)
            .next()
            .unwrap()
            .id();
        let pointer = reopened
            .get(id)
            .unwrap()
            .image()
            .unwrap()
            .raw()
            .unwrap()
            .plane(0)
            .unwrap()
            .bytes()
            .as_ptr();
        reopened.get_mut(id).unwrap().replace_image(source).unwrap();
        assert!(!reopened.is_dirty());
        assert_eq!(
            reopened
                .get(id)
                .unwrap()
                .image()
                .unwrap()
                .raw()
                .unwrap()
                .plane(0)
                .unwrap()
                .bytes()
                .as_ptr(),
            pointer
        );
        reopened
            .remove(
                reopened
                    .chunks_of_type(ChunkType::META)
                    .next()
                    .unwrap()
                    .id(),
            )
            .unwrap();
        assert_eq!(
            reopened.demote_to_flat(),
            Err(EditError::NotRepresentableAsFlat)
        );
    }
}

#[test]
fn planar_replacement_updates_primary_hints_and_invalid_sources_are_atomic() {
    let surface = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let image = RawImageAsset::new(surface, &[&[16; 4], &[128; 2]]);
    let mut document = Document::new();
    let id = document.push_image(&image).unwrap();
    document.set_primary(id).unwrap();
    let wide = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::P010,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let replacement = RawImageAsset::new(wide, &[&[0; 8], &[0; 4]]);
    document
        .get_mut(id)
        .unwrap()
        .replace_image(&replacement)
        .unwrap();
    assert_eq!(document.primary_hints().sample_layout(), SampleLayout::P010);
    assert_eq!(document.primary_hints().stride(), 4);
    let before = document.encode(&EncodeOptions::new()).unwrap();
    let malformed = RawImageAsset::new(wide, &[&[0; 7], &[0; 4]]);
    assert!(
        document
            .get_mut(id)
            .unwrap()
            .replace_image(&malformed)
            .is_err()
    );
    assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);
    let reader = Reader::open(&before).unwrap();
    assert_eq!(
        reader
            .chunks()
            .next()
            .unwrap()
            .image()
            .unwrap()
            .unwrap()
            .surface(),
        wide
    );
}
