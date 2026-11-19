use super::*;
use alloc::{vec, vec::Vec};
use mirx::{
    Document,
    document::EncodeOptions,
    extension::Extension,
    image::{AtlasMap, ColorFormat, ImageAsset, Region},
};

fn rgba_file(payload: &[u8]) -> Vec<u8> {
    let regions = [
        Region::new(0, 0, 3, 1).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
    ];
    let desc =
        StackDescriptor::new((3, 1), [0; 2], 256, AtlasMap::new(3, 1, &regions).unwrap()).unwrap();
    let mut doc = Document::new();
    let asset = ImageAsset::new(
        3,
        1,
        ColorFormat::RGBA8888,
        12,
        alloc::borrow::Cow::Borrowed(&[255; 12]),
    );
    let image = doc.push_image(&asset).unwrap();
    doc.set_primary(image).unwrap();
    doc.push_extension(desc.extension()).unwrap();
    doc.push_extension(
        Extension::borrowed(CHUNK_TYPE, payload).with_policy(crate::relocation_policy()),
    )
    .unwrap();
    doc.encode(&EncodeOptions::new()).unwrap()
}
fn open(bytes: &[u8]) -> Result<Option<StackLods<'_>>, LodError> {
    let reader = Reader::open(bytes).unwrap();
    let (d, img) = StackDescriptor::from_reader_with_atlas(&reader).unwrap();
    StackLods::from_reader(&reader, d, img)
}
#[test]
fn sparse_levels_keep_empty_layers_and_an_unpadded_last_cell() {
    let mut p = vec![
        b'S', b'L', b'O', b'D', 1, 0, 4, 0, 1, 0, 2, 0, 0, 0, 3, 0, 1, 0, 0, 0, 2, 0, 0, 0, 32, 0,
        0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 7, 8, 9, 255,
    ];
    let bytes = rgba_file(&p);
    let lods = open(&bytes).unwrap().unwrap();
    assert_eq!(lods.len(), 1);
    let level = lods.level(0).unwrap();
    assert_eq!(
        level.layer(0).unwrap().collect::<Vec<_>>(),
        [LodCell {
            x: 2,
            y: 0,
            color: LodColor::Rgba([7, 8, 9, 255])
        }]
    );
    assert_eq!(level.layer(1).unwrap().len(), 0);
    assert!(level.layer(usize::MAX).is_none());
    for (offset, value) in [
        (8, 2),
        (20, 1),
        (24, 33),
        (28, 255),
        (32, 1),
        (36, 2),
        (40, 0),
        (44, 1),
        (51, 0),
    ] {
        let old = core::mem::replace(&mut p[offset], value);
        assert!(open(&rgba_file(&p)).is_err(), "offset {offset}");
        p[offset] = old;
    }
}
