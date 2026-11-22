use super::{INSTRUMENT_INKS, Ink, Inks, blend};
use alloc::{vec, vec::Vec};
use mirui::types::Fixed;

pub(super) struct Layer {
    pub(super) pixels: LayerPixels,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) pad: i32,
}
pub(super) const ROW_PIXELS: usize = 160;
pub(super) enum LayerPixels {
    Indexed(Vec<u8>),
    Encoded { data: Vec<u8>, rows: Vec<u32> },
}
pub(super) enum RowPixels<'a> {
    Indexed(&'a [u8], &'a Inks),
    Empty,
}
impl RowPixels<'_> {
    pub(super) fn color(&self, x: usize) -> u32 {
        match self {
            Self::Indexed(row, colors) => colors[usize::from(row[x])],
            Self::Empty => 0,
        }
    }
    pub(super) fn sample(&self, above: Option<&Self>, x: usize, fraction: Fixed) -> (u32, Fixed) {
        let mut color = self.color(x);
        let mut cover = Fixed::ONE;
        if let Some(above) = above {
            let above = above.color(x);
            if above != color {
                if above == 0 {
                    cover = Fixed::ONE - fraction;
                } else if color == 0 {
                    color = above;
                    cover = fraction;
                } else {
                    color = blend(color, above, fraction);
                }
            }
        }
        (color, cover)
    }
}
impl Layer {
    pub(super) fn new(width: i32, height: i32, pad: i32, compact: bool) -> Self {
        if compact {
            assert!((width + pad * 2) as usize <= ROW_PIXELS);
        }
        Self {
            pixels: LayerPixels::Indexed(Vec::new()),
            width: width + pad * 2,
            height: height + pad * 2,
            pad,
        }
    }
    pub(super) fn mark_coverage(
        &self,
        clip: [i32; 4],
        offset: [Fixed; 3],
        mask: &mut [u8],
        inks: &Inks,
        row_scratch: &mut [u8; ROW_PIXELS],
        above_scratch: &mut [u8; ROW_PIXELS],
    ) -> usize {
        let [left, top, right, bottom] = clip;
        let width = (right - left) as usize;
        let dx = offset[0].trunc_to_int() - self.pad;
        let dy = offset[1].trunc_to_int() - self.pad;
        let fraction = offset[2];
        let mut marked = 0;
        for y in top.max(dy)..bottom.min(dy + self.height) {
            let sy = (y - dy) as usize;
            let row = self.row(sy, row_scratch, inks);
            let above = (fraction > Fixed::from_ratio(4, 1000) && sy > 0)
                .then(|| self.row(sy - 1, above_scratch, inks));
            if matches!(row, RowPixels::Empty)
                && above
                    .as_ref()
                    .is_none_or(|row| matches!(row, RowPixels::Empty))
            {
                continue;
            }
            for x in left.max(dx)..right.min(dx + self.width) {
                let at = (y - top) as usize * width + (x - left) as usize;
                if mask[at] != 0 {
                    continue;
                }
                let (color, cover) = row.sample(above.as_ref(), (x - dx) as usize, fraction);
                if color >> 24 == 255 && cover == Fixed::ONE {
                    mask[at] = 1;
                    marked += 1;
                }
            }
        }
        marked
    }
    pub(super) fn bytes(&self) -> usize {
        match &self.pixels {
            LayerPixels::Indexed(data) => data.capacity(),
            LayerPixels::Encoded { data, rows } => data.capacity() + rows.capacity() * 4,
        }
    }
    pub(super) fn ink_at(&self, at: usize) -> u32 {
        match &self.pixels {
            LayerPixels::Indexed(data) => u32::from(data.get(at).copied().unwrap_or(0)),
            LayerPixels::Encoded { .. } => panic!("completed cockpit layer is immutable"),
        }
    }
    pub(super) fn set_ink_at(&mut self, at: usize, ink: u32) {
        let LayerPixels::Indexed(data) = &mut self.pixels else {
            panic!("completed cockpit layer is immutable")
        };
        if data.is_empty() {
            data.resize((self.width * self.height) as usize, 0);
        }
        assert!(ink < INSTRUMENT_INKS.len() as u32, "cockpit material index");
        data[at] = ink as u8;
    }
    pub(super) fn seal(&mut self, compact: bool) {
        let LayerPixels::Indexed(data) = &self.pixels else {
            return;
        };
        if !compact || data.is_empty() {
            return;
        }
        let codec = mirx::coding::Rle::new();
        let width = self.width as usize;
        let length = data
            .chunks_exact(width)
            .map(|row| {
                if row.iter().all(|v| *v == 0) {
                    0
                } else {
                    codec.encoded_len(row).expect("cockpit row size")
                }
            })
            .sum();
        let mut encoded = vec![0; length];
        let mut rows = Vec::with_capacity(self.height as usize + 1);
        let mut offset = 0;
        rows.push(0);
        for row in data.chunks_exact(width) {
            if row.iter().any(|v| *v != 0) {
                offset += codec
                    .encode_into(row, &mut encoded[offset..])
                    .expect("cockpit row encoding");
            }
            rows.push(u32::try_from(offset).expect("bounded cockpit layer"));
        }
        self.pixels = LayerPixels::Encoded {
            data: encoded,
            rows,
        };
    }
    pub(super) fn row<'a>(
        &'a self,
        y: usize,
        scratch: &'a mut [u8; ROW_PIXELS],
        inks: &'a Inks,
    ) -> RowPixels<'a> {
        let width = self.width as usize;
        if let LayerPixels::Encoded { data, rows, .. } = &self.pixels {
            let encoded = &data[rows[y] as usize..rows[y + 1] as usize];
            if !encoded.is_empty() {
                mirx::coding::Rle::new()
                    .plan(encoded, width)
                    .expect("prepared cockpit row")
                    .decode_into(&mut scratch[..width])
                    .expect("cockpit row workspace");
            }
        }
        self.row_view(y, scratch, inks)
    }
    // Encoded scratch must already contain this row; transparent rows do not read it.
    pub(super) fn row_view<'a>(
        &'a self,
        y: usize,
        scratch: &'a [u8; ROW_PIXELS],
        inks: &'a Inks,
    ) -> RowPixels<'a> {
        let width = self.width as usize;
        match &self.pixels {
            LayerPixels::Indexed(data) if !data.is_empty() => {
                RowPixels::Indexed(&data[y * width..(y + 1) * width], inks)
            }
            LayerPixels::Encoded { rows, .. } if rows[y] != rows[y + 1] => {
                RowPixels::Indexed(&scratch[..width], inks)
            }
            _ => RowPixels::Empty,
        }
    }
}
impl Ink for Layer {
    fn clip(&self) -> [i32; 4] {
        [
            -self.pad,
            -self.pad,
            self.width - self.pad,
            self.height - self.pad,
        ]
    }
    fn put(&mut self, x: i32, y: i32, color: u32) {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.set_ink_at((y * self.width + x) as usize, color);
        }
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.ink_at((y * self.width + x) as usize)
        } else {
            0
        }
    }
}
