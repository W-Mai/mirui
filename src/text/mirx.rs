//! Borrowed MIRX FONT access for the renderer-neutral textflow pipeline.

use textflow::shaping::{
    FlowPoint, FontAccessError, FontId, FontMetrics, GlyphId, GlyphSource, SimpleTypeface,
};

#[derive(Clone, Copy, Debug)]
pub struct MirxGlyphSource<'a> {
    id: FontId,
    face: ::mirx::font::FontView<'a>,
}

impl<'a> MirxGlyphSource<'a> {
    pub const fn new(id: FontId, face: ::mirx::font::FontView<'a>) -> Self {
        Self { id, face }
    }

    pub const fn id(self) -> FontId {
        self.id
    }

    pub const fn view(self) -> ::mirx::font::FontView<'a> {
        self.face
    }

    pub fn typeface(&self) -> SimpleTypeface<'_, Self> {
        SimpleTypeface::new(self)
    }

    pub const fn shaping_data(self) -> Option<::mirx::font::ShapingData<'a>> {
        self.face.shaping_data()
    }
}

impl GlyphSource for MirxGlyphSource<'_> {
    fn id(&self) -> FontId {
        self.id
    }

    fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
        let face = self.face.face();
        Ok(FontMetrics {
            units_per_em: face.units_per_em(),
            ascender: fixed_raw(face.ascender()),
            descender: fixed_raw(face.descender()),
            line_gap: fixed_raw(face.line_gap()),
        })
    }

    fn glyph_for(&self, character: char) -> Result<Option<GlyphId>, FontAccessError> {
        Ok(self
            .face
            .map_char(character)
            .map(|glyph| GlyphId::new(glyph.get())))
    }

    fn glyph_advance(&self, glyph: GlyphId) -> Result<FlowPoint, FontAccessError> {
        let glyph = ::mirx::font::GlyphId::new(glyph.value());
        let x = match self.face.advance(glyph) {
            Some(value) => fixed_raw(value),
            None => {
                let shaping = self.face.shaping_data().ok_or(FontAccessError::Malformed)?;
                i32::from(horizontal_advance(shaping, glyph.get())?) << 8
            }
        };
        Ok(FlowPoint { x, y: 0 })
    }

    fn notdef_glyph(&self) -> Result<Option<GlyphId>, FontAccessError> {
        Ok(Some(GlyphId::new(self.face.face().default_glyph().get())))
    }

    fn kerning(&self, left: GlyphId, right: GlyphId) -> Result<i32, FontAccessError> {
        let Some(shaping) = self.face.shaping_data() else {
            return Ok(0);
        };
        legacy_kerning(shaping, left.value(), right.value()).map(|value| i32::from(value) << 8)
    }
}

fn fixed_raw(value: ::mirx::types::Fixed) -> i32 {
    i32::from_le_bytes(value.to_le_bytes())
}

fn horizontal_advance(
    shaping: ::mirx::font::ShapingData<'_>,
    glyph: u16,
) -> Result<u16, FontAccessError> {
    let hhea = shaping.table(*b"hhea").ok_or(FontAccessError::Malformed)?;
    let hmtx = shaping.table(*b"hmtx").ok_or(FontAccessError::Malformed)?;
    let maxp = shaping.table(*b"maxp").ok_or(FontAccessError::Malformed)?;
    horizontal_advance_tables(hhea, hmtx, maxp, glyph)
}

fn horizontal_advance_tables(
    hhea: &[u8],
    hmtx: &[u8],
    maxp: &[u8],
    glyph: u16,
) -> Result<u16, FontAccessError> {
    let metric_count = read_u16(hhea, 34).ok_or(FontAccessError::Malformed)?;
    let glyph_count = read_u16(maxp, 4).ok_or(FontAccessError::Malformed)?;
    if metric_count == 0 || metric_count > glyph_count || glyph >= glyph_count {
        return Err(FontAccessError::Malformed);
    }
    let required = usize::from(metric_count)
        .checked_mul(4)
        .and_then(|metrics| {
            usize::from(glyph_count - metric_count)
                .checked_mul(2)
                .and_then(|bearings| metrics.checked_add(bearings))
        })
        .ok_or(FontAccessError::Malformed)?;
    if hmtx.len() < required {
        return Err(FontAccessError::Malformed);
    }
    let metric = usize::from(glyph.min(metric_count - 1));
    read_u16(hmtx, metric * 4).ok_or(FontAccessError::Malformed)
}

fn legacy_kerning(
    shaping: ::mirx::font::ShapingData<'_>,
    left: u16,
    right: u16,
) -> Result<i16, FontAccessError> {
    let Some(table) = shaping.table(*b"kern") else {
        return Ok(0);
    };
    legacy_kerning_table(table, left, right)
}

fn legacy_kerning_table(table: &[u8], left: u16, right: u16) -> Result<i16, FontAccessError> {
    if read_u16(table, 0) != Some(0) {
        return Ok(0);
    }
    let count = read_u16(table, 2).ok_or(FontAccessError::Malformed)?;
    let key = u32::from(left) << 16 | u32::from(right);
    let mut offset = 4usize;
    let mut value = 0i32;
    for _ in 0..count {
        let length = usize::from(read_u16(table, offset + 2).ok_or(FontAccessError::Malformed)?);
        let coverage = read_u16(table, offset + 4).ok_or(FontAccessError::Malformed)?;
        if length < 6
            || offset
                .checked_add(length)
                .is_none_or(|end| end > table.len())
        {
            return Err(FontAccessError::Malformed);
        }
        let format = coverage >> 8;
        let horizontal = coverage & 1 != 0;
        let cross_stream = coverage & 4 != 0;
        if format == 0 && horizontal && !cross_stream {
            let delta = format_zero_pair(&table[offset + 6..offset + length], key)?;
            if coverage & 8 != 0 {
                value = i32::from(delta);
            } else {
                value = value
                    .checked_add(i32::from(delta))
                    .ok_or(FontAccessError::Malformed)?;
            }
        }
        offset += length;
    }
    i16::try_from(value).map_err(|_| FontAccessError::Malformed)
}

fn format_zero_pair(bytes: &[u8], key: u32) -> Result<i16, FontAccessError> {
    let count = usize::from(read_u16(bytes, 0).ok_or(FontAccessError::Malformed)?);
    let records = bytes.get(8..).ok_or(FontAccessError::Malformed)?;
    if count.checked_mul(6).is_none_or(|size| size > records.len()) {
        return Err(FontAccessError::Malformed);
    }
    let mut start = 0usize;
    let mut end = count;
    while start < end {
        let middle = start + (end - start) / 2;
        let offset = middle * 6;
        let current = read_u32(records, offset).ok_or(FontAccessError::Malformed)?;
        match current.cmp(&key) {
            core::cmp::Ordering::Less => start = middle + 1,
            core::cmp::Ordering::Greater => end = middle,
            core::cmp::Ordering::Equal => {
                return read_i16(records, offset + 4).ok_or(FontAccessError::Malformed);
            }
        }
    }
    Ok(0)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use textflow::shaping::{ShapeRequest, Typeface};
    use textflow::{bidi::Direction, unicode::Script};

    const FONT: &[u8] = include_bytes!("../gallery/demos/assets/misans_sdf_24.mirx");

    fn source() -> MirxGlyphSource<'static> {
        let reader = ::mirx::Reader::open(FONT).unwrap();
        let chunk = reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == ::mirx::ChunkType::FONT)
            .unwrap();
        let face = chunk
            .font(&::mirx::reader::PayloadLimits::HOST)
            .unwrap()
            .unwrap();
        MirxGlyphSource::new(FontId::new(17), face)
    }

    #[test]
    fn exposes_stable_identity_metrics_and_cmap() {
        let source = source();
        let metrics = source.metrics().unwrap();
        assert_eq!(source.id(), FontId::new(17));
        assert!(metrics.units_per_em > 0);
        assert!(metrics.ascender > 0);
        assert!(metrics.descender < 0);
        assert!(source.glyph_for('A').unwrap().is_some());
        assert!(source.glyph_for('\u{10ffff}').unwrap().is_none());
    }

    #[test]
    fn reads_shaping_backed_advances_without_an_owned_font() {
        let mut hhea = [0; 36];
        hhea[34..36].copy_from_slice(&2_u16.to_be_bytes());
        let mut maxp = [0; 6];
        maxp[4..6].copy_from_slice(&3_u16.to_be_bytes());
        let hmtx = [0x01, 0x2c, 0, 0, 0x02, 0x58, 0, 0, 0, 0];
        assert_eq!(horizontal_advance_tables(&hhea, &hmtx, &maxp, 0), Ok(300));
        assert_eq!(horizontal_advance_tables(&hhea, &hmtx, &maxp, 1), Ok(600));
        assert_eq!(horizontal_advance_tables(&hhea, &hmtx, &maxp, 2), Ok(600));
        assert_eq!(
            horizontal_advance_tables(&hhea, &hmtx[..9], &maxp, 2),
            Err(FontAccessError::Malformed)
        );
    }

    #[test]
    fn reads_horizontal_format_zero_kerning() {
        let table = [
            0, 0, 0, 1, 0, 0, 0, 20, 0, 1, 0, 1, 0, 6, 0, 0, 0, 0, 0, 4, 0, 7, 0xff, 0xec,
        ];
        assert_eq!(legacy_kerning_table(&table, 4, 7), Ok(-20));
        assert_eq!(legacy_kerning_table(&table, 4, 8), Ok(0));
    }

    #[test]
    fn shapes_mirx_cmap_and_hmtx_into_caller_storage() {
        let source = source();
        let typeface = source.typeface();
        let request = ShapeRequest::new("AV", 0..2, Direction::LeftToRight, Script::Latin);
        let mut glyphs = [textflow::shaping::ShapedGlyph::default(); 2];
        assert_eq!(typeface.shape_into(&request, &mut glyphs).unwrap(), 2);
        assert_eq!(
            glyphs[0].glyph_id(),
            source.glyph_for('A').unwrap().unwrap()
        );
        assert_eq!(
            glyphs[1].glyph_id(),
            source.glyph_for('V').unwrap().unwrap()
        );
        assert!(glyphs.iter().all(|glyph| glyph.advance.x > 0));
    }
}
