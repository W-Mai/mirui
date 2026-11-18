use super::*;
use mirx::document::{EncodeOptions, LayoutPolicy};
use mirx::image::{
    ColorDescription, ColorFormat as SampleFormat, EncodedImageAsset, ImageAsset, SampleLayout,
    SurfaceDescriptor,
};

const WIDTH: usize = 32;
const HEIGHT: usize = 5;

fn color(index: u8) -> [u8; 4] {
    [index.wrapping_mul(13), !index, index, index.wrapping_add(1)]
}

fn index(x: usize, y: usize, bits: u8) -> u8 {
    ((x * 31 + y * 47) & ((1 << bits) - 1)) as u8
}

fn fixture(format: SampleFormat, chunk: bool, encoded: bool) -> Rc<Vec<u8>> {
    let bits = format.indexed_bit_depth().unwrap();
    let colors: Vec<_> = (0..1u16 << bits).flat_map(|i| color(i as u8)).collect();
    let stride = format.minimum_stride(WIDTH as u32).unwrap() as usize + usize::from(!encoded) * 3;
    let mut samples = vec![0; stride * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let offset = x * usize::from(bits);
            let shift = 8 - usize::from(bits) - offset % 8;
            samples[y * stride + offset / 8] |= index(x, y, bits) << shift;
        }
    }
    if encoded {
        let surface = SurfaceDescriptor::new(
            WIDTH as u32,
            HEIGHT as u32,
            SampleLayout::from_color_format(format),
            ColorDescription::SRGB,
        )
        .unwrap();
        let codec = mirx::coding::Rle::new();
        let mut bytes = vec![0; 2 * samples.len() + 32];
        let len = codec.encode_into(&samples, &mut bytes).unwrap();
        let image = EncodedImageAsset::new(surface, codec.record(), &bytes[..len])
            .with_color_table(&colors);
        let mut doc = mirx::Document::new();
        let primary = doc.push_encoded_image(&image).unwrap();
        doc.set_primary(primary).unwrap();
        return Rc::new(doc.finish().unwrap().into_owned());
    }
    let doc = mirx::Document::new_flat(
        ImageAsset::new(
            WIDTH as u32,
            HEIGHT as u32,
            format,
            stride as u32,
            samples.into(),
        )
        .with_extra(colors.into()),
    )
    .unwrap();
    let policy = if chunk {
        LayoutPolicy::ForceChunk
    } else {
        LayoutPolicy::ForceFlat
    };
    Rc::new(
        doc.encode(&EncodeOptions::new().with_layout_policy(policy))
            .unwrap(),
    )
}

#[test]
fn indexed_sources_borrow_or_prepare_packed_rows_and_the_original_palette() {
    use SampleFormat::{I1, I2, I4, I8};
    for format in [I1, I2, I4, I8] {
        let bits = format.indexed_bit_depth().unwrap();
        let mut direct =
            Texture::owned(WIDTH as u16, HEIGHT as u16, ColorFormat::RGBA8888).unwrap();
        {
            let mut pixels = direct.pixels_mut().unwrap();
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let [r, g, b, a] = color(index(x, y, bits));
                    pixels.set_pixel(x as i32, y as i32, &Color::rgba(r, g, b, a));
                }
            }
        }
        for (chunk, encoded) in [(false, false), (true, false), (true, true)] {
            let bytes = fixture(format, chunk, encoded);
            for mapped in [false, true] {
                let control = Rc::new(Control::default());
                control.complete();
                let app = if mapped {
                    let mut app = super::super::mapped::app(bytes.clone(), control.clone(), true);
                    app.tick();
                    app
                } else {
                    source_app(bytes.clone(), control.clone(), 16 * 1024, false)
                };
                let texture = resolve(&app, "cover");
                drop(app);
                let area = Region::new(9, 1, 9, 3).unwrap();
                let view = region(&texture, area).unwrap();
                let pixels = view.pixels().unwrap();
                assert_eq!(pixels.sample_format(), format);
                assert_eq!(pixels.bit_offset(), (9 * bits) % 8);
                assert_eq!(pixels.indexed_color_table().unwrap().len(), 1 << bits);
                for y in area.y()..area.bottom() {
                    for x in area.x()..area.right() {
                        let [r, g, b, a] = color(index(x as usize, y as usize, bits));
                        assert_eq!(
                            view.get_pixel(x, y),
                            Some(Color::rgba(r, g, b, a)),
                            "{format:?} mapped={mapped}, chunk={chunk}, encoded={encoded}, ({x},{y})"
                        );
                    }
                }
                let reads = control.reads.get();
                let inner = texture
                    .pixels_in(Region::new(11, 2, 3, 1).unwrap())
                    .unwrap()
                    .unwrap();
                assert_eq!(inner.get_pixel(12, 2), view.get_pixel(12, 2));
                assert_eq!(control.reads.get(), reads);
                if mapped && !encoded {
                    let range = bytes.as_ptr() as usize..bytes.as_ptr() as usize + bytes.len();
                    assert!(range.contains(&(pixels.bytes().as_ptr() as usize)));
                    assert!(range.contains(
                        &(pixels.indexed_color_table().unwrap().as_bytes().as_ptr() as usize)
                    ));
                }
                super::super::mapped::compare_region_routes(
                    &texture,
                    &direct,
                    mirui::types::Rect::new(9, 6, 9, 18),
                );
            }
        }
    }
}

#[test]
fn pending_palette_and_packed_regions_share_the_source_decode_budget() {
    let height = 512;
    let data = vec![0x55; 4 * height];
    let palette = [255, 127, 0, 0, 32, 192, 64, 128];
    let image = ImageAsset::new(
        WIDTH as u32,
        height as u32,
        SampleFormat::I1,
        4,
        data.into(),
    )
    .with_extra(Cow::Borrowed(&palette));
    let source = Rc::new(
        mirx::Document::new_flat(image)
            .unwrap()
            .finish()
            .unwrap()
            .into_owned(),
    );
    let control = Rc::new(Control::default());
    // The full packed plane takes 2 KiB. The same pixels expanded to RGBA take 64 KiB.
    let app = source_app(source, control.clone(), 8 * 1024, true);
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..32 {
        if images.probe("indexed").is_some() {
            break;
        }
        images.poll_pending(&mut cx);
    }
    assert_eq!(images.probe("indexed").unwrap().width, WIDTH as u16);
    for _ in 0..32 {
        assert_ne!(images.resolve("indexed").width(), WIDTH as u16);
        images.poll_pending(&mut cx);
    }
    control.complete();
    let texture = resolve(&app, "indexed");
    let full = region(
        &texture,
        Region::new(0, 0, WIDTH as u32, height as u32).unwrap(),
    )
    .unwrap();
    let pixels = full.pixels().unwrap();
    assert_eq!(pixels.bytes().len(), 4 * height);
    assert_eq!(pixels.get_pixel(31, 511), Color::rgba(32, 192, 64, 128));
    assert_eq!(pixels.get_pixel(30, 511), Color::rgba(255, 127, 0, 0));
    assert_eq!(control.opens.get(), 1);
}
