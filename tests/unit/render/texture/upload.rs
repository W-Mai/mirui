use super::*;
use crate::render::texture::{Region, Texture};
use mirx::{image::ColorFormat as Sample, palette::ColorTableView};

#[test]
fn indexed_upload_expands_only_the_cropped_rows_at_each_source_depth() {
    for format in [Sample::I1, Sample::I2, Sample::I4, Sample::I8] {
        let depth = usize::from(format.indexed_bit_depth().unwrap());
        let max = (1usize << depth) - 1;
        let stride = format.minimum_stride(11).unwrap() as usize + 2;
        let palette: Vec<_> = (0..=max)
            .flat_map(|i| [i as u8, 22, 65, (255 - i) as u8])
            .collect();
        let color_table = ColorTableView::from_rgba_bytes(&palette).unwrap();
        let mut bytes = alloc::vec![0; 3 * stride];
        let index = |x: usize, y: usize| max - (x + y * 7) % (max + 1);
        for y in 0..3 {
            for x in 0..11 {
                let bit = x * depth;
                bytes[y * stride + bit / 8] |= (index(x, y) << (8 - depth - bit % 8)) as u8;
            }
        }
        let src = Texture::from_indexed(&bytes, 11, 3, format, color_table)
            .unwrap()
            .with_stride(stride)
            .unwrap();
        let crop = src
            .pixels_in(Region::new(1, 1, 7, 2).unwrap())
            .unwrap()
            .unwrap();
        let view = crop.pixels().unwrap();
        let upload = view.rgba8_upload_pixels().unwrap();
        let mut expected = Vec::new();
        for y in 1..3 {
            for x in 1..8 {
                let at = 4 * index(x, y);
                expected.extend_from_slice(&palette[at..at + 4]);
            }
        }
        assert_eq!(upload.as_ref(), expected, "{format:?}");
    }
}
