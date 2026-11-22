use super::*;
use mirx::Document;
use mirx::document::{EncodeOptions, OpenOptions};
use mirx::extension::SourcePolicy;
use mirx::image::{ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor};

#[test]
fn editor_roundtrip_keeps_primary_atlas_and_ordered_empty_layers() {
    let regions = [
        Region::new(2, 0, 2, 1).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(0, 0, 2, 1).unwrap(),
    ];
    let map = AtlasMap::new(4, 1, &regions).unwrap();
    let descriptor = StackDescriptor::new((2, 1), [128, -64], 384, map).unwrap();
    let mut file = Document::new();
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let image = file
        .push_image(&RawImageAsset::new(surface, &[&[1, 2, 3, 4]]))
        .unwrap();
    file.set_primary(image).unwrap();
    file.push_extension(descriptor.extension()).unwrap();
    let source = file.encode(&EncodeOptions::new()).unwrap();
    let policies = [SourcePolicy::new(CHUNK_TYPE, relocation_policy())];
    let mut editor =
        Document::open_with(&source, &OpenOptions::new().with_source_policies(&policies)).unwrap();
    let extension = editor
        .chunks()
        .find(|chunk| chunk.chunk_type() == CHUNK_TYPE)
        .unwrap()
        .id();
    editor
        .move_before(extension, editor.primary().unwrap())
        .unwrap();
    let reordered = editor.encode(&EncodeOptions::new()).unwrap();
    for bytes in [&source, &reordered] {
        let reader = Reader::open(bytes).unwrap();
        let primary = reader.primary().unwrap().unwrap();
        assert_eq!(
            primary
                .image()
                .unwrap()
                .unwrap()
                .raw()
                .unwrap()
                .plane(0)
                .unwrap()
                .bytes(),
            &[1, 2, 3, 4]
        );
        let stack = StackDescriptor::from_reader(&reader).unwrap();
        assert_eq!(stack.canvas(), (2, 1));
        assert_eq!(stack.pivot_q8(), [128, -64]);
        assert_eq!(stack.spacing_q8(), 384);
        assert!(stack.layers().iter().eq(regions));
    }
    editor.push_extension(descriptor.extension()).unwrap();
    assert_eq!(
        StackDescriptor::from_mirx(&editor.encode(&EncodeOptions::new()).unwrap()),
        Err(StackError::DuplicateDescriptor)
    );
}

#[test]
fn rejects_invalid_layer_placement_and_wire_lengths() {
    let regions = [Region::new(1, 1, 3, 2).unwrap(); 3];
    let map = AtlasMap::new(4, 3, &regions).unwrap();
    assert_eq!(
        StackDescriptor::new((4, 2), [0, 0], 256, map),
        Err(StackError::InvalidRegion { layer: 0 })
    );
    assert_eq!(
        StackDescriptor::new((3, 2), [0, 0], i32::MAX, map),
        Err(StackError::InvalidSpacing)
    );
    assert_eq!(
        StackDescriptor::new((3, 2), [i32::MIN, 0], 0, map),
        Err(StackError::InvalidPivot)
    );
    let descriptor = StackDescriptor::new((3, 2), [0, 0], 0, map).unwrap();
    let mut bytes = vec![0; descriptor.encoded_len()];
    assert_eq!(
        descriptor.encode_into(&mut bytes[1..]),
        Err(StackError::OutputTooSmall)
    );
    descriptor.encode_into(&mut bytes).unwrap();
    assert_eq!(
        StackDescriptor::open(4, 3, &bytes[..23]),
        Err(StackError::InvalidHeader)
    );
    assert_eq!(
        StackDescriptor::open(4, 3, &bytes[..24]),
        Err(StackError::InvalidLayerCount)
    );
    assert!(matches!(
        StackDescriptor::open(3, 3, &bytes),
        Err(StackError::Atlas(_))
    ));
    bytes[4] = 2;
    assert_eq!(
        StackDescriptor::open(4, 3, &bytes),
        Err(StackError::UnsupportedVersion(2))
    );
}
