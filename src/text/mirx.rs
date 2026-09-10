//! Borrowed MIRX FONT access for the renderer-neutral textflow pipeline.

use textflow::shaping::{
    FlowPoint, FontAccessError, FontId, FontMetrics, GlyphId, GlyphSource, ScriptProvider,
    ScriptTypeface, ShapeError, ShapeRequest, ShapedGlyph, SimpleTypeface, Typeface,
};
use textflow::unicode::Script;

#[derive(Clone, Copy, Debug)]
/// Borrowed MIRX face data exposed through textflow's scalar font contract.
pub struct MirxGlyphSource<'a> {
    id: FontId,
    face: ::mirx::font::FontView<'a>,
}

impl<'a> MirxGlyphSource<'a> {
    /// Binds a stable application font identity to a validated MIRX FONT view.
    pub const fn new(id: FontId, face: ::mirx::font::FontView<'a>) -> Self {
        Self { id, face }
    }

    /// Returns the stable font identity used by shaped runs and caches.
    pub const fn id(self) -> FontId {
        self.id
    }

    /// Returns the underlying validated MIRX FONT view.
    pub const fn view(self) -> ::mirx::font::FontView<'a> {
        self.face
    }

    /// Creates a borrowed typeface whose output coordinates use Q24.8 pixels at `ppem`.
    pub const fn typeface(&self, ppem: u16) -> MirxTypeface<'_, 'a> {
        MirxTypeface { source: self, ppem }
    }

    const fn shaping_data(self) -> Option<::mirx::font::ShapingData<'a>> {
        self.face.shaping_data()
    }
}

#[derive(Clone, Copy, Debug)]
/// A borrowed textflow typeface backed by one MIRX glyph source.
pub struct MirxTypeface<'source, 'font> {
    source: &'source MirxGlyphSource<'font>,
    ppem: u16,
}

impl MirxTypeface<'_, '_> {
    /// Returns the glyph source backing this typeface.
    pub const fn source(&self) -> &MirxGlyphSource<'_> {
        self.source
    }

    /// Returns the pixel size used to normalize face-unit metrics and positions.
    pub const fn ppem(&self) -> u16 {
        self.ppem
    }

    fn scale(&self, value: i32) -> Result<i32, FontAccessError> {
        crate::types::fixed::checked_scale_q24_8(
            value,
            self.ppem,
            self.source.face.face().units_per_em(),
        )
        .ok_or(FontAccessError::Malformed)
    }
}

impl Typeface for MirxTypeface<'_, '_> {
    fn id(&self) -> FontId {
        self.source.id
    }

    fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
        let metrics = self.source.metrics()?;
        Ok(FontMetrics {
            units_per_em: metrics.units_per_em,
            ascender: self.scale(metrics.ascender)?,
            descender: self.scale(metrics.descender)?,
            line_gap: self.scale(metrics.line_gap)?,
        })
    }

    fn covers(&self, grapheme: &str) -> Result<bool, FontAccessError> {
        if grapheme.is_empty() {
            return Ok(false);
        }
        for character in grapheme.chars() {
            if self.source.glyph_for(character)?.is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn supports_complex_shaping(&self) -> bool {
        self.source.shaping_data().is_some()
    }

    fn shape_into(
        &self,
        request: &ShapeRequest<'_>,
        output: &mut [ShapedGlyph],
    ) -> Result<usize, ShapeError> {
        let count = match self.source.shaping_data() {
            Some(shaping)
                if matches!(
                    request.script,
                    Script::Arabic | Script::Devanagari | Script::Thai
                ) =>
            {
                let shaping = crate::text::opentype::OpenTypeShaping::new(shaping);
                let scripts: [&dyn ScriptProvider; 3] = [
                    &textflow::scripts::ARABIC,
                    &textflow::scripts::DEVANAGARI,
                    &textflow::scripts::THAI,
                ];
                ScriptTypeface::new(self.source, &shaping)
                    .with_scripts(&scripts)
                    .shape_into(request, output)
            }
            Some(shaping) => crate::text::opentype::shape(self.source, shaping, request, output),
            None => SimpleTypeface::new(self.source).shape_into(request, output),
        }?;
        for glyph in &mut output[..count] {
            glyph.advance.x = self.scale(glyph.advance.x)?;
            glyph.advance.y = self.scale(glyph.advance.y)?;
            glyph.offset.x = self.scale(glyph.offset.x)?;
            glyph.offset.y = self.scale(glyph.offset.y)?;
        }
        Ok(count)
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
    use textflow::shaping::{ShapeRequest, TextRange, Typeface};
    use textflow::{bidi::Direction, unicode::Script};

    const FONT: &[u8] = include_bytes!("../gallery/demos/assets/misans_ui.mirx");
    const ARABIC_FONT: &[u8] = include_bytes!("../gallery/demos/assets/typography_arabic.mirx");
    const DEVANAGARI_FONT: &[u8] =
        include_bytes!("../gallery/demos/assets/typography_devanagari.mirx");
    const THAI_FONT: &[u8] = include_bytes!("../gallery/demos/assets/typography_thai.mirx");

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

    fn source_from(bytes: &'static [u8], id: u64) -> MirxGlyphSource<'static> {
        let reader = ::mirx::Reader::open(bytes).unwrap();
        let chunk = reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == ::mirx::ChunkType::FONT)
            .unwrap();
        let face = chunk
            .font(&::mirx::reader::PayloadLimits::HOST)
            .unwrap()
            .unwrap();
        MirxGlyphSource::new(FontId::new(id), face)
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
    fn covers_every_scalar_in_a_grapheme() {
        let source = source_from(ARABIC_FONT, 18);
        let typeface = source.typeface(48);

        assert!(typeface.covers("مَ").unwrap());
        assert!(!typeface.covers("م\u{10ffff}").unwrap());
    }

    #[test]
    fn typeface_normalizes_face_metrics_to_ppem() {
        let source = source();
        let raw = source.metrics().unwrap();
        let scaled = source.typeface(24).metrics().unwrap();
        let units_per_em = i32::from(raw.units_per_em);
        assert_eq!(scaled.units_per_em, raw.units_per_em);
        assert_eq!(scaled.ascender, raw.ascender * 24 / units_per_em);
        assert_eq!(scaled.descender, raw.descender * 24 / units_per_em);
        assert_eq!(scaled.line_gap, raw.line_gap * 24 / units_per_em);
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
    fn shapes_mirx_cmap_hmtx_and_pair_positioning_into_caller_storage() {
        let source = source();
        let typeface = source.typeface(24);
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
        let source_advance = source
            .glyph_advance(source.glyph_for('A').unwrap().unwrap())
            .unwrap()
            .x;
        let units_per_em = i32::from(source.metrics().unwrap().units_per_em);
        assert!(glyphs[0].advance.x < source_advance * 24 / units_per_em);
        assert_eq!(typeface.ppem(), 24);
        assert!(glyphs.iter().all(|glyph| glyph.advance.x > 0));
    }

    #[test]
    fn shapes_arabic_forms_marks_and_rtl_order_from_mirx() {
        let source = source_from(ARABIC_FONT, 18);
        let typeface = source.typeface(48);
        let text = "مَرْحَبًا بِالْعَالَمِ";
        let request =
            ShapeRequest::new(text, 0..text.len(), Direction::RightToLeft, Script::Arabic);
        let mut glyphs = [textflow::shaping::ShapedGlyph::default(); 32];
        let count = typeface.shape_into(&request, &mut glyphs).unwrap();

        assert_eq!(count, 22);
        assert_eq!(glyphs[0].glyph_id(), GlyphId::new(46));
        assert_eq!(glyphs[0].advance.x, 0);
        assert_eq!(glyphs[0].offset, FlowPoint { x: 3219, y: -233 });
        assert_eq!(glyphs[0].cluster, glyphs[1].cluster);
        assert_eq!(glyphs[14].glyph_id(), GlyphId::new(44));
        assert_eq!(glyphs[14].advance.x, 0);
        assert_eq!(glyphs[14].offset, FlowPoint { x: -86, y: -4042 });
        assert_eq!(glyphs[18].offset, FlowPoint { x: -135, y: -3944 });
        assert_eq!(glyphs[19].offset, FlowPoint { x: -737, y: 0 });
        assert_eq!(glyphs[21].glyph_id(), GlyphId::new(29));
        assert!(
            glyphs[..count]
                .iter()
                .filter(|glyph| matches!(glyph.glyph_id().value(), 44..=47))
                .all(|glyph| glyph.advance.x == 0 && glyph.unsafe_to_break())
        );
    }

    #[test]
    fn shapes_thai_marks_and_decomposition_from_mirx() {
        let source = source_from(THAI_FONT, 19);
        let typeface = source.typeface(48);
        let text = "สวัสดีครับ ภาษาไทย กำลังทดสอบ";
        let request = ShapeRequest::new(text, 0..text.len(), Direction::LeftToRight, Script::Thai)
            .with_language("th");
        let mut glyphs = [textflow::shaping::ShapedGlyph::default(); 48];
        let count = typeface.shape_into(&request, &mut glyphs).unwrap();

        assert_eq!(count, 30);
        assert_eq!(glyphs[0].glyph_id(), GlyphId::new(19));
        assert_eq!(glyphs[0].advance.x, 7028);
        assert_eq!(glyphs[2].glyph_id(), GlyphId::new(6));
        assert_eq!(glyphs[2].offset, FlowPoint { x: 122, y: 0 });
        assert_eq!(glyphs[8].offset, FlowPoint { x: 602, y: 0 });
        assert_eq!(glyphs[20].glyph_id(), GlyphId::new(10));
        assert_eq!(glyphs[20].offset, FlowPoint { x: -24, y: 0 });
        assert_eq!(glyphs[20].cluster, glyphs[21].cluster);
        assert_eq!(glyphs[23].offset, FlowPoint { x: -86, y: 0 });
        assert!(
            [2, 5, 8, 20, 23]
                .into_iter()
                .all(|index| glyphs[index].advance.x == 0 && glyphs[index].unsafe_to_break())
        );
    }

    #[test]
    fn shapes_thai_stacked_marks_from_mirx() {
        let source = source_from(THAI_FONT, 19);
        let typeface = source.typeface(48);
        let text = "ตั้ง";
        let request = ShapeRequest::new(text, 0..text.len(), Direction::LeftToRight, Script::Thai)
            .with_language("th");
        let mut glyphs = [textflow::shaping::ShapedGlyph::default(); 8];
        let count = typeface.shape_into(&request, &mut glyphs).unwrap();

        assert_eq!(count, 4);
        assert_eq!(
            glyphs[..count]
                .iter()
                .map(|glyph| glyph.glyph_id().value())
                .collect::<Vec<_>>(),
            [22, 6, 8, 9]
        );
        assert_eq!(glyphs[0].advance.x, 7815);
        assert_eq!(glyphs[1].advance.x, 0);
        assert_eq!(glyphs[1].offset, FlowPoint { x: -49, y: 0 });
        assert_eq!(glyphs[2].advance.x, 0);
        assert_eq!(glyphs[2].offset, FlowPoint { x: 319, y: -700 });
        assert_eq!(glyphs[3].advance.x, 6598);
        assert!(
            glyphs[..3]
                .iter()
                .all(|glyph| { glyph.cluster == TextRange::new(0, 9) && glyph.unsafe_to_break() })
        );
        assert_eq!(glyphs[3].cluster, TextRange::new(9, 12));
        assert!(!glyphs[3].unsafe_to_break());
    }

    #[test]
    fn shapes_devanagari_matra_and_conjuncts_from_mirx() {
        let source = source_from(DEVANAGARI_FONT, 20);
        let typeface = source.typeface(36);
        let text = "किरण क्षत्रिय";
        let request = ShapeRequest::new(
            text,
            0..text.len(),
            Direction::LeftToRight,
            Script::Devanagari,
        )
        .with_language("hi");
        let mut glyphs = [textflow::shaping::ShapedGlyph::default(); 24];
        let count = typeface.shape_into(&request, &mut glyphs).unwrap();

        assert_eq!(count, 9);
        assert_eq!(
            glyphs[..count]
                .iter()
                .map(|glyph| glyph.glyph_id().value())
                .collect::<Vec<_>>(),
            [38, 3, 7, 4, 1, 9, 38, 19, 6]
        );
        assert_eq!(
            glyphs[..count]
                .iter()
                .map(|glyph| glyph.advance.x)
                .collect::<Vec<_>>(),
            [2386, 7077, 3769, 6663, 2396, 6607, 2386, 5087, 5345]
        );
        assert_eq!(glyphs[0].cluster, TextRange::new(0, 6));
        assert_eq!(glyphs[1].cluster, TextRange::new(0, 6));
        assert_eq!(glyphs[5].cluster, TextRange::new(13, 22));
        assert_eq!(glyphs[6].cluster, TextRange::new(22, 34));
        assert_eq!(glyphs[7].cluster, TextRange::new(22, 34));
        assert!(
            [0, 1, 5, 6, 7]
                .into_iter()
                .all(|index| glyphs[index].unsafe_to_break())
        );
    }
}
