use mirx::{
    coding::CodingRecord,
    font::GlyphSurfaceAsset,
    image::{EncodedImageAsset, SurfaceDescriptor},
};

fn retained<'a>(
    surface: SurfaceDescriptor,
    data: &'a [u8],
    glyphs: GlyphSurfaceAsset<'a>,
) {
    let _ = EncodedImageAsset::new(surface, CodingRecord::RAW, data).with_unit_index(&[]);
    let _ = glyphs.data();
}

fn main() {}
