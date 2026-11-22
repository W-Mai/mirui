use super::super::gradient::radial::tests::with_plan;
use super::*;
use crate::types::Color;

#[test]
fn cropped_indexed_layers_keep_bit_offsets_palette_alpha_and_cached_uploads() {
    use mirx::{image::ColorFormat::I2, palette::ColorTableView};
    use sdl2::{pixels::PixelFormatEnum, render::BlendMode};

    let canvas = sdl2::surface::Surface::new(3, 2, PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    let device = Rc::new(Device::new(canvas, creator).unwrap());
    let mut cache = TextureCache::new(device.clone(), 4096);
    let bytes = [0b00_01_10_11, 0b01_00_00_00, 0b11_10_01_00, 0b10_00_00_00];
    let a = [
        [1, 2, 3, 0],
        [5, 6, 7, 255],
        [9, 10, 11, 128],
        [13, 14, 15, 255],
    ];
    let mut b = a;
    b[1] = [47, 53, 59, 42];
    let mut read = |x, palette: &[[u8; 4]; 4]| {
        let table = ColorTableView::from_rgba_bytes(palette.as_flattened()).unwrap();
        let atlas = Texture::from_indexed(&bytes, 5, 2, I2, table).unwrap();
        let slice = atlas
            .pixels_in(Region::new(x, 0, 3, 2).unwrap())
            .unwrap()
            .unwrap();
        let pixels = slice.pixels().unwrap();
        let texture = pixels.as_texture().with_cache_revision(19);
        cache
            .with_texture(&texture, CompositeMode::SourceOver, None, |source, _| {
                source.set_blend_mode(BlendMode::None);
                let mut canvas = device.canvas().unwrap();
                canvas.copy(source, None, None).unwrap();
                canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap()
            })
            .unwrap()
    };
    assert_eq!(
        read(1, &a),
        [a[1], a[2], a[3], a[2], a[1], a[0]].as_flattened()
    );
    assert_eq!(
        read(1, &a),
        [a[1], a[2], a[3], a[2], a[1], a[0]].as_flattened()
    );
    assert_eq!(
        read(0, &a),
        [a[0], a[1], a[2], a[3], a[2], a[1]].as_flattened()
    );
    assert_eq!(
        read(1, &b),
        [b[1], b[2], b[3], b[2], b[1], b[0]].as_flattened()
    );
    assert_eq!(cache.pool.stats().hit_count, 1);
}

#[test]
fn strided_sprite_layers_reuse_completed_uploads_and_distinguish_revisions() {
    use crate::render::texture::{AlphaMode, ColorFormat};
    use sdl2::pixels::PixelFormatEnum;
    let canvas = sdl2::surface::Surface::new(1, 2, PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    let device = Rc::new(Device::new(canvas, creator).unwrap());
    let mut cache = TextureCache::new(device.clone(), 4096);
    let mut bytes = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255];
    let mut read = |bytes: &[u8; 16], x, revision| {
        let parent = Texture::from_ref(bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        let slice = parent
            .pixels_in(Region::new(x, 0, 1, 2).unwrap())
            .unwrap()
            .unwrap();
        let pixels = slice.pixels().unwrap();
        let mut texture = pixels.as_texture().with_cache_revision(revision);
        texture.alpha_mode = AlphaMode::Blend;
        cache
            .with_texture(&texture, CompositeMode::SourceOver, None, |source, _| {
                let mut canvas = device.canvas().unwrap();
                canvas.copy(source, None, None).unwrap();
                canvas.read_pixels(None, PixelFormatEnum::RGBA32).unwrap()
            })
            .unwrap()
    };
    assert_eq!(read(&bytes, 1, 3), [4, 5, 6, 255, 10, 11, 12, 255]);
    assert_eq!(read(&bytes, 0, 1), [1, 2, 3, 255, 7, 8, 9, 255]);
    bytes[0] = 122;
    // Deliberately retain the old test revision. A second upload would
    // expose the changed byte, while a cache hit preserves the original.
    assert_eq!(read(&bytes, 0, 1)[0], 1);
    assert_eq!(read(&bytes, 0, 2)[0], 122);
    assert_eq!(cache.pool.stats().hit_count, 1);
}

#[test]
fn radial_entries_share_image_budget_own_palette_values_and_reuse_after_source_drop() {
    let canvas = sdl2::surface::Surface::new(16, 16, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    let device = Rc::new(Device::new(canvas, creator).unwrap());
    let color = Color::rgba(0, 0, 0, 0);
    let bytes = with_plan(color, |plan| {
        plan.key.bytes() + core::mem::size_of::<CacheKey>() + core::mem::size_of::<UploadCoverage>()
    });
    let mut cache = TextureCache::new(device.clone(), bytes);
    with_plan(color, |plan| cache.with_radial(plan, |_| ()).unwrap());
    assert_eq!(cache.pool.len(), 1);
    with_plan(color, |plan| {
        assert_eq!(
            crate::test_support::tracked_allocations(|| cache.with_radial(plan, |_| ()).unwrap()),
            0
        );
    });
    assert_eq!(cache.pool.stats().hit_count, 1);
    let old = with_plan(color, |plan| plan.key.clone());
    with_plan(Color::rgba(9, 8, 7, 0), |plan| {
        assert_ne!(old, plan.key);
        cache.with_radial(plan, |_| ()).unwrap();
    });
    assert_eq!(cache.pool.len(), 1);
    assert_eq!(cache.pool.stats().evict_count, 1);
    let short = TextureCache::new(device.clone(), bytes - 1);
    with_plan(color, |plan| assert!(!short.radial_fits(&plan.key)));
    let pixels = [7_u8; 4 * 4 * 4];
    let texture = Texture::from_ref(&pixels, 4, 4, crate::render::texture::ColorFormat::RGBA8888)
        .unwrap()
        .with_transient(false);
    cache
        .with_texture(&texture, CompositeMode::SourceOver, None, |_, _| ())
        .unwrap();
    assert_eq!(cache.pool.len(), 1);
    assert_eq!(cache.pool.stats().evict_count, 2);
    assert!(cache.matches(&device));
    device.invalidate();
    assert!(!cache.matches(&device));
    std::println!(
        "gradient_key_bytes={} image_key_bytes={} batch_bytes={}",
        core::mem::size_of::<CacheKey>(),
        core::mem::size_of::<TextureKey>(),
        core::mem::size_of::<sdl2_sys::SDL_Vertex>() * super::super::gradient::BATCH_VERTICES
    );
}
