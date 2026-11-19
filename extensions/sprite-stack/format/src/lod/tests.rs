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

#[test]
fn bake_samples_base_levels_directly_preserving_color_ties_and_transparency() {
    let region = [Region::new(0, 0, 5, 1).unwrap()];
    let descriptor =
        StackDescriptor::new((5, 1), [0; 2], 256, AtlasMap::new(5, 1, &region).unwrap()).unwrap();
    let colors = [9, 1, 1, 1, 0];
    let mut reads = [0; 5];
    let p = bake::encode(
        descriptor,
        bake::Samples::Rgba,
        &[1, 2, 4],
        &[],
        1024,
        |x, _| {
            reads[x as usize] += 1;
            let v = colors[x as usize];
            Ok(LodColor::Rgba([v, 0, 0, if v == 0 { 0 } else { 255 }]))
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(reads, [2; 5]);
    let mut asset = Document::new();
    let rgba = [255; 20];
    let image = asset
        .push_image(&ImageAsset::new(
            5,
            1,
            ColorFormat::RGBA8888,
            20,
            alloc::borrow::Cow::Borrowed(&rgba),
        ))
        .unwrap();
    asset.set_primary(image).unwrap();
    asset.push_extension(descriptor.extension()).unwrap();
    asset
        .push_extension(Extension::borrowed(CHUNK_TYPE, &p).with_policy(crate::relocation_policy()))
        .unwrap();
    let bytes = asset.encode(&EncodeOptions::new()).unwrap();
    let lods = open(&bytes).unwrap().unwrap();
    let colors = |n: usize| {
        lods.level(n)
            .unwrap()
            .layer(0)
            .unwrap()
            .map(|c| (c.x, c.color))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        colors(0),
        [
            (0, LodColor::Rgba([9, 0, 0, 255])),
            (2, LodColor::Rgba([1, 0, 0, 255]))
        ]
    );
    assert_eq!(colors(1), [(0, LodColor::Rgba([1, 0, 0, 255]))]);
}
#[test]
fn material_votes_use_order_and_round_only_within_the_winning_group() {
    let regions = [Region::new(0, 0, 2, 2).unwrap()];
    let d =
        StackDescriptor::new((2, 2), [0; 2], 256, AtlasMap::new(2, 2, &regions).unwrap()).unwrap();
    let mut palette = [255; 128];
    palette[3] = 0;
    let ramps = [[1, 16], [17, 31]];
    for (colors, expected) in [
        ([18, 17, 1, 0], 18),
        ([17, 18, 15, 16], 18),
        ([0, 0, 0, 0], 0),
    ] {
        let p = bake::encode(
            d,
            bake::Samples::Indexed(&palette),
            &[1, 2],
            &ramps,
            1024,
            |x, y| Ok(LodColor::Indexed(colors[(y * 2 + x) as usize])),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            u32_at(&p, HEADER + 4 + RECORD + 4),
            usize::from(expected != 0)
        );
        if expected > 0 {
            assert_eq!(p.last(), Some(&expected));
        }
    }
    assert_eq!(
        bake::encode(
            d,
            bake::Samples::Rgba,
            &[1, 2],
            &[],
            12,
            |_, _| unreachable!()
        ),
        Err(LodError::Limit)
    );
    assert!(
        bake::encode(
            d,
            bake::Samples::Rgba,
            &[1],
            &[],
            1024,
            |_, _| unreachable!()
        )
        .unwrap()
        .is_none()
    );
}
