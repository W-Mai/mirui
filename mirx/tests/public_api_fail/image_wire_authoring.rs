use mirx::{
    coding::CodingRecord,
    font::GlyphSurfaceAsset,
    image::{EncodedImageAsset, SurfaceDescriptor, UnitGroupRecord},
};

fn retained<'a>(
    surface: SurfaceDescriptor,
    codings: &'a [CodingRecord<'a>],
    groups: &'a [UnitGroupRecord],
    data: &'a [u8],
    glyphs: GlyphSurfaceAsset<'a>,
) {
    let _ = EncodedImageAsset::from_records(surface, codings, groups, data);
    let _ = EncodedImageAsset::new(surface, CodingRecord::RAW, data)
        .with_groups(groups)
        .with_unit_index(&[]);
    let _ = EncodedImageAsset::new(surface, CodingRecord::RAW, data).with_unit_index(&[]);
    let _ = glyphs.data();
}

fn main() {}
