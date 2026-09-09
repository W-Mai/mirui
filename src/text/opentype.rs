use mirx::font::ShapingData;
use textflow::{
    bidi::Direction,
    shaping::{
        FontAccessError, FontFeature, GlyphSource, ShapeError, ShapeRequest, ShapedGlyph, TextRange,
    },
    unicode::Script,
};

const CLIG: [u8; 4] = *b"clig";
const DLIG: [u8; 4] = *b"dlig";
const KERN: [u8; 4] = *b"kern";
const LIGA: [u8; 4] = *b"liga";
const IGNORE_MARKS: u16 = 0x0008;

pub(super) fn shape(
    source: &impl GlyphSource,
    shaping: ShapingData<'_>,
    request: &ShapeRequest<'_>,
    output: &mut [ShapedGlyph],
) -> Result<usize, ShapeError> {
    validate_request(request)?;
    validate_features(request.features)?;
    if matches!(request.script, Script::Arabic | Script::Devanagari) {
        return Err(ShapeError::UnsupportedScript {
            script: request.script,
        });
    }

    let text = &request.text[request.range.clone()];
    for cluster in textflow::unicode::graphemes(text) {
        if cluster.text.chars().nth(1).is_some() {
            return Err(ShapeError::UnsupportedCluster {
                offset: request.range.start + cluster.range.start,
            });
        }
    }
    let mut count = text.chars().count();
    if output.len() < count {
        return Err(ShapeError::InsufficientCapacity { required: count });
    }
    for (slot, (offset, character)) in output.iter_mut().zip(text.char_indices()) {
        let start = request.range.start + offset;
        let end = start + character.len_utf8();
        let glyph = source
            .glyph_for(character)?
            .or(source.notdef_glyph()?)
            .ok_or(ShapeError::MissingGlyph { offset: start })?;
        *slot = ShapedGlyph::new(glyph, TextRange::new(start as u32, end as u32));
    }

    let gdef = shaping.table(*b"GDEF").map(Table::new);
    if let Some(table) = shaping.table(*b"GSUB") {
        count = apply_layout(
            table,
            gdef,
            LayoutKind::Substitution,
            request,
            output,
            count,
        )?
        .glyph_count;
    }
    for glyph in &mut output[..count] {
        glyph.advance = source.glyph_advance(glyph.glyph_id())?;
    }

    let kern_enabled = feature_enabled(request, KERN, true)?;
    let positioned = if let Some(table) = shaping.table(*b"GPOS") {
        apply_layout(table, gdef, LayoutKind::Positioning, request, output, count)?.kern_lookups > 0
    } else {
        false
    };
    if kern_enabled && !positioned {
        for index in 0..count.saturating_sub(1) {
            let delta = source.kerning(output[index].glyph_id(), output[index + 1].glyph_id())?;
            output[index].advance.x = output[index]
                .advance
                .x
                .checked_add(delta)
                .ok_or(ShapeError::Source(FontAccessError::Malformed))?;
        }
    }
    if request.direction == Direction::RightToLeft {
        output[..count].reverse();
    }
    Ok(count)
}

fn validate_request(request: &ShapeRequest<'_>) -> Result<(), ShapeError> {
    if request.range.start > request.range.end
        || request.range.end > request.text.len()
        || !request.text.is_char_boundary(request.range.start)
        || !request.text.is_char_boundary(request.range.end)
    {
        return Err(ShapeError::InvalidTextRange);
    }
    if request.text.len() > u32::MAX as usize {
        return Err(ShapeError::TextTooLong);
    }
    Ok(())
}

fn validate_features(features: &[FontFeature]) -> Result<(), ShapeError> {
    for feature in features {
        if !matches!(feature.tag, CLIG | DLIG | KERN | LIGA) {
            return Err(ShapeError::UnsupportedFeature { tag: feature.tag });
        }
    }
    Ok(())
}

fn feature_enabled(
    request: &ShapeRequest<'_>,
    tag: [u8; 4],
    default: bool,
) -> Result<bool, ShapeError> {
    let mut enabled = default;
    for feature in request.features.iter().filter(|feature| feature.tag == tag) {
        let range = feature.range;
        let request_range = TextRange::new(request.range.start as u32, request.range.end as u32);
        if range.end <= request_range.start || range.start >= request_range.end {
            continue;
        }
        if range.start > request_range.start || range.end < request_range.end {
            return Err(ShapeError::UnsupportedFeature { tag });
        }
        enabled = feature.value != 0;
    }
    Ok(enabled)
}

#[derive(Clone, Copy)]
enum LayoutKind {
    Substitution,
    Positioning,
}

struct LayoutOutcome {
    glyph_count: usize,
    kern_lookups: usize,
}

#[derive(Clone, Copy)]
struct FeatureApplication<'table, 'request, 'text> {
    feature_list: Table<'table>,
    lookup_list: Table<'table>,
    gdef: Option<Table<'table>>,
    kind: LayoutKind,
    request: &'request ShapeRequest<'text>,
}

fn apply_layout(
    bytes: &[u8],
    gdef: Option<Table<'_>>,
    kind: LayoutKind,
    request: &ShapeRequest<'_>,
    glyphs: &mut [ShapedGlyph],
    count: usize,
) -> Result<LayoutOutcome, ShapeError> {
    let table = Table::new(bytes);
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let script_list = table.offset16(4).ok_or_else(malformed)?;
    let feature_list = table.offset16(6).ok_or_else(malformed)?;
    let lookup_list = table.offset16(8).ok_or_else(malformed)?;
    let Some(langsys) = script_list.langsys(script_tag(request.script))? else {
        return Ok(LayoutOutcome {
            glyph_count: count,
            kern_lookups: 0,
        });
    };
    let required = langsys.u16(2).ok_or_else(malformed)?;
    let mut outcome = LayoutOutcome {
        glyph_count: count,
        kern_lookups: 0,
    };
    let application = FeatureApplication {
        feature_list,
        lookup_list,
        gdef,
        kind,
        request,
    };
    if required != u16::MAX {
        apply_feature(application, required, true, glyphs, &mut outcome)?;
    }
    let feature_count = langsys.u16(4).ok_or_else(malformed)?;
    for index in 0..feature_count {
        let feature_index = langsys
            .u16(6 + usize::from(index) * 2)
            .ok_or_else(malformed)?;
        if feature_index == required {
            continue;
        }
        apply_feature(application, feature_index, false, glyphs, &mut outcome)?;
    }
    Ok(outcome)
}

fn apply_feature(
    application: FeatureApplication<'_, '_, '_>,
    feature_index: u16,
    required: bool,
    glyphs: &mut [ShapedGlyph],
    outcome: &mut LayoutOutcome,
) -> Result<(), ShapeError> {
    let FeatureApplication {
        feature_list,
        lookup_list,
        gdef,
        kind,
        request,
    } = application;
    let count = feature_list.u16(0).ok_or_else(malformed)?;
    if feature_index >= count {
        return Err(malformed());
    }
    let record = 2 + usize::from(feature_index) * 6;
    let tag = feature_list.tag(record).ok_or_else(malformed)?;
    let feature = feature_list.offset16(record + 4).ok_or_else(malformed)?;
    let default = match (kind, tag) {
        (LayoutKind::Substitution, CLIG | LIGA) => true,
        (LayoutKind::Substitution, DLIG) => false,
        (LayoutKind::Positioning, KERN) => true,
        _ => false,
    };
    if !required && !feature_enabled(request, tag, default)? {
        return Ok(());
    }
    let lookup_count = feature.u16(2).ok_or_else(malformed)?;
    for index in 0..lookup_count {
        let lookup_index = feature
            .u16(4 + usize::from(index) * 2)
            .ok_or_else(malformed)?;
        let total_lookups = lookup_list.u16(0).ok_or_else(malformed)?;
        if lookup_index >= total_lookups {
            return Err(malformed());
        }
        let lookup = lookup_list
            .offset16(2 + usize::from(lookup_index) * 2)
            .ok_or_else(malformed)?;
        match kind {
            LayoutKind::Substitution => {
                outcome.glyph_count =
                    apply_substitution_lookup(lookup, tag, glyphs, outcome.glyph_count)?;
            }
            LayoutKind::Positioning => {
                apply_positioning_lookup(lookup, gdef, tag, glyphs, outcome.glyph_count)?;
                if tag == KERN {
                    outcome.kern_lookups += 1;
                }
            }
        }
    }
    Ok(())
}

fn apply_substitution_lookup(
    lookup: Table<'_>,
    feature: [u8; 4],
    glyphs: &mut [ShapedGlyph],
    mut count: usize,
) -> Result<usize, ShapeError> {
    if lookup.u16(2).unwrap_or(0) != 0 {
        return Err(ShapeError::UnsupportedFeature { tag: feature });
    }
    let lookup_type = lookup.u16(0).ok_or_else(malformed)?;
    let subtable_count = lookup.u16(4).ok_or_else(malformed)?;
    for subtable_index in 0..subtable_count {
        let subtable = lookup
            .offset16(6 + usize::from(subtable_index) * 2)
            .ok_or_else(malformed)?;
        let (lookup_type, subtable) = extension(subtable, lookup_type, 7)?;
        if lookup_type != 4 {
            return Err(ShapeError::UnsupportedFeature { tag: feature });
        }
        let mut index = 0;
        while index < count {
            let consumed = apply_ligature(subtable, glyphs, count, index)?;
            if consumed > 0 {
                count -= consumed - 1;
            }
            index += 1;
        }
    }
    Ok(count)
}

fn apply_ligature(
    table: Table<'_>,
    glyphs: &mut [ShapedGlyph],
    count: usize,
    index: usize,
) -> Result<usize, ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let Some(set_index) = coverage_index(coverage, glyphs[index].glyph_id().value())? else {
        return Ok(0);
    };
    let set_count = table.u16(4).ok_or_else(malformed)?;
    if set_index >= set_count {
        return Err(malformed());
    }
    let set = table
        .offset16(6 + usize::from(set_index) * 2)
        .ok_or_else(malformed)?;
    let ligature_count = set.u16(0).ok_or_else(malformed)?;
    for ligature_index in 0..ligature_count {
        let ligature = set
            .offset16(2 + usize::from(ligature_index) * 2)
            .ok_or_else(malformed)?;
        let component_count = usize::from(ligature.u16(2).ok_or_else(malformed)?);
        if component_count < 2 || index + component_count > count {
            continue;
        }
        let mut matches = true;
        for component in 1..component_count {
            let expected = ligature
                .u16(4 + (component - 1) * 2)
                .ok_or_else(malformed)?;
            matches &= glyphs[index + component].glyph_id().value() == expected;
        }
        if !matches {
            continue;
        }
        let replacement = ligature.u16(0).ok_or_else(malformed)?;
        let cluster = TextRange::new(
            glyphs[index].cluster.start,
            glyphs[index + component_count - 1].cluster.end,
        );
        glyphs[index] = ShapedGlyph::new(textflow::shaping::GlyphId::new(replacement), cluster);
        glyphs[index].set_unsafe_to_break(true);
        glyphs.copy_within(index + component_count..count, index + 1);
        return Ok(component_count);
    }
    Ok(0)
}

fn apply_positioning_lookup(
    lookup: Table<'_>,
    gdef: Option<Table<'_>>,
    feature: [u8; 4],
    glyphs: &mut [ShapedGlyph],
    count: usize,
) -> Result<(), ShapeError> {
    let flags = lookup.u16(2).ok_or_else(malformed)?;
    if flags & !IGNORE_MARKS != 0 {
        return Err(ShapeError::UnsupportedFeature { tag: feature });
    }
    let lookup_type = lookup.u16(0).ok_or_else(malformed)?;
    let subtable_count = lookup.u16(4).ok_or_else(malformed)?;
    for subtable_index in 0..subtable_count {
        let subtable = lookup
            .offset16(6 + usize::from(subtable_index) * 2)
            .ok_or_else(malformed)?;
        let (lookup_type, subtable) = extension(subtable, lookup_type, 9)?;
        if lookup_type != 2 {
            return Err(ShapeError::UnsupportedFeature { tag: feature });
        }
        for first in 0..count.saturating_sub(1) {
            if flags & IGNORE_MARKS != 0 && glyph_is_mark(gdef, glyphs[first].glyph_id().value())? {
                continue;
            }
            let mut second = first + 1;
            while second < count
                && flags & IGNORE_MARKS != 0
                && glyph_is_mark(gdef, glyphs[second].glyph_id().value())?
            {
                second += 1;
            }
            if second == count {
                break;
            }
            let (left, right) = glyphs.split_at_mut(second);
            apply_pair(subtable, &mut left[first], &mut right[0])?;
        }
    }
    Ok(())
}

fn extension(
    table: Table<'_>,
    lookup_type: u16,
    extension_type: u16,
) -> Result<(u16, Table<'_>), ShapeError> {
    if lookup_type != extension_type {
        return Ok((lookup_type, table));
    }
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let inner_type = table.u16(2).ok_or_else(malformed)?;
    let inner = table.offset32(4).ok_or_else(malformed)?;
    Ok((inner_type, inner))
}

fn apply_pair(
    table: Table<'_>,
    first_glyph: &mut ShapedGlyph,
    second_glyph: &mut ShapedGlyph,
) -> Result<(), ShapeError> {
    let first = first_glyph.glyph_id().value();
    let second = second_glyph.glyph_id().value();
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let Some(coverage_index) = coverage_index(coverage, first)? else {
        return Ok(());
    };
    let format1 = table.u16(4).ok_or_else(malformed)?;
    let format2 = table.u16(6).ok_or_else(malformed)?;
    let pair = match table.u16(0).ok_or_else(malformed)? {
        1 => {
            let set_count = table.u16(8).ok_or_else(malformed)?;
            if coverage_index >= set_count {
                return Err(malformed());
            }
            let set = table
                .offset16(10 + usize::from(coverage_index) * 2)
                .ok_or_else(malformed)?;
            pair_format_one(set, second, format1, format2)?
        }
        2 => pair_format_two(table, first, second, format1, format2)?,
        _ => return Err(malformed()),
    };
    let Some((first_value, second_value)) = pair else {
        return Ok(());
    };
    apply_value(first_glyph, first_value, format1)?;
    apply_value(second_glyph, second_value, format2)
}

fn glyph_is_mark(gdef: Option<Table<'_>>, glyph: u16) -> Result<bool, ShapeError> {
    let Some(gdef) = gdef else {
        return Ok(false);
    };
    if gdef.u16(0) != Some(1) {
        return Err(malformed());
    }
    let offset = gdef.u16(4).ok_or_else(malformed)?;
    if offset == 0 {
        return Ok(false);
    }
    Ok(class_index(gdef.tail(usize::from(offset)).ok_or_else(malformed)?, glyph)? == 3)
}

fn pair_format_one<'a>(
    set: Table<'a>,
    second: u16,
    format1: u16,
    format2: u16,
) -> Result<Option<(Table<'a>, Table<'a>)>, ShapeError> {
    let count = set.u16(0).ok_or_else(malformed)?;
    let value1 = value_size(format1)?;
    let value2 = value_size(format2)?;
    let record_size = 2usize
        .checked_add(value1)
        .and_then(|value| value.checked_add(value2))
        .ok_or_else(malformed)?;
    let mut start = 0usize;
    let mut end = usize::from(count);
    while start < end {
        let middle = start + (end - start) / 2;
        let offset = 2 + middle * record_size;
        let current = set.u16(offset).ok_or_else(malformed)?;
        match current.cmp(&second) {
            core::cmp::Ordering::Less => start = middle + 1,
            core::cmp::Ordering::Greater => end = middle,
            core::cmp::Ordering::Equal => {
                return Ok(Some((
                    set.tail(offset + 2).ok_or_else(malformed)?,
                    set.tail(offset + 2 + value1).ok_or_else(malformed)?,
                )));
            }
        }
    }
    Ok(None)
}

fn pair_format_two<'a>(
    table: Table<'a>,
    first: u16,
    second: u16,
    format1: u16,
    format2: u16,
) -> Result<Option<(Table<'a>, Table<'a>)>, ShapeError> {
    let class1 = class_index(table.offset16(8).ok_or_else(malformed)?, first)?;
    let class2 = class_index(table.offset16(10).ok_or_else(malformed)?, second)?;
    let count1 = table.u16(12).ok_or_else(malformed)?;
    let count2 = table.u16(14).ok_or_else(malformed)?;
    if class1 >= count1 || class2 >= count2 {
        return Err(malformed());
    }
    let value1 = value_size(format1)?;
    let value2 = value_size(format2)?;
    let record_size = value1.checked_add(value2).ok_or_else(malformed)?;
    let record = usize::from(class1)
        .checked_mul(usize::from(count2))
        .and_then(|value| value.checked_add(usize::from(class2)))
        .and_then(|value| value.checked_mul(record_size))
        .and_then(|value| value.checked_add(16))
        .ok_or_else(malformed)?;
    Ok(Some((
        table.tail(record).ok_or_else(malformed)?,
        table.tail(record + value1).ok_or_else(malformed)?,
    )))
}

fn apply_value(glyph: &mut ShapedGlyph, table: Table<'_>, format: u16) -> Result<(), ShapeError> {
    let mut offset = 0usize;
    for (mask, target) in [
        (0x0001, ValueTarget::OffsetX),
        (0x0002, ValueTarget::OffsetY),
        (0x0004, ValueTarget::AdvanceX),
        (0x0008, ValueTarget::AdvanceY),
    ] {
        if format & mask == 0 {
            continue;
        }
        let value = i32::from(table.i16(offset).ok_or_else(malformed)?) << 8;
        offset += 2;
        let target = match target {
            ValueTarget::OffsetX => &mut glyph.offset.x,
            ValueTarget::OffsetY => &mut glyph.offset.y,
            ValueTarget::AdvanceX => &mut glyph.advance.x,
            ValueTarget::AdvanceY => &mut glyph.advance.y,
        };
        *target = target.checked_add(value).ok_or_else(malformed)?;
    }
    Ok(())
}

enum ValueTarget {
    OffsetX,
    OffsetY,
    AdvanceX,
    AdvanceY,
}

fn value_size(format: u16) -> Result<usize, ShapeError> {
    if format & !0x00ff != 0 {
        return Err(malformed());
    }
    Ok(format.count_ones() as usize * 2)
}

fn coverage_index(table: Table<'_>, glyph: u16) -> Result<Option<u16>, ShapeError> {
    match table.u16(0).ok_or_else(malformed)? {
        1 => {
            let count = table.u16(2).ok_or_else(malformed)?;
            for index in 0..count {
                match table.u16(4 + usize::from(index) * 2) {
                    Some(current) if current == glyph => return Ok(Some(index)),
                    Some(current) if current > glyph => return Ok(None),
                    Some(_) => {}
                    None => return Err(malformed()),
                }
            }
            Ok(None)
        }
        2 => {
            let count = table.u16(2).ok_or_else(malformed)?;
            for index in 0..count {
                let offset = 4 + usize::from(index) * 6;
                let start = table.u16(offset).ok_or_else(malformed)?;
                let end = table.u16(offset + 2).ok_or_else(malformed)?;
                let base = table.u16(offset + 4).ok_or_else(malformed)?;
                if (start..=end).contains(&glyph) {
                    return base
                        .checked_add(glyph - start)
                        .ok_or_else(malformed)
                        .map(Some);
                }
            }
            Ok(None)
        }
        _ => Err(malformed()),
    }
}

fn class_index(table: Table<'_>, glyph: u16) -> Result<u16, ShapeError> {
    match table.u16(0).ok_or_else(malformed)? {
        1 => {
            let start = table.u16(2).ok_or_else(malformed)?;
            let count = table.u16(4).ok_or_else(malformed)?;
            let Some(index) = glyph.checked_sub(start).filter(|index| *index < count) else {
                return Ok(0);
            };
            table.u16(6 + usize::from(index) * 2).ok_or_else(malformed)
        }
        2 => {
            let count = table.u16(2).ok_or_else(malformed)?;
            for index in 0..count {
                let offset = 4 + usize::from(index) * 6;
                let start = table.u16(offset).ok_or_else(malformed)?;
                let end = table.u16(offset + 2).ok_or_else(malformed)?;
                let class = table.u16(offset + 4).ok_or_else(malformed)?;
                if (start..=end).contains(&glyph) {
                    return Ok(class);
                }
            }
            Ok(0)
        }
        _ => Err(malformed()),
    }
}

fn script_tag(script: Script) -> [u8; 4] {
    match script {
        Script::Latin => *b"latn",
        Script::Greek => *b"grek",
        Script::Cyrillic => *b"cyrl",
        Script::Hebrew => *b"hebr",
        Script::Arabic => *b"arab",
        Script::Thai => *b"thai",
        Script::Devanagari => *b"deva",
        Script::Han => *b"hani",
        Script::Hiragana | Script::Katakana => *b"kana",
        Script::Hangul => *b"hang",
        Script::Common | Script::Inherited | Script::Unknown => *b"DFLT",
    }
}

#[derive(Clone, Copy)]
struct Table<'a> {
    bytes: &'a [u8],
}

impl<'a> Table<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn u16(self, offset: usize) -> Option<u16> {
        Some(u16::from_be_bytes(
            self.bytes.get(offset..offset + 2)?.try_into().ok()?,
        ))
    }

    fn i16(self, offset: usize) -> Option<i16> {
        Some(i16::from_be_bytes(
            self.bytes.get(offset..offset + 2)?.try_into().ok()?,
        ))
    }

    fn u32(self, offset: usize) -> Option<u32> {
        Some(u32::from_be_bytes(
            self.bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }

    fn tag(self, offset: usize) -> Option<[u8; 4]> {
        self.bytes.get(offset..offset + 4)?.try_into().ok()
    }

    fn tail(self, offset: usize) -> Option<Self> {
        Some(Self::new(self.bytes.get(offset..)?))
    }

    fn offset16(self, offset: usize) -> Option<Self> {
        self.tail(usize::from(self.u16(offset)?))
    }

    fn offset32(self, offset: usize) -> Option<Self> {
        self.tail(usize::try_from(self.u32(offset)?).ok()?)
    }

    fn langsys(self, requested: [u8; 4]) -> Result<Option<Self>, ShapeError> {
        let count = self.u16(0).ok_or_else(malformed)?;
        let mut fallback = None;
        for index in 0..count {
            let record = 2 + usize::from(index) * 6;
            let tag = self.tag(record).ok_or_else(malformed)?;
            let script = self.offset16(record + 4).ok_or_else(malformed)?;
            if tag == requested {
                return script.default_langsys();
            }
            if tag == *b"DFLT" {
                fallback = Some(script);
            }
        }
        match fallback {
            Some(script) => script.default_langsys(),
            None => Ok(None),
        }
    }

    fn default_langsys(self) -> Result<Option<Self>, ShapeError> {
        let offset = self.u16(0).ok_or_else(malformed)?;
        if offset == 0 {
            Ok(None)
        } else {
            self.tail(usize::from(offset))
                .ok_or_else(malformed)
                .map(Some)
        }
    }
}

fn malformed() -> ShapeError {
    ShapeError::Source(FontAccessError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use textflow::shaping::{FlowPoint, GlyphId};

    #[test]
    fn maps_thai_to_the_opentype_script_tag() {
        assert_eq!(script_tag(Script::Thai), *b"thai");
    }

    fn glyph(id: u16, start: u32, end: u32) -> ShapedGlyph {
        let mut glyph = ShapedGlyph::new(GlyphId::new(id), TextRange::new(start, end));
        glyph.advance = FlowPoint { x: 100 << 8, y: 0 };
        glyph
    }

    fn write_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn patch_u16(bytes: &mut [u8], offset: usize, value: usize) {
        bytes[offset..offset + 2].copy_from_slice(&u16::try_from(value).unwrap().to_be_bytes());
    }

    fn ligature_lookup(first: u16, second: u16, replacement: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [
            4,
            0,
            1,
            8,
            1,
            8,
            1,
            14,
            1,
            1,
            first,
            1,
            4,
            replacement,
            2,
            second,
        ] {
            write_u16(&mut bytes, value);
        }
        bytes
    }

    fn layout_table(tag: [u8; 4], lookups: &[Vec<u8>], order: &[u16]) -> Vec<u8> {
        let mut bytes = vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0];

        let script_list = bytes.len();
        write_u16(&mut bytes, 1);
        bytes.extend_from_slice(b"latn");
        write_u16(&mut bytes, 8);
        for value in [4, 0, 0, u16::MAX, 1, 0] {
            write_u16(&mut bytes, value);
        }

        let feature_list = bytes.len();
        write_u16(&mut bytes, 1);
        bytes.extend_from_slice(&tag);
        write_u16(&mut bytes, 8);
        write_u16(&mut bytes, 0);
        write_u16(&mut bytes, u16::try_from(order.len()).unwrap());
        for lookup in order {
            write_u16(&mut bytes, *lookup);
        }

        let lookup_list = bytes.len();
        write_u16(&mut bytes, u16::try_from(lookups.len()).unwrap());
        let offsets = bytes.len();
        bytes.resize(offsets + lookups.len() * 2, 0);
        for (index, lookup) in lookups.iter().enumerate() {
            let offset = bytes.len() - lookup_list;
            patch_u16(&mut bytes, offsets + index * 2, offset);
            bytes.extend_from_slice(lookup);
        }

        patch_u16(&mut bytes, 4, script_list);
        patch_u16(&mut bytes, 6, feature_list);
        patch_u16(&mut bytes, 8, lookup_list);
        bytes
    }

    fn pair_lookup() -> Vec<u8> {
        pair_lookup_with_flags(0)
    }

    fn pair_lookup_with_flags(flags: u16) -> Vec<u8> {
        let mut bytes = vec![0, 2];
        write_u16(&mut bytes, flags);
        bytes.extend_from_slice(&[0, 1, 0, 8]);
        bytes.extend_from_slice(&[
            0, 1, 0, 12, 0, 5, 0, 1, 0, 1, 0, 18, 0, 1, 0, 1, 0, 4, 0, 1, 0, 7, 0xff, 0xec, 0, 0,
            0, 5,
        ]);
        bytes
    }

    #[test]
    fn ligature_compacts_components_and_unions_the_cluster() {
        let table = [
            0, 1, 0, 8, 0, 1, 0, 14, 0, 1, 0, 1, 0, 4, 0, 1, 0, 4, 0, 9, 0, 3, 0, 5, 0, 6,
        ];
        let mut glyphs = [
            glyph(4, 0, 1),
            glyph(5, 1, 2),
            glyph(6, 2, 3),
            glyph(8, 3, 4),
        ];
        assert_eq!(apply_ligature(Table::new(&table), &mut glyphs, 4, 0), Ok(3));
        assert_eq!(glyphs[0].glyph_id(), GlyphId::new(9));
        assert_eq!(glyphs[0].cluster, TextRange::new(0, 3));
        assert!(glyphs[0].unsafe_to_break());
        assert_eq!(glyphs[1].glyph_id(), GlyphId::new(8));
    }

    #[test]
    fn layout_applies_lookups_in_feature_order() {
        let lookups = [ligature_lookup(8, 6, 9), ligature_lookup(4, 5, 8)];
        let table = layout_table(LIGA, &lookups, &[1, 0]);
        let request = ShapeRequest::new("abc", 0..3, Direction::LeftToRight, Script::Latin);
        let mut glyphs = [glyph(4, 0, 1), glyph(5, 1, 2), glyph(6, 2, 3)];
        let outcome = apply_layout(
            &table,
            None,
            LayoutKind::Substitution,
            &request,
            &mut glyphs,
            3,
        )
        .unwrap();
        assert_eq!(outcome.glyph_count, 1);
        assert_eq!(glyphs[0].glyph_id(), GlyphId::new(9));
        assert_eq!(glyphs[0].cluster, TextRange::new(0, 3));
        assert!(glyphs[0].unsafe_to_break());
    }

    #[test]
    fn kern_feature_controls_pair_positioning() {
        let table = layout_table(KERN, &[pair_lookup()], &[0]);
        let request = ShapeRequest::new("ab", 0..2, Direction::LeftToRight, Script::Latin);
        let mut glyphs = [glyph(4, 0, 1), glyph(7, 1, 2)];
        let outcome = apply_layout(
            &table,
            None,
            LayoutKind::Positioning,
            &request,
            &mut glyphs,
            2,
        )
        .unwrap();
        assert_eq!(outcome.kern_lookups, 1);
        assert_eq!(glyphs[0].offset.x, -20 << 8);
        assert_eq!(glyphs[1].offset.x, 5 << 8);

        let features = [FontFeature::new(KERN, 0)];
        let request = request.with_features(&features);
        let mut glyphs = [glyph(4, 0, 1), glyph(7, 1, 2)];
        let outcome = apply_layout(
            &table,
            None,
            LayoutKind::Positioning,
            &request,
            &mut glyphs,
            2,
        )
        .unwrap();
        assert_eq!(outcome.kern_lookups, 0);
        assert_eq!(glyphs[0].offset.x, 0);
        assert_eq!(glyphs[1].offset.x, 0);
    }

    #[test]
    fn pair_format_one_applies_placement_and_advance() {
        let table = [
            0, 1, 0, 12, 0, 5, 0, 1, 0, 1, 0, 18, 0, 1, 0, 1, 0, 4, 0, 1, 0, 7, 0xff, 0xec, 0, 0,
            0, 5,
        ];
        let mut glyphs = [glyph(4, 0, 1), glyph(7, 1, 2)];
        let (left, right) = glyphs.split_at_mut(1);
        apply_pair(Table::new(&table), &mut left[0], &mut right[0]).unwrap();
        assert_eq!(glyphs[0].offset.x, -20 << 8);
        assert_eq!(glyphs[0].advance.x, 100 << 8);
        assert_eq!(glyphs[1].offset.x, 5 << 8);
    }

    #[test]
    fn ignore_marks_pairs_base_glyphs_across_gdef_marks() {
        let table = layout_table(KERN, &[pair_lookup_with_flags(IGNORE_MARKS)], &[0]);
        let gdef = [
            0, 1, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 1, 0, 4, 0, 4, 0, 1, 0, 0, 0, 3, 0, 1,
        ];
        let request = ShapeRequest::new("abc", 0..3, Direction::LeftToRight, Script::Latin);
        let mut glyphs = [glyph(4, 0, 1), glyph(6, 1, 2), glyph(7, 2, 3)];
        let outcome = apply_layout(
            &table,
            Some(Table::new(&gdef)),
            LayoutKind::Positioning,
            &request,
            &mut glyphs,
            3,
        )
        .unwrap();

        assert_eq!(outcome.kern_lookups, 1);
        assert_eq!(glyphs[0].offset.x, -20 << 8);
        assert_eq!(glyphs[1].offset.x, 0);
        assert_eq!(glyphs[2].offset.x, 5 << 8);
    }

    #[test]
    fn coverage_and_class_def_support_ranges() {
        let coverage = [0, 2, 0, 1, 0, 4, 0, 6, 0, 9];
        assert_eq!(coverage_index(Table::new(&coverage), 5), Ok(Some(10)));
        let classes = [0, 2, 0, 1, 0, 4, 0, 6, 0, 3];
        assert_eq!(class_index(Table::new(&classes), 5), Ok(3));
        assert_eq!(class_index(Table::new(&classes), 7), Ok(0));
    }
}
