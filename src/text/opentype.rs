use mirx::font::ShapingData as MirxShapingData;
use textflow::{
    bidi::Direction,
    shaping::{
        FontAccessError, FontFeature, GlyphBuffer, GlyphMask, GlyphSource, LookupRequest,
        LookupStatus, ShapeError, ShapeRequest, ShapedGlyph, ShapingData, TextRange,
    },
    unicode::Script,
};

const CLIG: [u8; 4] = *b"clig";
const DLIG: [u8; 4] = *b"dlig";
const KERN: [u8; 4] = *b"kern";
const LIGA: [u8; 4] = *b"liga";
const RIGHT_TO_LEFT: u16 = 0x0001;
const IGNORE_MARKS: u16 = 0x0008;
const USE_MARK_FILTERING_SET: u16 = 0x0010;

#[derive(Clone, Copy)]
pub(super) struct OpenTypeShaping<'a> {
    shaping: MirxShapingData<'a>,
}

impl<'a> OpenTypeShaping<'a> {
    pub(super) const fn new(shaping: MirxShapingData<'a>) -> Self {
        Self { shaping }
    }
}

impl ShapingData for OpenTypeShaping<'_> {
    fn substitute(
        &self,
        request: LookupRequest<'_, '_>,
        glyphs: &mut GlyphBuffer<'_>,
    ) -> Result<LookupStatus, ShapeError> {
        let Some(table) = self.shaping.table(*b"GSUB") else {
            return Ok(LookupStatus::NotFound);
        };
        apply_requested_layout(
            table,
            self.shaping.table(*b"GDEF").map(Table::new),
            LayoutKind::Substitution,
            request,
            glyphs,
        )
    }

    fn position(
        &self,
        request: LookupRequest<'_, '_>,
        glyphs: &mut GlyphBuffer<'_>,
    ) -> Result<LookupStatus, ShapeError> {
        let Some(table) = self.shaping.table(*b"GPOS") else {
            return Ok(LookupStatus::NotFound);
        };
        apply_requested_layout(
            table,
            self.shaping.table(*b"GDEF").map(Table::new),
            LayoutKind::Positioning,
            request,
            glyphs,
        )
    }
}

pub(super) fn shape(
    source: &impl GlyphSource,
    shaping: MirxShapingData<'_>,
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

fn apply_requested_layout(
    bytes: &[u8],
    gdef: Option<Table<'_>>,
    kind: LayoutKind,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<LookupStatus, ShapeError> {
    let table = Table::new(bytes);
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let script_list = table.offset16(4).ok_or_else(malformed)?;
    let feature_list = table.offset16(6).ok_or_else(malformed)?;
    let lookup_list = table.offset16(8).ok_or_else(malformed)?;
    let Some(langsys) =
        script_list.langsys(script_tag(request.shape().script), request.shape().language)?
    else {
        return Ok(LookupStatus::NotFound);
    };
    let application = RequestedFeatureApplication {
        feature_list,
        lookup_list,
        gdef,
        kind,
        request,
    };
    let mut found = false;
    let required = langsys.u16(2).ok_or_else(malformed)?;
    if required != u16::MAX {
        found |= application.apply(required, glyphs)?;
    }
    let feature_count = langsys.u16(4).ok_or_else(malformed)?;
    for index in 0..feature_count {
        let feature_index = langsys
            .u16(6 + usize::from(index) * 2)
            .ok_or_else(malformed)?;
        if feature_index == required {
            continue;
        }
        found |= application.apply(feature_index, glyphs)?;
    }
    Ok(if found {
        LookupStatus::Applied
    } else {
        LookupStatus::NotFound
    })
}

fn selected(request: LookupRequest<'_, '_>, glyphs: &GlyphBuffer<'_>, index: usize) -> bool {
    glyphs.mask(index).is_some_and(|mask| request.selects(mask))
}

#[derive(Clone, Copy)]
struct LookupFilter<'a> {
    gdef: Option<Table<'a>>,
    flags: u16,
    mark_set: Option<Table<'a>>,
}

impl<'a> LookupFilter<'a> {
    fn parse(
        lookup: Table<'a>,
        gdef: Option<Table<'a>>,
        feature: [u8; 4],
    ) -> Result<Self, ShapeError> {
        let flags = lookup.u16(2).ok_or_else(malformed)?;
        if flags & !(RIGHT_TO_LEFT | IGNORE_MARKS | USE_MARK_FILTERING_SET) != 0 {
            return Err(ShapeError::UnsupportedFeature { tag: feature });
        }
        let mark_set = if flags & USE_MARK_FILTERING_SET != 0 {
            let subtable_count = usize::from(lookup.u16(4).ok_or_else(malformed)?);
            let set_index = lookup.u16(6 + subtable_count * 2).ok_or_else(malformed)?;
            Some(mark_filter_set(gdef.ok_or_else(malformed)?, set_index)?)
        } else {
            None
        };
        Ok(Self {
            gdef,
            flags,
            mark_set,
        })
    }

    fn ignores(self, glyph: &ShapedGlyph) -> Result<bool, ShapeError> {
        let is_mark = glyph_is_mark(self.gdef, glyph.glyph_id().value())?;
        if !is_mark {
            return Ok(false);
        }
        if self.flags & IGNORE_MARKS != 0 {
            return Ok(true);
        }
        match self.mark_set {
            Some(coverage) => covered(coverage, glyph).map(|included| !included),
            None => Ok(false),
        }
    }
}

fn next_included(
    glyphs: &GlyphBuffer<'_>,
    filter: LookupFilter<'_>,
    mut index: usize,
) -> Result<Option<usize>, ShapeError> {
    while let Some(glyph) = glyphs.get(index) {
        if !filter.ignores(glyph)? {
            return Ok(Some(index));
        }
        index += 1;
    }
    Ok(None)
}

fn previous_included(
    glyphs: &GlyphBuffer<'_>,
    filter: LookupFilter<'_>,
    mut index: usize,
) -> Result<Option<usize>, ShapeError> {
    while index > 0 {
        index -= 1;
        let glyph = glyphs.get(index).ok_or_else(malformed)?;
        if !filter.ignores(glyph)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn included_at(
    glyphs: &GlyphBuffer<'_>,
    filter: LookupFilter<'_>,
    start: usize,
    offset: usize,
) -> Result<Option<usize>, ShapeError> {
    let mut slot = Some(start);
    for _ in 0..offset {
        slot = match slot {
            Some(slot) => next_included(glyphs, filter, slot + 1)?,
            None => return Ok(None),
        };
    }
    Ok(slot)
}

fn covered(table: Table<'_>, glyph: &ShapedGlyph) -> Result<bool, ShapeError> {
    coverage_index(table, glyph.glyph_id().value()).map(|index| index.is_some())
}

fn apply_substitution_lookup_buffer(
    lookup: Table<'_>,
    lookup_list: Table<'_>,
    gdef: Option<Table<'_>>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
    depth: u8,
) -> Result<(), ShapeError> {
    if depth > 8 {
        return Err(malformed());
    }
    let filter = LookupFilter::parse(lookup, gdef, request.feature())?;
    let lookup_type = lookup.u16(0).ok_or_else(malformed)?;
    let subtable_count = lookup.u16(4).ok_or_else(malformed)?;
    for subtable_index in 0..subtable_count {
        let subtable = lookup
            .offset16(6 + usize::from(subtable_index) * 2)
            .ok_or_else(malformed)?;
        let (lookup_type, subtable) = extension(subtable, lookup_type, 7)?;
        match lookup_type {
            1 => apply_single_substitution(subtable, filter, request, glyphs)?,
            2 => apply_multiple_substitution(subtable, filter, request, glyphs)?,
            4 => apply_ligature_substitution(subtable, filter, request, glyphs)?,
            6 => apply_chained_substitution(subtable, lookup_list, filter, request, glyphs, depth)?,
            _ => {
                return Err(ShapeError::UnsupportedFeature {
                    tag: request.feature(),
                });
            }
        }
    }
    Ok(())
}

fn apply_single_substitution(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    for index in 0..glyphs.len() {
        if !selected(request, glyphs, index)
            || filter.ignores(glyphs.get(index).ok_or_else(malformed)?)?
        {
            continue;
        }
        apply_single_at(table, glyphs, index)?;
    }
    Ok(())
}

fn apply_single_at(
    table: Table<'_>,
    glyphs: &mut GlyphBuffer<'_>,
    index: usize,
) -> Result<(), ShapeError> {
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let glyph = glyphs.get(index).ok_or_else(malformed)?.glyph_id().value();
    let Some(coverage_index) = coverage_index(coverage, glyph)? else {
        return Ok(());
    };
    let replacement = match table.u16(0).ok_or_else(malformed)? {
        1 => glyph.wrapping_add_signed(table.i16(4).ok_or_else(malformed)?),
        2 => {
            let count = table.u16(4).ok_or_else(malformed)?;
            if coverage_index >= count {
                return Err(malformed());
            }
            table
                .u16(6 + usize::from(coverage_index) * 2)
                .ok_or_else(malformed)?
        }
        _ => return Err(malformed()),
    };
    glyphs.set_glyph(index, textflow::shaping::GlyphId::new(replacement))
}

fn apply_multiple_substitution(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let mut index = 0;
    while index < glyphs.len() {
        if !selected(request, glyphs, index)
            || filter.ignores(glyphs.get(index).ok_or_else(malformed)?)?
        {
            index += 1;
            continue;
        }
        let replacement_count = apply_multiple_at(table, glyphs, index)?;
        index += replacement_count.max(1);
    }
    Ok(())
}

fn apply_multiple_at(
    table: Table<'_>,
    glyphs: &mut GlyphBuffer<'_>,
    index: usize,
) -> Result<usize, ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let sequence_count = table.u16(4).ok_or_else(malformed)?;
    let glyph = glyphs.get(index).ok_or_else(malformed)?;
    let Some(sequence_index) = coverage_index(coverage, glyph.glyph_id().value())? else {
        return Ok(1);
    };
    if sequence_index >= sequence_count {
        return Err(malformed());
    }
    let sequence = table
        .offset16(6 + usize::from(sequence_index) * 2)
        .ok_or_else(malformed)?;
    let replacement_count = usize::from(sequence.u16(0).ok_or_else(malformed)?);
    for replacement in 0..replacement_count {
        sequence.u16(2 + replacement * 2).ok_or_else(malformed)?;
    }
    let cluster = glyph.cluster;
    let mask = glyphs.mask(index).unwrap_or(GlyphMask::NONE);
    glyphs.replace_with(index..index + 1, replacement_count, |replacement| {
        let glyph_id = sequence.u16(2 + replacement * 2).unwrap_or_default();
        let mut glyph = ShapedGlyph::new(textflow::shaping::GlyphId::new(glyph_id), cluster);
        glyph.set_unsafe_to_break(replacement_count > 1);
        glyph
    })?;
    for replacement in 0..replacement_count {
        glyphs.set_mask(index + replacement, mask)?;
    }
    Ok(replacement_count)
}

fn apply_ligature_substitution(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    let mut index = 0;
    while index < glyphs.len() {
        if selected(request, glyphs, index)
            && !filter.ignores(glyphs.get(index).ok_or_else(malformed)?)?
        {
            apply_ligature_buffer(table, filter, glyphs, index)?;
        }
        index += 1;
    }
    Ok(())
}

fn apply_ligature_buffer(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    glyphs: &mut GlyphBuffer<'_>,
    index: usize,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let Some(set_index) = coverage_index(
        coverage,
        glyphs.get(index).ok_or_else(malformed)?.glyph_id().value(),
    )?
    else {
        return Ok(());
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
        if component_count < 2 {
            continue;
        }
        let mut matches = true;
        let mut slots = index;
        for component in 1..component_count {
            slots = next_included(glyphs, filter, slots + 1)?.unwrap_or(glyphs.len());
            let expected = ligature
                .u16(4 + (component - 1) * 2)
                .ok_or_else(malformed)?;
            matches &= glyphs
                .get(slots)
                .is_some_and(|glyph| glyph.glyph_id().value() == expected);
        }
        if !matches {
            continue;
        }
        let replacement = ligature.u16(0).ok_or_else(malformed)?;
        let first = glyphs.get(index).ok_or_else(malformed)?;
        let last = glyphs.get(slots).ok_or_else(malformed)?;
        let cluster = TextRange::new(first.cluster.start, last.cluster.end);
        let mask = glyphs.mask(index).unwrap_or(GlyphMask::NONE);
        let first = glyphs.get_mut(index).ok_or_else(malformed)?;
        *first = ShapedGlyph::new(textflow::shaping::GlyphId::new(replacement), cluster);
        first.set_unsafe_to_break(true);
        glyphs.set_mask(index, mask)?;
        for slot in (index + 1..=slots).rev() {
            if !filter.ignores(glyphs.get(slot).ok_or_else(malformed)?)? {
                glyphs.replace(slot..slot + 1, &[])?;
            }
        }
        return Ok(());
    }
    Ok(())
}

fn apply_chained_substitution(
    table: Table<'_>,
    lookup_list: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
    depth: u8,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(3) {
        return Err(ShapeError::UnsupportedFeature {
            tag: request.feature(),
        });
    }
    let context = ChainedContext::parse(table)?;

    let mut start = 0;
    while start < glyphs.len() {
        if selected(request, glyphs, start)
            && !filter.ignores(glyphs.get(start).ok_or_else(malformed)?)?
            && context.matches(filter, glyphs, start)?
        {
            for record in 0..context.record_count {
                let offset = context.records + record * 4;
                let sequence_index = usize::from(table.u16(offset).ok_or_else(malformed)?);
                if sequence_index >= context.input_count {
                    return Err(malformed());
                }
                let lookup_index = table.u16(offset + 2).ok_or_else(malformed)?;
                let lookup = lookup_at(lookup_list, lookup_index)?;
                let target =
                    included_at(glyphs, filter, start, sequence_index)?.ok_or_else(malformed)?;
                SubstitutionApplication {
                    lookup_list,
                    gdef: filter.gdef,
                    request,
                    depth: depth + 1,
                }
                .apply_at(lookup, glyphs, target)?;
            }
        }
        start += 1;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ChainedContext<'a> {
    table: Table<'a>,
    backtrack_count: usize,
    input_count: usize,
    input_offsets: usize,
    lookahead_count: usize,
    lookahead_offsets: usize,
    record_count: usize,
    records: usize,
}

impl<'a> ChainedContext<'a> {
    fn parse(table: Table<'a>) -> Result<Self, ShapeError> {
        let backtrack_count = usize::from(table.u16(2).ok_or_else(malformed)?);
        let input_count_offset = 4usize
            .checked_add(backtrack_count.checked_mul(2).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let input_count = usize::from(table.u16(input_count_offset).ok_or_else(malformed)?);
        if input_count == 0 {
            return Err(malformed());
        }
        let input_offsets = input_count_offset + 2;
        let lookahead_count_offset = input_offsets
            .checked_add(input_count.checked_mul(2).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let lookahead_count = usize::from(table.u16(lookahead_count_offset).ok_or_else(malformed)?);
        let lookahead_offsets = lookahead_count_offset + 2;
        let record_count_offset = lookahead_offsets
            .checked_add(lookahead_count.checked_mul(2).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let record_count = usize::from(table.u16(record_count_offset).ok_or_else(malformed)?);
        Ok(Self {
            table,
            backtrack_count,
            input_count,
            input_offsets,
            lookahead_count,
            lookahead_offsets,
            record_count,
            records: record_count_offset + 2,
        })
    }

    fn matches(
        self,
        filter: LookupFilter<'_>,
        glyphs: &GlyphBuffer<'_>,
        start: usize,
    ) -> Result<bool, ShapeError> {
        let mut slot = start;
        for backtrack in 0..self.backtrack_count {
            let Some(previous) = previous_included(glyphs, filter, slot)? else {
                return Ok(false);
            };
            let coverage = self
                .table
                .offset16(4 + backtrack * 2)
                .ok_or_else(malformed)?;
            if !covered(coverage, glyphs.get(previous).ok_or_else(malformed)?)? {
                return Ok(false);
            }
            slot = previous;
        }
        for input in 0..self.input_count {
            let Some(input_slot) = included_at(glyphs, filter, start, input)? else {
                return Ok(false);
            };
            let coverage = self
                .table
                .offset16(self.input_offsets + input * 2)
                .ok_or_else(malformed)?;
            if !covered(coverage, glyphs.get(input_slot).ok_or_else(malformed)?)? {
                return Ok(false);
            }
        }
        let Some(mut slot) = included_at(glyphs, filter, start, self.input_count - 1)? else {
            return Ok(false);
        };
        for lookahead in 0..self.lookahead_count {
            let Some(next) = next_included(glyphs, filter, slot + 1)? else {
                return Ok(false);
            };
            let coverage = self
                .table
                .offset16(self.lookahead_offsets + lookahead * 2)
                .ok_or_else(malformed)?;
            if !covered(coverage, glyphs.get(next).ok_or_else(malformed)?)? {
                return Ok(false);
            }
            slot = next;
        }
        Ok(true)
    }
}

fn lookup_at(lookup_list: Table<'_>, index: u16) -> Result<Table<'_>, ShapeError> {
    let count = lookup_list.u16(0).ok_or_else(malformed)?;
    if index >= count {
        return Err(malformed());
    }
    lookup_list
        .offset16(2 + usize::from(index) * 2)
        .ok_or_else(malformed)
}

#[derive(Clone, Copy)]
struct SubstitutionApplication<'table, 'request, 'text> {
    lookup_list: Table<'table>,
    gdef: Option<Table<'table>>,
    request: LookupRequest<'request, 'text>,
    depth: u8,
}

impl SubstitutionApplication<'_, '_, '_> {
    fn apply_at(
        self,
        lookup: Table<'_>,
        glyphs: &mut GlyphBuffer<'_>,
        index: usize,
    ) -> Result<(), ShapeError> {
        if self.depth > 8 {
            return Err(malformed());
        }
        let filter = LookupFilter::parse(lookup, self.gdef, self.request.feature())?;
        if filter.ignores(glyphs.get(index).ok_or_else(malformed)?)? {
            return Ok(());
        }
        let lookup_type = lookup.u16(0).ok_or_else(malformed)?;
        let subtable_count = lookup.u16(4).ok_or_else(malformed)?;
        for subtable_index in 0..subtable_count {
            let subtable = lookup
                .offset16(6 + usize::from(subtable_index) * 2)
                .ok_or_else(malformed)?;
            let (lookup_type, subtable) = extension(subtable, lookup_type, 7)?;
            match lookup_type {
                1 => apply_single_at(subtable, glyphs, index)?,
                2 => {
                    apply_multiple_at(subtable, glyphs, index)?;
                }
                4 => apply_ligature_buffer(subtable, filter, glyphs, index)?,
                6 => apply_chained_substitution(
                    subtable,
                    self.lookup_list,
                    filter,
                    self.request,
                    glyphs,
                    self.depth,
                )?,
                _ => {
                    return Err(ShapeError::UnsupportedFeature {
                        tag: self.request.feature(),
                    });
                }
            }
        }
        Ok(())
    }
}

fn apply_positioning_lookup_buffer(
    lookup: Table<'_>,
    gdef: Option<Table<'_>>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    let filter = LookupFilter::parse(lookup, gdef, request.feature())?;
    let lookup_type = lookup.u16(0).ok_or_else(malformed)?;
    let subtable_count = lookup.u16(4).ok_or_else(malformed)?;
    for subtable_index in 0..subtable_count {
        let subtable = lookup
            .offset16(6 + usize::from(subtable_index) * 2)
            .ok_or_else(malformed)?;
        let (lookup_type, subtable) = extension(subtable, lookup_type, 9)?;
        match lookup_type {
            2 => apply_pair_positioning(subtable, filter, request, glyphs)?,
            3 => apply_cursive_positioning(subtable, filter, request, glyphs)?,
            4 => apply_mark_to_base(subtable, filter, request, glyphs)?,
            5 => apply_mark_to_ligature(subtable, filter, request, glyphs)?,
            6 => apply_mark_to_mark(subtable, filter, request, glyphs)?,
            _ => {
                return Err(ShapeError::UnsupportedFeature {
                    tag: request.feature(),
                });
            }
        }
    }
    Ok(())
}

fn apply_pair_positioning(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    let mut first = 0;
    while first < glyphs.len() {
        if !selected(request, glyphs, first)
            || filter.ignores(glyphs.get(first).ok_or_else(malformed)?)?
        {
            first += 1;
            continue;
        }
        let Some(second) = next_included(glyphs, filter, first + 1)? else {
            break;
        };
        let (left, right) = glyphs.glyphs_mut().split_at_mut(second);
        apply_pair(table, &mut left[first], &mut right[0])?;
        first += 1;
    }
    Ok(())
}

fn apply_cursive_positioning(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let coverage = table.offset16(2).ok_or_else(malformed)?;
    let count = table.u16(4).ok_or_else(malformed)?;
    let mut first = 0;
    while first < glyphs.len() {
        if !selected(request, glyphs, first)
            || filter.ignores(glyphs.get(first).ok_or_else(malformed)?)?
        {
            first += 1;
            continue;
        }
        let Some(second) = next_included(glyphs, filter, first + 1)? else {
            break;
        };
        let Some(first_coverage) = coverage_index(
            coverage,
            glyphs.get(first).ok_or_else(malformed)?.glyph_id().value(),
        )?
        else {
            first += 1;
            continue;
        };
        let Some(second_coverage) = coverage_index(
            coverage,
            glyphs.get(second).ok_or_else(malformed)?.glyph_id().value(),
        )?
        else {
            first += 1;
            continue;
        };
        if first_coverage >= count || second_coverage >= count {
            return Err(malformed());
        }
        let exit = anchor(table, 6 + usize::from(first_coverage) * 4 + 2)?;
        let entry = anchor(table, 6 + usize::from(second_coverage) * 4)?;
        if let (Some(exit), Some(entry)) = (exit, entry) {
            Attachment {
                first,
                second,
                first_anchor: exit,
                second_anchor: entry,
                direction: request.shape().direction,
                zero_second_advance: false,
            }
            .apply(glyphs)?;
        }
        first = second;
    }
    Ok(())
}

fn apply_mark_to_base(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let mark_coverage = table.offset16(2).ok_or_else(malformed)?;
    let base_coverage = table.offset16(4).ok_or_else(malformed)?;
    let class_count = usize::from(table.u16(6).ok_or_else(malformed)?);
    let mark_array = table.offset16(8).ok_or_else(malformed)?;
    let base_array = table.offset16(10).ok_or_else(malformed)?;
    for mark in 0..glyphs.len() {
        if !selected(request, glyphs, mark)
            || filter.ignores(glyphs.get(mark).ok_or_else(malformed)?)?
        {
            continue;
        }
        let Some(mark_index) = coverage_index(
            mark_coverage,
            glyphs.get(mark).ok_or_else(malformed)?.glyph_id().value(),
        )?
        else {
            continue;
        };
        let Some((base, base_index)) = previous_base(glyphs, base_coverage, filter.gdef, mark)?
        else {
            continue;
        };
        let (class, mark_anchor) = mark_record(mark_array, mark_index, class_count)?;
        let base_anchor = array_anchor(base_array, base_index, class_count, class)?;
        if let (Some(base_anchor), Some(mark_anchor)) = (base_anchor, mark_anchor) {
            Attachment {
                first: base,
                second: mark,
                first_anchor: base_anchor,
                second_anchor: mark_anchor,
                direction: request.shape().direction,
                zero_second_advance: true,
            }
            .apply(glyphs)?;
        }
    }
    Ok(())
}

fn apply_mark_to_ligature(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let mark_coverage = table.offset16(2).ok_or_else(malformed)?;
    let ligature_coverage = table.offset16(4).ok_or_else(malformed)?;
    let class_count = usize::from(table.u16(6).ok_or_else(malformed)?);
    let mark_array = table.offset16(8).ok_or_else(malformed)?;
    let ligature_array = table.offset16(10).ok_or_else(malformed)?;
    for mark in 0..glyphs.len() {
        if !selected(request, glyphs, mark)
            || filter.ignores(glyphs.get(mark).ok_or_else(malformed)?)?
        {
            continue;
        }
        let Some(mark_index) = coverage_index(
            mark_coverage,
            glyphs.get(mark).ok_or_else(malformed)?.glyph_id().value(),
        )?
        else {
            continue;
        };
        let Some((ligature, ligature_index)) =
            previous_base(glyphs, ligature_coverage, filter.gdef, mark)?
        else {
            continue;
        };
        let attach = ligature_array
            .offset16(2 + usize::from(ligature_index) * 2)
            .ok_or_else(malformed)?;
        let component_count = usize::from(attach.u16(0).ok_or_else(malformed)?);
        if component_count == 0 {
            return Err(malformed());
        }
        let (class, mark_anchor) = mark_record(mark_array, mark_index, class_count)?;
        let component = component_count - 1;
        let ligature_anchor = anchor(attach, 2 + (component * class_count + class) * 2)?;
        if let (Some(ligature_anchor), Some(mark_anchor)) = (ligature_anchor, mark_anchor) {
            Attachment {
                first: ligature,
                second: mark,
                first_anchor: ligature_anchor,
                second_anchor: mark_anchor,
                direction: request.shape().direction,
                zero_second_advance: true,
            }
            .apply(glyphs)?;
        }
    }
    Ok(())
}

fn apply_mark_to_mark(
    table: Table<'_>,
    filter: LookupFilter<'_>,
    request: LookupRequest<'_, '_>,
    glyphs: &mut GlyphBuffer<'_>,
) -> Result<(), ShapeError> {
    if table.u16(0) != Some(1) {
        return Err(malformed());
    }
    let mark1_coverage = table.offset16(2).ok_or_else(malformed)?;
    let mark2_coverage = table.offset16(4).ok_or_else(malformed)?;
    let class_count = usize::from(table.u16(6).ok_or_else(malformed)?);
    let mark1_array = table.offset16(8).ok_or_else(malformed)?;
    let mark2_array = table.offset16(10).ok_or_else(malformed)?;
    for mark1 in 0..glyphs.len() {
        if !selected(request, glyphs, mark1)
            || filter.ignores(glyphs.get(mark1).ok_or_else(malformed)?)?
        {
            continue;
        }
        let Some(mark1_index) = coverage_index(
            mark1_coverage,
            glyphs.get(mark1).ok_or_else(malformed)?.glyph_id().value(),
        )?
        else {
            continue;
        };
        let Some((mark2, mark2_index)) = previous_mark(glyphs, mark2_coverage, filter, mark1)?
        else {
            continue;
        };
        let (class, mark1_anchor) = mark_record(mark1_array, mark1_index, class_count)?;
        let mark2_anchor = array_anchor(mark2_array, mark2_index, class_count, class)?;
        if let (Some(mark2_anchor), Some(mark1_anchor)) = (mark2_anchor, mark1_anchor) {
            Attachment {
                first: mark2,
                second: mark1,
                first_anchor: mark2_anchor,
                second_anchor: mark1_anchor,
                direction: request.shape().direction,
                zero_second_advance: true,
            }
            .apply(glyphs)?;
        }
    }
    Ok(())
}

fn previous_base(
    glyphs: &GlyphBuffer<'_>,
    coverage: Table<'_>,
    gdef: Option<Table<'_>>,
    mut index: usize,
) -> Result<Option<(usize, u16)>, ShapeError> {
    while index > 0 {
        index -= 1;
        let glyph = glyphs.get(index).ok_or_else(malformed)?;
        if glyph_is_mark(gdef, glyph.glyph_id().value())? {
            continue;
        }
        return Ok(coverage_index(coverage, glyph.glyph_id().value())?
            .map(|coverage_index| (index, coverage_index)));
    }
    Ok(None)
}

fn previous_mark(
    glyphs: &GlyphBuffer<'_>,
    coverage: Table<'_>,
    filter: LookupFilter<'_>,
    index: usize,
) -> Result<Option<(usize, u16)>, ShapeError> {
    let Some(index) = previous_included(glyphs, filter, index)? else {
        return Ok(None);
    };
    coverage_index(
        coverage,
        glyphs.get(index).ok_or_else(malformed)?.glyph_id().value(),
    )
    .map(|coverage_index| coverage_index.map(|coverage_index| (index, coverage_index)))
}

fn mark_record(
    array: Table<'_>,
    index: u16,
    class_count: usize,
) -> Result<(usize, Option<textflow::shaping::FlowPoint>), ShapeError> {
    let count = array.u16(0).ok_or_else(malformed)?;
    if index >= count {
        return Err(malformed());
    }
    let record = 2 + usize::from(index) * 4;
    let class = usize::from(array.u16(record).ok_or_else(malformed)?);
    if class >= class_count {
        return Err(malformed());
    }
    Ok((class, anchor(array, record + 2)?))
}

fn array_anchor(
    array: Table<'_>,
    index: u16,
    class_count: usize,
    class: usize,
) -> Result<Option<textflow::shaping::FlowPoint>, ShapeError> {
    let count = array.u16(0).ok_or_else(malformed)?;
    if index >= count || class >= class_count {
        return Err(malformed());
    }
    anchor(array, 2 + (usize::from(index) * class_count + class) * 2)
}

fn anchor(
    parent: Table<'_>,
    offset: usize,
) -> Result<Option<textflow::shaping::FlowPoint>, ShapeError> {
    let offset = parent.u16(offset).ok_or_else(malformed)?;
    if offset == 0 {
        return Ok(None);
    }
    let anchor = parent.tail(usize::from(offset)).ok_or_else(malformed)?;
    if !matches!(anchor.u16(0), Some(1..=3)) {
        return Err(malformed());
    }
    Ok(Some(textflow::shaping::FlowPoint {
        x: i32::from(anchor.i16(2).ok_or_else(malformed)?) << 8,
        y: i32::from(anchor.i16(4).ok_or_else(malformed)?) << 8,
    }))
}

struct Attachment {
    first: usize,
    second: usize,
    first_anchor: textflow::shaping::FlowPoint,
    second_anchor: textflow::shaping::FlowPoint,
    direction: Direction,
    zero_second_advance: bool,
}

impl Attachment {
    fn apply(self, glyphs: &mut GlyphBuffer<'_>) -> Result<(), ShapeError> {
        let first_offset = glyphs.get(self.first).ok_or_else(malformed)?.offset;
        if self.zero_second_advance {
            glyphs.get_mut(self.second).ok_or_else(malformed)?.advance =
                textflow::shaping::FlowPoint { x: 0, y: 0 };
        }
        let mut pen = textflow::shaping::FlowPoint { x: 0, y: 0 };
        let pen_range = match self.direction {
            Direction::LeftToRight => self.first..self.second,
            Direction::RightToLeft => self.first + 1..self.second + 1,
        };
        for glyph in &glyphs.glyphs()[pen_range] {
            pen.x = pen.x.checked_add(glyph.advance.x).ok_or_else(malformed)?;
            pen.y = pen.y.checked_add(glyph.advance.y).ok_or_else(malformed)?;
        }
        let second = glyphs.get_mut(self.second).ok_or_else(malformed)?;
        let x = first_offset
            .x
            .checked_add(self.first_anchor.x)
            .and_then(|value| value.checked_sub(self.second_anchor.x))
            .ok_or_else(malformed)?;
        let y = first_offset
            .y
            .checked_add(self.first_anchor.y)
            .and_then(|value| value.checked_sub(self.second_anchor.y))
            .ok_or_else(malformed)?;
        match self.direction {
            Direction::LeftToRight => {
                second.offset.x = x.checked_sub(pen.x).ok_or_else(malformed)?;
                second.offset.y = y.checked_sub(pen.y).ok_or_else(malformed)?;
            }
            Direction::RightToLeft => {
                second.offset.x = x.checked_add(pen.x).ok_or_else(malformed)?;
                second.offset.y = y.checked_add(pen.y).ok_or_else(malformed)?;
            }
        }
        second.set_unsafe_to_break(true);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum LayoutKind {
    Substitution,
    Positioning,
}

#[derive(Clone, Copy)]
struct RequestedFeatureApplication<'table, 'request, 'text> {
    feature_list: Table<'table>,
    lookup_list: Table<'table>,
    gdef: Option<Table<'table>>,
    kind: LayoutKind,
    request: LookupRequest<'request, 'text>,
}

impl RequestedFeatureApplication<'_, '_, '_> {
    fn apply(self, feature_index: u16, glyphs: &mut GlyphBuffer<'_>) -> Result<bool, ShapeError> {
        let count = self.feature_list.u16(0).ok_or_else(malformed)?;
        if feature_index >= count {
            return Err(malformed());
        }
        let record = 2 + usize::from(feature_index) * 6;
        if self.feature_list.tag(record).ok_or_else(malformed)? != self.request.feature() {
            return Ok(false);
        }
        let feature = self
            .feature_list
            .offset16(record + 4)
            .ok_or_else(malformed)?;
        let lookup_count = feature.u16(2).ok_or_else(malformed)?;
        for index in 0..lookup_count {
            let lookup_index = feature
                .u16(4 + usize::from(index) * 2)
                .ok_or_else(malformed)?;
            let lookup = lookup_at(self.lookup_list, lookup_index)?;
            match self.kind {
                LayoutKind::Substitution => apply_substitution_lookup_buffer(
                    lookup,
                    self.lookup_list,
                    self.gdef,
                    self.request,
                    glyphs,
                    0,
                )?,
                LayoutKind::Positioning => {
                    apply_positioning_lookup_buffer(lookup, self.gdef, self.request, glyphs)?;
                }
            }
        }
        Ok(true)
    }
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
    let Some(langsys) = script_list.langsys(script_tag(request.script), request.language)? else {
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

fn mark_filter_set(gdef: Table<'_>, index: u16) -> Result<Table<'_>, ShapeError> {
    if gdef.u16(0) != Some(1) || gdef.u16(2).ok_or_else(malformed)? < 2 {
        return Err(malformed());
    }
    let sets_offset = usize::from(gdef.u16(12).ok_or_else(malformed)?);
    if sets_offset == 0 {
        return Err(malformed());
    }
    let sets = gdef.tail(sets_offset).ok_or_else(malformed)?;
    if sets.u16(0) != Some(1) || index >= sets.u16(2).ok_or_else(malformed)? {
        return Err(malformed());
    }
    sets.offset32(4 + usize::from(index) * 4)
        .ok_or_else(malformed)
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

    fn langsys(
        self,
        requested: [u8; 4],
        language: Option<&str>,
    ) -> Result<Option<Self>, ShapeError> {
        let count = self.u16(0).ok_or_else(malformed)?;
        let mut fallback = None;
        for index in 0..count {
            let record = 2 + usize::from(index) * 6;
            let tag = self.tag(record).ok_or_else(malformed)?;
            let script = self.offset16(record + 4).ok_or_else(malformed)?;
            if tag == requested {
                return script.language_system(language);
            }
            if tag == *b"DFLT" {
                fallback = Some(script);
            }
        }
        match fallback {
            Some(script) => script.language_system(language),
            None => Ok(None),
        }
    }

    fn language_system(self, language: Option<&str>) -> Result<Option<Self>, ShapeError> {
        if let Some(requested) = language_tag(language) {
            let count = self.u16(2).ok_or_else(malformed)?;
            for index in 0..count {
                let record = 4 + usize::from(index) * 6;
                if self.tag(record).ok_or_else(malformed)? == requested {
                    return self
                        .tail(usize::from(self.u16(record + 4).ok_or_else(malformed)?))
                        .ok_or_else(malformed)
                        .map(Some);
                }
            }
        }
        self.default_langsys()
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

fn language_tag(language: Option<&str>) -> Option<[u8; 4]> {
    let language = language?.split(['-', '_']).next().unwrap_or_default();
    if language.eq_ignore_ascii_case("ar") {
        Some(*b"ARA ")
    } else if language.eq_ignore_ascii_case("fa") {
        Some(*b"FAR ")
    } else if language.eq_ignore_ascii_case("ur") {
        Some(*b"URD ")
    } else if language.eq_ignore_ascii_case("th") {
        Some(*b"THA ")
    } else {
        None
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
        assert_eq!(language_tag(Some("ar")), Some(*b"ARA "));
        assert_eq!(language_tag(Some("fa-IR")), Some(*b"FAR "));
        assert_eq!(language_tag(Some("ur_PK")), Some(*b"URD "));
        assert_eq!(language_tag(Some("th")), Some(*b"THA "));
        assert_eq!(language_tag(Some("en")), None);
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
    fn multiple_substitution_expands_inside_caller_storage() {
        let table = [0, 1, 0, 8, 0, 1, 0, 14, 0, 1, 0, 1, 0, 4, 0, 2, 0, 7, 0, 8];
        let mut storage = [glyph(4, 0, 1), glyph(0, 0, 0), glyph(0, 0, 0)];
        let mut glyphs = GlyphBuffer::new(&mut storage, 1).unwrap();
        glyphs.set_mask(0, GlyphMask::new(3)).unwrap();

        assert_eq!(apply_multiple_at(Table::new(&table), &mut glyphs, 0), Ok(2));
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs.get(0).unwrap().glyph_id(), GlyphId::new(7));
        assert_eq!(glyphs.get(1).unwrap().glyph_id(), GlyphId::new(8));
        assert!(
            glyphs
                .glyphs()
                .iter()
                .all(|glyph| { glyph.cluster == TextRange::new(0, 1) && glyph.unsafe_to_break() })
        );
        assert!((0..2).all(|index| glyphs.mask(index) == Some(GlyphMask::new(3))));
    }

    #[test]
    fn attachment_uses_directional_pen_coordinates() {
        let mut ltr_storage = [glyph(4, 0, 2), glyph(5, 0, 2)];
        let mut ltr = GlyphBuffer::new(&mut ltr_storage, 2).unwrap();
        Attachment {
            first: 0,
            second: 1,
            first_anchor: FlowPoint { x: 20 << 8, y: 0 },
            second_anchor: FlowPoint { x: 5 << 8, y: 0 },
            direction: Direction::LeftToRight,
            zero_second_advance: true,
        }
        .apply(&mut ltr)
        .unwrap();
        assert_eq!(ltr.get(1).unwrap().offset.x, -85 << 8);
        assert_eq!(ltr.get(1).unwrap().advance.x, 0);

        let mut rtl_storage = [glyph(4, 0, 2), glyph(5, 0, 2)];
        let mut rtl = GlyphBuffer::new(&mut rtl_storage, 2).unwrap();
        Attachment {
            first: 0,
            second: 1,
            first_anchor: FlowPoint { x: 20 << 8, y: 0 },
            second_anchor: FlowPoint { x: 5 << 8, y: 0 },
            direction: Direction::RightToLeft,
            zero_second_advance: true,
        }
        .apply(&mut rtl)
        .unwrap();
        assert_eq!(rtl.get(1).unwrap().offset.x, 15 << 8);
        assert_eq!(rtl.get(1).unwrap().advance.x, 0);
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
    fn mark_filtering_set_selects_only_covered_marks() {
        let lookup = [0, 1, 0, 16, 0, 0, 0, 0];
        let gdef = [
            0, 1, 0, 2, 0, 14, 0, 0, 0, 0, 0, 0, 0, 24, 0, 1, 0, 5, 0, 2, 0, 3, 0, 3, 0, 1, 0, 1,
            0, 0, 0, 8, 0, 1, 0, 1, 0, 5,
        ];
        let filter =
            LookupFilter::parse(Table::new(&lookup), Some(Table::new(&gdef)), *b"mark").unwrap();

        assert!(!filter.ignores(&glyph(4, 0, 1)).unwrap());
        assert!(!filter.ignores(&glyph(5, 0, 1)).unwrap());
        assert!(filter.ignores(&glyph(6, 0, 1)).unwrap());
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
