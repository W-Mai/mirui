use alloc::vec::Vec;
use core::cell::{Ref, RefCell, RefMut};
use core::mem::size_of;

use textflow::TextFlow;
use textflow::bidi::{BaseDirection, BidiError};
use textflow::layout::{
    Alignment, LayoutError, LayoutLine, Overflow, TextSpacing, VisualRun, WrapMode,
};
use textflow::shaping::{CaretStop, FlowPoint, FontFeature, PositionedGlyph, Typeface};
use textflow::workspace::{
    LayoutLimits as FlowLayoutLimits, LayoutOutput as FlowLayoutOutput, TextWorkspace,
    WorkspaceError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextLayoutLimits {
    pub text_bytes: usize,
    pub clusters: usize,
    pub glyphs: usize,
    pub lines: usize,
    pub fallback_faces: usize,
    pub cache_bytes: usize,
}

impl TextLayoutLimits {
    pub const EMBEDDED: Self = Self {
        text_bytes: 2_048,
        clusters: 1_024,
        glyphs: 2_048,
        lines: 128,
        fallback_faces: 8,
        cache_bytes: 32 * 1_024,
    };

    pub const HOST: Self = Self {
        text_bytes: 4 * 1_024 * 1_024,
        clusters: 2_000_000,
        glyphs: 4_000_000,
        lines: 65_535,
        fallback_faces: 64,
        cache_bytes: 128 * 1_024 * 1_024,
    };

    pub const fn with_cache_bytes(mut self, cache_bytes: usize) -> Self {
        self.cache_bytes = cache_bytes;
        self
    }
}

impl Default for TextLayoutLimits {
    fn default() -> Self {
        if cfg!(feature = "std") {
            Self::HOST
        } else {
            Self::EMBEDDED
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::Component)]
pub struct TextLayoutHandle {
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextMeasure {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutError {
    TextLimit { required: usize, limit: usize },
    ClusterLimit { required: usize, limit: usize },
    TypefaceLimit { required: usize, limit: usize },
    CacheBudget { required: usize, budget: usize },
    DimensionOverflow,
    Allocation,
    Bidi(BidiError),
    Layout(LayoutError),
}

pub struct TextLayout<'a> {
    lines: &'a [LayoutLine],
    runs: &'a [VisualRun],
    glyphs: &'a [PositionedGlyph],
    carets: &'a [CaretStop],
    measure: TextMeasure,
}

impl TextLayout<'_> {
    pub const fn lines(&self) -> &[LayoutLine] {
        self.lines
    }

    #[cfg(test)]
    pub(crate) const fn runs(&self) -> &[VisualRun] {
        self.runs
    }

    pub(crate) const fn glyphs(&self) -> &[PositionedGlyph] {
        self.glyphs
    }

    pub(crate) fn paragraph(&self) -> Result<textflow::layout::ParagraphLayout<'_>, LayoutError> {
        textflow::layout::ParagraphLayout::from_slices(
            self.lines,
            self.runs,
            self.glyphs,
            self.carets,
        )
    }

    pub const fn carets(&self) -> &[CaretStop] {
        self.carets
    }

    pub const fn measure(&self) -> TextMeasure {
        self.measure
    }

    pub fn runs_for(&self, line: LayoutLine) -> Option<&[VisualRun]> {
        let range = line.runs();
        self.runs.get(range.start as usize..range.end as usize)
    }

    pub fn glyphs_for(&self, run: VisualRun) -> Option<&[PositionedGlyph]> {
        let range = run.glyphs();
        self.glyphs.get(range.start as usize..range.end as usize)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TextLayoutRequest<'a> {
    pub text: &'a str,
    pub max_width: i32,
    pub width: Option<i32>,
    pub max_lines: usize,
    pub line_height: i32,
    pub baseline: i32,
    pub direction: BaseDirection,
    pub wrap: WrapMode,
    pub alignment: Alignment,
    pub overflow: Overflow,
    pub spacing: TextSpacing,
    pub features: &'a [FontFeature],
    pub line_widths: Option<&'a dyn textflow::layout::LineWidthProvider>,
}

pub struct TextLayoutCache {
    limits: TextLayoutLimits,
    frame: u32,
    next_order: u64,
    entries: Vec<Option<LayoutEntry>>,
    slot_generations: Vec<u32>,
    measurements: Vec<MeasureEntry>,
    workspace: TextWorkspace,
    output_slots: OutputStarts,
    lines: Vec<LayoutLine>,
    runs: Vec<VisualRun>,
    glyphs: Vec<PositionedGlyph>,
    carets: Vec<CaretStop>,
}

pub(crate) struct TextLayoutResource(RefCell<TextLayoutCache>);

impl TextLayoutResource {
    pub const fn new(limits: TextLayoutLimits) -> Self {
        Self(RefCell::new(TextLayoutCache::new(limits)))
    }

    pub fn borrow(&self) -> Ref<'_, TextLayoutCache> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, TextLayoutCache> {
        self.0.borrow_mut()
    }
}

impl TextLayoutCache {
    pub const fn new(limits: TextLayoutLimits) -> Self {
        Self {
            limits,
            frame: 0,
            next_order: 0,
            entries: Vec::new(),
            slot_generations: Vec::new(),
            measurements: Vec::new(),
            workspace: TextWorkspace::new(FlowLayoutLimits {
                text_bytes: limits.text_bytes,
                runs: limits.clusters,
                glyphs: limits.glyphs,
                scratch_glyphs: limits.glyphs,
                lines: limits.lines,
                memory_bytes: limits.cache_bytes,
            }),
            output_slots: OutputStarts::ZERO,
            lines: Vec::new(),
            runs: Vec::new(),
            glyphs: Vec::new(),
            carets: Vec::new(),
        }
    }

    pub const fn limits(&self) -> TextLayoutLimits {
        self.limits
    }

    pub fn resident_bytes(&self) -> usize {
        self.non_workspace_bytes()
            .saturating_add(self.workspace.resident_bytes())
    }

    fn non_workspace_bytes(&self) -> usize {
        vec_bytes(&self.entries)
            .saturating_add(vec_bytes(&self.slot_generations))
            .saturating_add(vec_bytes(&self.measurements))
            .saturating_add(vec_bytes(&self.lines))
            .saturating_add(vec_bytes(&self.runs))
            .saturating_add(vec_bytes(&self.glyphs))
            .saturating_add(vec_bytes(&self.carets))
    }

    pub fn begin_frame(&mut self) {
        if self.frame != 0 {
            self.retain_previous_frame();
        }
        self.frame = self.frame.wrapping_add(1).max(1);
    }

    pub fn get(&self, handle: TextLayoutHandle) -> Option<TextLayout<'_>> {
        let slot = handle.slot as usize;
        if self.slot_generations.get(slot).copied()? != handle.generation {
            return None;
        }
        let entry = self.entries.get(slot)?.as_ref()?;
        Some(TextLayout {
            lines: self.lines.get(entry.lines.clone())?,
            runs: self.runs.get(entry.runs.clone())?,
            glyphs: self.glyphs.get(entry.glyphs.clone())?,
            carets: self.carets.get(entry.carets.clone())?,
            measure: entry.measure,
        })
    }

    pub(crate) fn measure(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextMeasure, TextLayoutError> {
        let starts = self.output_starts();
        let result = self.run(request, typefaces);
        self.truncate_outputs(starts);
        result.map(|output| output.measure)
    }

    pub(crate) fn measure_cached(
        &mut self,
        owner: u64,
        font_fingerprint: u64,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextMeasure, TextLayoutError> {
        let key = LayoutKey::new(owner, font_fingerprint, request);
        if let Some(entry) = self.measurements.iter_mut().find(|entry| entry.key == key) {
            entry.last_used = self.frame;
            return Ok(entry.measure);
        }
        let measure = self.measure(request, typefaces)?;
        self.reserve_measurements(self.measurements.len().saturating_add(1))?;
        self.measurements.push(MeasureEntry {
            key,
            measure,
            last_used: self.frame,
        });
        Ok(measure)
    }

    #[cfg(test)]
    pub(crate) fn layout(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextLayoutHandle, TextLayoutError> {
        self.layout_new(None, request, typefaces)
    }

    pub(crate) fn layout_cached(
        &mut self,
        owner: u64,
        font_fingerprint: u64,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextLayoutHandle, TextLayoutError> {
        let key = LayoutKey::new(owner, font_fingerprint, request);
        if let Some((slot, entry)) =
            self.entries
                .iter_mut()
                .enumerate()
                .find_map(|(slot, entry)| {
                    entry
                        .as_mut()
                        .filter(|entry| entry.key == Some(key))
                        .map(|entry| (slot, entry))
                })
        {
            entry.last_used = self.frame;
            return Ok(TextLayoutHandle {
                slot: slot as u32,
                generation: self.slot_generations[slot],
            });
        }
        self.layout_new(Some(key), request, typefaces)
    }

    fn layout_new(
        &mut self,
        key: Option<LayoutKey>,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextLayoutHandle, TextLayoutError> {
        self.validate_request(request, typefaces.len())?;
        let vacant = self.entries.iter().position(Option::is_none);
        if vacant.is_none() {
            self.reserve_entries(self.entries.len().saturating_add(1))?;
            self.reserve_slot_generations(self.slot_generations.len().saturating_add(1))?;
        }
        let starts = self.output_starts();
        let output = match self.run(request, typefaces) {
            Ok(output) => output,
            Err(error) => {
                self.truncate_outputs(starts);
                return Err(error);
            }
        };
        let slot = vacant.unwrap_or(self.entries.len());
        let entry = LayoutEntry {
            key,
            last_used: self.frame,
            order: self.next_order,
            lines: starts.lines..self.lines.len(),
            runs: starts.runs..self.runs.len(),
            glyphs: starts.glyphs..self.glyphs.len(),
            carets: starts.carets..self.carets.len(),
            measure: output.measure,
        };
        self.next_order = self.next_order.wrapping_add(1);
        if slot == self.entries.len() {
            self.entries.push(Some(entry));
            self.slot_generations.push(0);
        } else {
            self.entries[slot] = Some(entry);
        }
        Ok(TextLayoutHandle {
            slot: slot as u32,
            generation: self.slot_generations[slot],
        })
    }

    fn run(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<LayoutOutput, TextLayoutError> {
        self.validate_request(request, typefaces.len())?;
        self.prepare_output_slots(request.text);
        for _ in 0..10 {
            let starts = self.output_starts();
            match self.try_layout(request, typefaces) {
                Ok(output) => return Ok(output),
                Err(AttemptError::Grow(kind, required)) => {
                    self.truncate_outputs(starts);
                    self.grow(kind, required)?;
                }
                Err(AttemptError::Public(error)) => {
                    self.truncate_outputs(starts);
                    return Err(error);
                }
            }
        }
        Err(TextLayoutError::DimensionOverflow)
    }

    fn try_layout(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<LayoutOutput, AttemptError> {
        let starts = self.output_starts();
        self.prepare_output_storage(starts)?;
        let non_workspace = self.non_workspace_bytes();
        let workspace_budget =
            self.limits
                .cache_bytes
                .checked_sub(non_workspace)
                .ok_or(AttemptError::Public(TextLayoutError::CacheBudget {
                    required: self.resident_bytes(),
                    budget: self.limits.cache_bytes,
                }))?;
        self.workspace
            .set_memory_limit(workspace_budget)
            .map_err(|error| map_workspace(error, non_workspace, self.limits.cache_bytes))?;

        let max_width = usize::try_from(request.max_width)
            .map_err(|_| AttemptError::Public(TextLayoutError::DimensionOverflow))?;
        let line_height = usize::try_from(request.line_height)
            .map_err(|_| AttemptError::Public(TextLayoutError::DimensionOverflow))?;
        let mut flow = TextFlow::new(request.text, max_width)
            .with_line_height(line_height)
            .with_direction(request.direction)
            .with_features(request.features)
            .with_origin(FlowPoint {
                x: 0,
                y: request.baseline,
            })
            .with_wrap(request.wrap)
            .with_letter_spacing(request.spacing.letter)
            .with_word_spacing(request.spacing.word)
            .with_alignment(request.alignment)
            .with_max_lines(request.max_lines)
            .with_overflow(request.overflow);
        if let Some(widths) = request.line_widths {
            flow = flow.with_line_widths(widths);
        }
        if let Some(width) = request.width {
            flow = flow.with_width(
                usize::try_from(width)
                    .map_err(|_| AttemptError::Public(TextLayoutError::DimensionOverflow))?,
            );
        } else {
            flow = flow.without_width();
        }
        let (counts, measure) = {
            let mut output = FlowLayoutOutput::new(
                &mut self.glyphs[starts.glyphs..],
                &mut self.runs[starts.runs..],
                &mut self.lines[starts.lines..],
                &mut self.carets[starts.carets..],
            );
            let paragraph = flow
                .layout_into(typefaces, &mut self.workspace, &mut output)
                .map_err(|error| map_workspace(error, non_workspace, self.limits.cache_bytes))?;
            let visible = paragraph.lines();
            (
                visible_counts(visible),
                measure(visible, request.line_height)?,
            )
        };
        self.truncate_outputs(starts.add(counts));
        Ok(LayoutOutput { measure })
    }

    fn validate_request(
        &self,
        request: TextLayoutRequest<'_>,
        typeface_count: usize,
    ) -> Result<(), TextLayoutError> {
        if request.text.len() > self.limits.text_bytes {
            return Err(TextLayoutError::TextLimit {
                required: request.text.len(),
                limit: self.limits.text_bytes,
            });
        }
        let clusters = request.text.chars().count();
        if clusters > self.limits.clusters {
            return Err(TextLayoutError::ClusterLimit {
                required: clusters,
                limit: self.limits.clusters,
            });
        }
        if typeface_count == 0 || typeface_count > self.limits.fallback_faces {
            return Err(TextLayoutError::TypefaceLimit {
                required: typeface_count,
                limit: self.limits.fallback_faces,
            });
        }
        if request.max_width < 0 || request.line_height < 0 {
            return Err(TextLayoutError::DimensionOverflow);
        }
        Ok(())
    }

    fn prepare_output_storage(&mut self, starts: OutputStarts) -> Result<(), AttemptError> {
        self.resize_output(BufferKind::PositionedGlyphs, starts.glyphs)?;
        self.resize_output(BufferKind::VisualRuns, starts.runs)?;
        self.resize_output(BufferKind::LayoutLines, starts.lines)?;
        self.resize_output(BufferKind::Carets, starts.carets)?;
        Ok(())
    }

    fn resize_output(&mut self, kind: BufferKind, start: usize) -> Result<(), AttemptError> {
        let slots = self.output_slots.get(kind);
        let required = start
            .checked_add(slots)
            .ok_or(AttemptError::Public(TextLayoutError::DimensionOverflow))?;
        self.ensure(kind, required).map_err(AttemptError::Public)
    }

    fn grow(&mut self, kind: BufferKind, required: usize) -> Result<(), TextLayoutError> {
        let limit = kind.limit(self.limits);
        if required > limit {
            return Err(kind.limit_error(required, limit));
        }
        self.output_slots.set(kind, required);
        Ok(())
    }

    fn ensure(&mut self, kind: BufferKind, required: usize) -> Result<(), TextLayoutError> {
        let resident = self.resident_bytes();
        match kind {
            BufferKind::PositionedGlyphs => reserve_slots(
                &mut self.glyphs,
                required,
                PositionedGlyph::default(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::VisualRuns => reserve_slots(
                &mut self.runs,
                required,
                VisualRun::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::LayoutLines => reserve_slots(
                &mut self.lines,
                required,
                LayoutLine::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::Carets => reserve_slots(
                &mut self.carets,
                required,
                CaretStop::default(),
                resident,
                self.limits.cache_bytes,
            ),
        }
    }

    fn prepare_output_slots(&mut self, text: &str) {
        let clusters = text.chars().count();
        let glyphs = clusters.saturating_mul(2).min(self.limits.glyphs);
        let lines = if clusters == 0 {
            0
        } else {
            clusters.saturating_add(1).min(self.limits.lines)
        };
        self.output_slots = OutputStarts {
            lines,
            runs: clusters.saturating_mul(2).min(self.limits.glyphs),
            glyphs,
            carets: glyphs
                .saturating_add(lines)
                .min(self.limits.glyphs.saturating_add(self.limits.lines)),
        };
    }

    fn reserve_entries(&mut self, required: usize) -> Result<(), TextLayoutError> {
        let resident = self.resident_bytes();
        reserve_capacity(
            &mut self.entries,
            required,
            resident,
            self.limits.cache_bytes,
        )
    }

    fn reserve_slot_generations(&mut self, required: usize) -> Result<(), TextLayoutError> {
        let resident = self.resident_bytes();
        reserve_capacity(
            &mut self.slot_generations,
            required,
            resident,
            self.limits.cache_bytes,
        )
    }

    fn reserve_measurements(&mut self, required: usize) -> Result<(), TextLayoutError> {
        let resident = self.resident_bytes();
        reserve_capacity(
            &mut self.measurements,
            required,
            resident,
            self.limits.cache_bytes,
        )
    }

    fn retain_previous_frame(&mut self) {
        let frame = self.frame;
        for (slot, entry) in self.entries.iter_mut().enumerate() {
            let keep = entry
                .as_ref()
                .is_some_and(|entry| entry.key.is_some() && entry.last_used == frame);
            if !keep && entry.take().is_some() {
                self.slot_generations[slot] = self.slot_generations[slot].wrapping_add(1);
            }
        }
        self.measurements.retain(|entry| entry.last_used == frame);

        let mut lines = 0;
        let mut runs = 0;
        let mut glyphs = 0;
        let mut carets = 0;
        let mut compacted_order = 0;
        let mut last_source_order = None;
        loop {
            let next = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(slot, entry)| {
                    let entry = entry.as_ref()?;
                    if last_source_order.is_some_and(|last| entry.order <= last) {
                        return None;
                    }
                    Some((slot, entry.order))
                })
                .min_by_key(|(_, order)| *order);
            let Some((slot, source_order)) = next else {
                break;
            };
            let entry = self.entries[slot].as_ref().unwrap();
            let old_lines = entry.lines.clone();
            let old_runs = entry.runs.clone();
            let old_glyphs = entry.glyphs.clone();
            let old_carets = entry.carets.clone();
            let line_count = old_lines.len();
            let run_count = old_runs.len();
            let glyph_count = old_glyphs.len();
            let caret_count = old_carets.len();
            self.lines.copy_within(old_lines, lines);
            self.runs.copy_within(old_runs, runs);
            self.glyphs.copy_within(old_glyphs, glyphs);
            self.carets.copy_within(old_carets, carets);
            let entry = self.entries[slot].as_mut().unwrap();
            entry.lines = lines..lines + line_count;
            entry.runs = runs..runs + run_count;
            entry.glyphs = glyphs..glyphs + glyph_count;
            entry.carets = carets..carets + caret_count;
            entry.order = compacted_order;
            lines += line_count;
            runs += run_count;
            glyphs += glyph_count;
            carets += caret_count;
            compacted_order += 1;
            last_source_order = Some(source_order);
        }
        self.lines.truncate(lines);
        self.runs.truncate(runs);
        self.glyphs.truncate(glyphs);
        self.carets.truncate(carets);
        self.next_order = compacted_order;
    }

    fn output_starts(&self) -> OutputStarts {
        OutputStarts {
            lines: self.lines.len(),
            runs: self.runs.len(),
            glyphs: self.glyphs.len(),
            carets: self.carets.len(),
        }
    }

    fn truncate_outputs(&mut self, starts: OutputStarts) {
        self.lines.truncate(starts.lines);
        self.runs.truncate(starts.runs);
        self.glyphs.truncate(starts.glyphs);
        self.carets.truncate(starts.carets);
    }
}

impl Default for TextLayoutCache {
    fn default() -> Self {
        Self::new(TextLayoutLimits::default())
    }
}

#[derive(Clone, Copy)]
struct OutputStarts {
    lines: usize,
    runs: usize,
    glyphs: usize,
    carets: usize,
}

impl OutputStarts {
    const ZERO: Self = Self {
        lines: 0,
        runs: 0,
        glyphs: 0,
        carets: 0,
    };

    fn add(self, counts: Self) -> Self {
        Self {
            lines: self.lines + counts.lines,
            runs: self.runs + counts.runs,
            glyphs: self.glyphs + counts.glyphs,
            carets: self.carets + counts.carets,
        }
    }

    const fn get(self, kind: BufferKind) -> usize {
        match kind {
            BufferKind::PositionedGlyphs => self.glyphs,
            BufferKind::VisualRuns => self.runs,
            BufferKind::LayoutLines => self.lines,
            BufferKind::Carets => self.carets,
        }
    }

    fn set(&mut self, kind: BufferKind, required: usize) {
        match kind {
            BufferKind::PositionedGlyphs => self.glyphs = required,
            BufferKind::VisualRuns => self.runs = required,
            BufferKind::LayoutLines => self.lines = required,
            BufferKind::Carets => self.carets = required,
        }
    }
}

struct LayoutEntry {
    key: Option<LayoutKey>,
    last_used: u32,
    order: u64,
    lines: core::ops::Range<usize>,
    runs: core::ops::Range<usize>,
    glyphs: core::ops::Range<usize>,
    carets: core::ops::Range<usize>,
    measure: TextMeasure,
}

struct MeasureEntry {
    key: LayoutKey,
    measure: TextMeasure,
    last_used: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct LayoutKey {
    owner: u64,
    first: u64,
    second: u64,
}

impl LayoutKey {
    fn new(owner: u64, font_fingerprint: u64, request: TextLayoutRequest<'_>) -> Self {
        let mut hash = Fingerprint::new();
        hash.write(request.text.as_bytes());
        hash.write_i32(request.max_width);
        hash.write_i32(request.width.unwrap_or(i32::MIN));
        hash.write_usize(request.max_lines);
        hash.write_i32(request.line_height);
        hash.write_i32(request.baseline);
        hash.write_u8(match request.direction {
            BaseDirection::Auto => 0,
            BaseDirection::LeftToRight => 1,
            BaseDirection::RightToLeft => 2,
        });
        hash.write_u8(match request.wrap {
            WrapMode::NoWrap => 0,
            WrapMode::Word => 1,
            WrapMode::Grapheme => 2,
            WrapMode::WordOrGrapheme => 3,
        });
        hash.write_u8(match request.alignment {
            Alignment::Start => 0,
            Alignment::Center => 1,
            Alignment::End => 2,
            Alignment::Justify => 3,
        });
        hash.write_u8(match request.overflow {
            Overflow::Clip => 0,
            Overflow::Ellipsis => 1,
        });
        hash.write_i32(request.spacing.letter);
        hash.write_i32(request.spacing.word);
        hash.write_usize(request.features.len());
        for feature in request.features {
            hash.write(&feature.tag);
            hash.write_u32(feature.value);
            hash.write_u32(feature.range.start);
            hash.write_u32(feature.range.end);
        }
        if let Some(widths) = request.line_widths {
            hash.write_u8(1);
            let line_count = widths.line_count().min(request.max_lines);
            hash.write_usize(line_count);
            for line in 0..line_count {
                hash.write_usize(widths.width(line).unwrap_or(usize::MAX));
            }
        } else {
            hash.write_u8(0);
            hash.write_usize(0);
        }
        hash.write_u64(font_fingerprint);
        Self {
            owner,
            first: hash.first,
            second: hash.second,
        }
    }
}

struct Fingerprint {
    first: u64,
    second: u64,
}

impl Fingerprint {
    const fn new() -> Self {
        Self {
            first: 0xcbf2_9ce4_8422_2325,
            second: 0x8422_2325_cbf2_9ce4,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.first ^= u64::from(*byte);
            self.first = self.first.wrapping_mul(0x100_0000_01b3);
            self.second ^= u64::from(*byte);
            self.second = self.second.wrapping_mul(0x9e37_79b1_85eb_ca87);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

struct LayoutOutput {
    measure: TextMeasure,
}

enum AttemptError {
    Grow(BufferKind, usize),
    Public(TextLayoutError),
}

#[derive(Clone, Copy)]
enum BufferKind {
    PositionedGlyphs,
    VisualRuns,
    LayoutLines,
    Carets,
}

impl BufferKind {
    const fn limit(self, limits: TextLayoutLimits) -> usize {
        match self {
            Self::PositionedGlyphs | Self::VisualRuns => limits.glyphs,
            Self::LayoutLines => limits.lines,
            Self::Carets => limits.glyphs + limits.lines,
        }
    }

    const fn limit_error(self, required: usize, _limit: usize) -> TextLayoutError {
        match self {
            Self::LayoutLines => {
                TextLayoutError::Layout(LayoutError::InsufficientLineCapacity { required })
            }
            Self::PositionedGlyphs | Self::VisualRuns | Self::Carets => {
                TextLayoutError::Layout(LayoutError::InsufficientGlyphCapacity {
                    minimum: required,
                })
            }
        }
    }
}

fn map_workspace(error: WorkspaceError, non_workspace: usize, budget: usize) -> AttemptError {
    match error {
        WorkspaceError::Allocation => AttemptError::Public(TextLayoutError::Allocation),
        WorkspaceError::MemoryLimit { required, .. } => {
            AttemptError::Public(TextLayoutError::CacheBudget {
                required: non_workspace.saturating_add(required),
                budget,
            })
        }
        WorkspaceError::TextLimit { required, limit } => {
            AttemptError::Public(TextLayoutError::TextLimit { required, limit })
        }
        WorkspaceError::RunLimit { required, limit } => {
            AttemptError::Public(TextLayoutError::ClusterLimit { required, limit })
        }
        WorkspaceError::GlyphLimit { required, .. } => AttemptError::Public(
            TextLayoutError::Layout(LayoutError::InsufficientGlyphCapacity { minimum: required }),
        ),
        WorkspaceError::ScratchLimit { required, .. } => AttemptError::Public(
            TextLayoutError::Layout(LayoutError::InsufficientScratchCapacity { minimum: required }),
        ),
        WorkspaceError::LineLimit { required, .. } => AttemptError::Public(
            TextLayoutError::Layout(LayoutError::InsufficientLineCapacity { required }),
        ),
        WorkspaceError::DimensionOverflow => {
            AttemptError::Public(TextLayoutError::DimensionOverflow)
        }
        WorkspaceError::Bidi(error) => AttemptError::Public(TextLayoutError::Bidi(error)),
        WorkspaceError::Layout(error) => match error {
            LayoutError::InsufficientPositionedCapacity { minimum } => {
                AttemptError::Grow(BufferKind::PositionedGlyphs, minimum)
            }
            LayoutError::InsufficientCaretCapacity { minimum } => {
                AttemptError::Grow(BufferKind::Carets, minimum)
            }
            LayoutError::InsufficientRunCapacity { required } => {
                AttemptError::Grow(BufferKind::VisualRuns, required)
            }
            LayoutError::InsufficientLineCapacity { required } => {
                AttemptError::Grow(BufferKind::LayoutLines, required)
            }
            error => AttemptError::Public(TextLayoutError::Layout(error)),
        },
    }
}

fn measure(lines: &[LayoutLine], line_height: i32) -> Result<TextMeasure, AttemptError> {
    let width = lines.iter().map(|line| line.advance()).max().unwrap_or(0);
    let height = if lines.is_empty() {
        0
    } else {
        i32::try_from(lines.len())
            .ok()
            .and_then(|count| count.checked_mul(line_height))
            .ok_or(AttemptError::Public(TextLayoutError::DimensionOverflow))?
    };
    Ok(TextMeasure { width, height })
}

fn visible_counts(lines: &[LayoutLine]) -> OutputStarts {
    let Some(last) = lines.last().copied() else {
        return OutputStarts::ZERO;
    };
    OutputStarts {
        lines: lines.len(),
        runs: last.runs().end as usize,
        glyphs: last.glyphs().end as usize,
        carets: last.carets().end as usize,
    }
}

fn reserve_slots<T: Clone>(
    slots: &mut Vec<T>,
    required: usize,
    value: T,
    resident: usize,
    budget: usize,
) -> Result<(), TextLayoutError> {
    reserve_capacity(slots, required, resident, budget)?;
    slots.resize(required, value);
    Ok(())
}

fn reserve_capacity<T>(
    slots: &mut Vec<T>,
    required: usize,
    resident: usize,
    budget: usize,
) -> Result<(), TextLayoutError> {
    if required <= slots.capacity() {
        return Ok(());
    }
    let added = required
        .checked_sub(slots.capacity())
        .and_then(|count| count.checked_mul(size_of::<T>()))
        .ok_or(TextLayoutError::DimensionOverflow)?;
    let projected = resident
        .checked_add(added)
        .ok_or(TextLayoutError::DimensionOverflow)?;
    if projected > budget {
        return Err(TextLayoutError::CacheBudget {
            required: projected,
            budget,
        });
    }
    slots
        .try_reserve_exact(required - slots.len())
        .map_err(|_| TextLayoutError::Allocation)
}

fn vec_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(size_of::<T>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use textflow::shaping::{FontAccessError, FontId, FontMetrics, GlyphId, GlyphSource};

    struct Source;

    impl GlyphSource for Source {
        fn id(&self) -> FontId {
            FontId::new(1)
        }

        fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
            Ok(FontMetrics::default())
        }

        fn glyph_for(&self, character: char) -> Result<Option<GlyphId>, FontAccessError> {
            Ok(Some(GlyphId::new(character as u16)))
        }

        fn glyph_advance(&self, _glyph: GlyphId) -> Result<FlowPoint, FontAccessError> {
            Ok(FlowPoint { x: 256, y: 0 })
        }
    }

    struct CountingSource {
        glyph_queries: Cell<usize>,
    }

    impl GlyphSource for CountingSource {
        fn id(&self) -> FontId {
            FontId::new(2)
        }

        fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
            Ok(FontMetrics::default())
        }

        fn glyph_for(&self, character: char) -> Result<Option<GlyphId>, FontAccessError> {
            self.glyph_queries.set(self.glyph_queries.get() + 1);
            Ok(Some(GlyphId::new(character as u16)))
        }

        fn glyph_advance(&self, _glyph: GlyphId) -> Result<FlowPoint, FontAccessError> {
            Ok(FlowPoint { x: 256, y: 0 })
        }
    }

    fn request<'a>(text: &'a str, width: i32) -> TextLayoutRequest<'a> {
        TextLayoutRequest {
            text,
            max_width: width,
            width: (width != i32::MAX).then_some(width),
            max_lines: usize::MAX,
            line_height: 256,
            baseline: 192,
            direction: BaseDirection::Auto,
            wrap: WrapMode::WordOrGrapheme,
            alignment: Alignment::Start,
            overflow: Overflow::Clip,
            spacing: TextSpacing::default(),
            features: &[],
            line_widths: None,
        }
    }

    #[test]
    fn frame_arena_reuses_capacity_and_invalidates_handles() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let handle = cache.layout(request("ab cd", 3 * 256), &typefaces).unwrap();
        let resident = cache.resident_bytes();
        let layout = cache.get(handle).unwrap();
        assert_eq!(layout.lines().len(), 2);
        assert_eq!(layout.measure().height, 2 * 256);
        cache.begin_frame();
        assert!(cache.get(handle).is_none());
        assert_eq!(cache.resident_bytes(), resident);
    }

    #[test]
    fn cached_layout_keeps_its_stable_handle_across_frames() {
        let source = CountingSource {
            glyph_queries: Cell::new(0),
        };
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let first = cache
            .layout_cached(7, 11, request("persistent", i32::MAX), &typefaces)
            .unwrap();
        let resident = cache.resident_bytes();
        let glyph_queries = source.glyph_queries.get();

        cache.begin_frame();
        let second = cache
            .layout_cached(7, 11, request("persistent", i32::MAX), &typefaces)
            .unwrap();

        assert_eq!(second, first);
        assert_eq!(cache.get(second).unwrap().glyphs().len(), 10);
        assert_eq!(cache.resident_bytes(), resident);
        assert_eq!(source.glyph_queries.get(), glyph_queries);
    }

    #[test]
    fn cached_layout_invalidates_on_font_revision_changes() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let first = cache
            .layout_cached(7, 11, request("revision", i32::MAX), &typefaces)
            .unwrap();
        let changed = cache
            .layout_cached(7, 12, request("revision", i32::MAX), &typefaces)
            .unwrap();

        assert_ne!(changed, first);
        assert_eq!(cache.entries.iter().flatten().count(), 2);
    }

    #[test]
    fn cached_layout_includes_per_line_widths_in_its_key() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let first_widths = [2 * 256, 256];
        let second_widths = [256, 2 * 256];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut first_request = request("abc", i32::MAX);
        first_request.line_widths = Some(&first_widths);
        let first = cache
            .layout_cached(7, 11, first_request, &typefaces)
            .unwrap();
        let mut second_request = request("abc", i32::MAX);
        second_request.line_widths = Some(&second_widths);
        let second = cache
            .layout_cached(7, 11, second_request, &typefaces)
            .unwrap();

        assert_ne!(second, first);
        assert_eq!(cache.get(first).unwrap().lines()[0].advance(), 2 * 256);
        assert_eq!(cache.get(second).unwrap().lines()[0].advance(), 256);
    }

    #[test]
    fn stale_layouts_compact_without_invalidating_live_slots() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let first = cache
            .layout_cached(1, 1, request("a", i32::MAX), &typefaces)
            .unwrap();
        let stale = cache
            .layout_cached(2, 1, request("bb", i32::MAX), &typefaces)
            .unwrap();
        let third = cache
            .layout_cached(3, 1, request("ccc", i32::MAX), &typefaces)
            .unwrap();

        cache.begin_frame();
        assert_eq!(
            cache
                .layout_cached(1, 1, request("a", i32::MAX), &typefaces)
                .unwrap(),
            first
        );
        assert_eq!(
            cache
                .layout_cached(3, 1, request("ccc", i32::MAX), &typefaces)
                .unwrap(),
            third
        );
        cache.begin_frame();

        assert!(cache.get(stale).is_none());
        assert_eq!(cache.get(first).unwrap().glyphs().len(), 1);
        assert_eq!(cache.get(third).unwrap().glyphs().len(), 3);
        let reused = cache
            .layout_cached(4, 1, request("dddd", i32::MAX), &typefaces)
            .unwrap();
        assert_eq!(reused.slot, stale.slot);
        assert_ne!(reused.generation, stale.generation);
        assert_eq!(cache.get(reused).unwrap().glyphs().len(), 4);
    }

    #[test]
    fn measurement_cache_invalidates_on_constraint_changes() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let first = cache
            .measure_cached(5, 9, request("ab cd", 3 * 256), &typefaces)
            .unwrap();
        assert_eq!(cache.measurements.len(), 1);

        let repeated = cache
            .measure_cached(5, 9, request("ab cd", 3 * 256), &typefaces)
            .unwrap();
        let changed = cache
            .measure_cached(5, 9, request("ab cd", 5 * 256), &typefaces)
            .unwrap();

        assert_eq!(repeated, first);
        assert_ne!(changed, first);
        assert_eq!(cache.measurements.len(), 2);
    }

    #[test]
    fn measurement_does_not_commit_output_ranges() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        let measure = cache.measure(request("abc", i32::MAX), &typefaces).unwrap();
        assert_eq!(measure.width, 3 * 256);
        assert!(cache.entries.is_empty());
        assert!(cache.lines.is_empty());
        assert!(cache.glyphs.is_empty());
    }

    #[test]
    fn empty_measurement_keeps_pipeline_storage_empty() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();

        assert_eq!(
            cache.measure(request("", i32::MAX), &typefaces).unwrap(),
            TextMeasure::default()
        );
        assert_eq!(cache.resident_bytes(), 0);
    }

    #[test]
    fn budget_failure_precedes_unbounded_growth() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::new(TextLayoutLimits::EMBEDDED.with_cache_bytes(64));
        cache.begin_frame();
        assert!(matches!(
            cache.layout(request("a paragraph that cannot fit", 4 * 256), &typefaces,),
            Err(TextLayoutError::CacheBudget { .. })
        ));
        assert!(cache.resident_bytes() <= cache.limits().cache_bytes);
    }

    #[test]
    fn max_lines_limits_retained_output_and_measurement() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut limited = request("ab cd ef", 2 * 256);
        limited.max_lines = 2;
        let handle = cache.layout(limited, &typefaces).unwrap();
        let layout = cache.get(handle).unwrap();
        assert_eq!(layout.lines().len(), 2);
        assert_eq!(layout.measure().height, 2 * 256);
        assert_eq!(layout.runs().len(), 2);
        assert_eq!(layout.glyphs().len(), 3);
    }

    #[test]
    fn ellipsis_is_part_of_the_retained_glyph_layout() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut limited = request("ab cd", 3 * 256);
        limited.max_lines = 1;
        limited.overflow = Overflow::Ellipsis;
        let handle = cache.layout(limited, &typefaces).unwrap();
        let layout = cache.get(handle).unwrap();

        assert_eq!(layout.lines().len(), 1);
        assert_eq!(layout.lines()[0].text().end, 2);
        assert_eq!(layout.glyphs().len(), 3);
        assert_eq!(
            layout.glyphs()[2].glyph_id(),
            GlyphId::new('\u{2026}' as u16)
        );
        assert_eq!(layout.carets().len(), 3);
        assert_eq!(layout.measure().width, 3 * 256);
    }

    #[test]
    fn wrap_mode_changes_the_selected_line_boundary() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];

        let first_line_end = |wrap| {
            let mut cache = TextLayoutCache::default();
            cache.begin_frame();
            let mut request = request("ab cd", 4 * 256);
            request.wrap = wrap;
            let handle = cache.layout(request, &typefaces).unwrap();
            cache.get(handle).unwrap().lines()[0].text().end
        };

        assert_eq!(first_line_end(WrapMode::Word), 3);
        assert_eq!(first_line_end(WrapMode::Grapheme), 4);

        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut request = request("ab cd", 4 * 256);
        request.wrap = WrapMode::NoWrap;
        let handle = cache.layout(request, &typefaces).unwrap();
        assert_eq!(cache.get(handle).unwrap().lines().len(), 1);
    }

    #[test]
    fn spacing_participates_in_measurement_and_glyph_positions() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut request = request("ab c", i32::MAX);
        request.wrap = WrapMode::NoWrap;
        request.spacing = TextSpacing {
            letter: 256,
            word: 2 * 256,
        };
        let handle = cache.layout(request, &typefaces).unwrap();
        let layout = cache.get(handle).unwrap();

        assert_eq!(layout.measure().width, 9 * 256);
        assert_eq!(layout.glyphs()[1].origin.x, 2 * 256);
        assert_eq!(layout.glyphs()[2].origin.x, 4 * 256);
        assert_eq!(layout.glyphs()[3].origin.x, 8 * 256);
    }

    #[test]
    fn alignment_is_retained_in_authoritative_positions() {
        let source = Source;
        let face = textflow::shaping::SimpleTypeface::new(&source);
        let typefaces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let mut request = request("ab", 10 * 256);
        request.wrap = WrapMode::NoWrap;
        request.alignment = Alignment::Center;
        let handle = cache.layout(request, &typefaces).unwrap();
        let layout = cache.get(handle).unwrap();

        assert_eq!(layout.lines()[0].origin().x, 4 * 256);
        assert_eq!(layout.glyphs()[0].origin.x, 4 * 256);
        assert_eq!(layout.carets()[0].position.x, 4 * 256);
        assert_eq!(layout.measure().width, 2 * 256);
    }
}
