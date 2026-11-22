use super::*;
use alloc::boxed::Box;
use mirx::Document;
use mirx::document::EncodeOptions;
use mirx::image::{
    AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor,
};

fn file(pixels: &[u8; 8], with_stack: bool) -> &'static [u8] {
    let surface =
        SurfaceDescriptor::new(2, 1, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    sprite_file(RawImageAsset::new(surface, &[pixels]), with_stack)
}

fn sprite_file(image: RawImageAsset<'_, '_>, with_stack: bool) -> &'static [u8] {
    let mut file = Document::new();
    let atlas = file.push_image(&image).unwrap();
    file.set_primary(atlas).unwrap();
    if with_stack {
        let regions = [
            Region::new(1, 0, 1, 1).unwrap(),
            Region::new(0, 0, 1, 1).unwrap(),
        ];
        let map = AtlasMap::new(2, 1, &regions).unwrap();
        let stack = StackDescriptor::new((1, 1), [0, 0], 256, map).unwrap();
        file.push_extension(stack.extension()).unwrap();
    }
    Box::leak(
        file.encode(&EncodeOptions::new())
            .unwrap()
            .into_boxed_slice(),
    )
}

#[test]
fn only_used_palette_entries_need_binary_alpha() {
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::I2, ColorDescription::SRGB).unwrap();
    let palette = [[7, 8, 9, 255], [1, 2, 3, 0], [4, 5, 6, 72], [0; 4]];
    let indexed = |byte| {
        sprite_file(
            RawImageAsset::new(surface, &[&[byte]]).with_color_table(palette.as_flattened()),
            true,
        )
    };
    let asset = SpriteStackAsset::from_mirx(indexed(0b00_01_10_00)).unwrap();
    let pixels = asset.atlas().read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(0, 0).a, 255);
    assert_eq!(pixels.get_pixel(1, 0).a, 0);
    assert_eq!(
        SpriteStackAsset::from_mirx(indexed(0b00_10_00_00)).err(),
        Some(SpriteStackError::PartialAlpha {
            x: 1,
            y: 0,
            alpha: 72
        })
    );
}

#[test]
fn sprite_and_standard_image_borrow_the_same_atlas() {
    let bytes = file(&[22, 33, 44, 255, 0, 0, 0, 0], true);
    let asset = mirx::Reader::open(bytes).unwrap().sprite_stack().unwrap();
    let plain = Texture::from_mirx(bytes).unwrap();
    let sprite_pixels = asset.atlas().read_pixels().unwrap();
    assert_eq!(
        sprite_pixels.bytes().as_ptr(),
        plain.read_pixels().unwrap().bytes().as_ptr()
    );
    assert_eq!(asset.descriptor().layers().get(0).unwrap().x(), 1);
    assert_eq!(sprite_pixels.get_pixel(1, 0).a, 0);
    assert_eq!(sprite_pixels.get_pixel(0, 0).a, 255);
}

#[test]
fn missing_metadata_and_partial_alpha_have_specific_load_errors() {
    let bytes = file(&[0, 0, 0, 0, 0, 0, 0, 255], false);
    assert!(Texture::from_mirx(bytes).is_ok());
    assert_eq!(
        SpriteStackAsset::from_mirx(bytes).err(),
        Some(SpriteStackError::Placement(StackError::MissingDescriptor))
    );
    let bytes = file(&[0, 0, 0, 0, 0, 0, 0, 64], true);
    assert_eq!(
        SpriteStackAsset::from_mirx(bytes).err(),
        Some(SpriteStackError::PartialAlpha {
            x: 1,
            y: 0,
            alpha: 64
        })
    );
}
