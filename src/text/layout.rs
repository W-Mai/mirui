use alloc::vec::Vec;
use core::cell::{Ref, RefCell, RefMut};
use core::mem::size_of;

use textflow::bidi::{BaseDirection, BidiError, BidiRun, BidiText};
use textflow::layout::{
    BrokenLine, GlyphRun, LayoutBuffers, LayoutError, LayoutLine, LayoutOptions, LogicalRun,
    LogicalRuns, VisualRun,
};
use textflow::shaping::{
    CaretStop, FlowPoint, FontFeature, PositionedGlyph, ShapedGlyph, Typeface,
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

    pub const fn runs(&self) -> &[VisualRun] {
        self.runs
    }

    pub const fn glyphs(&self) -> &[PositionedGlyph] {
        self.glyphs
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
    pub max_lines: usize,
    pub line_height: i32,
    pub baseline: i32,
    pub direction: BaseDirection,
    pub features: &'a [FontFeature],
}

pub struct TextLayoutCache {
    limits: TextLayoutLimits,
    generation: u32,
    entries: Vec<LayoutEntry>,
    workspace: LayoutWorkspace,
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
            generation: 0,
            entries: Vec::new(),
            workspace: LayoutWorkspace::new(),
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
        vec_bytes(&self.entries)
            .saturating_add(self.workspace.resident_bytes())
            .saturating_add(vec_bytes(&self.lines))
            .saturating_add(vec_bytes(&self.runs))
            .saturating_add(vec_bytes(&self.glyphs))
            .saturating_add(vec_bytes(&self.carets))
    }

    pub fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.entries.clear();
        self.lines.clear();
        self.runs.clear();
        self.glyphs.clear();
        self.carets.clear();
    }

    pub fn get(&self, handle: TextLayoutHandle) -> Option<TextLayout<'_>> {
        if handle.generation != self.generation {
            return None;
        }
        let entry = self.entries.get(handle.slot as usize)?;
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

    pub(crate) fn layout(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<TextLayoutHandle, TextLayoutError> {
        self.validate_request(request, typefaces.len())?;
        self.reserve_entries(self.entries.len().saturating_add(1))?;
        let starts = self.output_starts();
        let output = match self.run(request, typefaces) {
            Ok(output) => output,
            Err(error) => {
                self.truncate_outputs(starts);
                return Err(error);
            }
        };
        let slot = self.entries.len();
        self.entries.push(LayoutEntry {
            lines: starts.lines..self.lines.len(),
            runs: starts.runs..self.runs.len(),
            glyphs: starts.glyphs..self.glyphs.len(),
            carets: starts.carets..self.carets.len(),
            measure: output.measure,
        });
        Ok(TextLayoutHandle {
            slot: slot as u32,
            generation: self.generation,
        })
    }

    fn run(
        &mut self,
        request: TextLayoutRequest<'_>,
        typefaces: &[&dyn Typeface],
    ) -> Result<LayoutOutput, TextLayoutError> {
        self.validate_request(request, typefaces.len())?;
        self.prepare_workspace(request.text)?;
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
        self.prepare_output_slots(starts)?;
        let bidi = BidiText::resolve(
            request.text,
            0..request.text.len(),
            request.direction,
            &mut self.workspace.bidi,
        )
        .map_err(map_bidi)?;
        let logical =
            LogicalRuns::resolve(request.text, &bidi, typefaces, &mut self.workspace.logical)
                .map_err(|error| map_layout(BufferKind::LogicalRuns, error))?;
        let shaped = logical
            .shape_into(
                request.text,
                typefaces,
                request.features,
                &mut self.workspace.initial_glyphs,
                &mut self.workspace.initial_runs,
            )
            .map_err(map_shape)?;
        let broken = shaped
            .break_into(request.text, request.max_width, &mut self.workspace.broken)
            .map_err(|error| map_layout(BufferKind::BrokenLines, error))?;

        let paragraph = logical
            .layout_into(
                request.text,
                typefaces,
                request.features,
                &broken,
                LayoutOptions::new(request.line_height).with_origin(FlowPoint {
                    x: 0,
                    y: request.baseline,
                }),
                LayoutBuffers::new(
                    &mut self.workspace.scratch,
                    &mut self.glyphs[starts.glyphs..],
                    &mut self.runs[starts.runs..],
                    &mut self.lines[starts.lines..],
                    &mut self.carets[starts.carets..],
                ),
            )
            .map_err(map_final)?;
        let visible_lines = paragraph.lines().len().min(request.max_lines);
        let visible = &paragraph.lines()[..visible_lines];
        let counts = visible_counts(visible);
        let measure = measure(visible, request.line_height)?;
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

    fn prepare_output_slots(&mut self, starts: OutputStarts) -> Result<(), AttemptError> {
        self.resize_output(BufferKind::PositionedGlyphs, starts.glyphs)?;
        self.resize_output(BufferKind::VisualRuns, starts.runs)?;
        self.resize_output(BufferKind::LayoutLines, starts.lines)?;
        self.resize_output(BufferKind::Carets, starts.carets)?;
        Ok(())
    }

    fn resize_output(&mut self, kind: BufferKind, start: usize) -> Result<(), AttemptError> {
        let slots = self.workspace.slots(kind);
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
        match kind {
            BufferKind::BidiRuns
            | BufferKind::LogicalRuns
            | BufferKind::InitialGlyphs
            | BufferKind::InitialRuns
            | BufferKind::BrokenLines
            | BufferKind::ScratchGlyphs => self.ensure_workspace(kind, required),
            BufferKind::PositionedGlyphs
            | BufferKind::VisualRuns
            | BufferKind::LayoutLines
            | BufferKind::Carets => {
                self.workspace.set_slots(kind, required);
                Ok(())
            }
        }
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
            _ => Ok(()),
        }
    }

    fn prepare_workspace(&mut self, text: &str) -> Result<(), TextLayoutError> {
        let clusters = text.chars().count().max(1);
        let glyphs = clusters.saturating_mul(2).min(self.limits.glyphs).max(1);
        let lines = clusters.saturating_add(1).min(self.limits.lines).max(1);
        self.grow(BufferKind::BidiRuns, clusters)?;
        self.grow(BufferKind::LogicalRuns, clusters)?;
        self.grow(BufferKind::InitialGlyphs, glyphs)?;
        self.grow(BufferKind::InitialRuns, clusters)?;
        self.grow(BufferKind::BrokenLines, lines)?;
        self.grow(BufferKind::ScratchGlyphs, glyphs)?;
        self.workspace.output_slots = OutputStarts {
            lines,
            runs: clusters.saturating_mul(2).min(self.limits.glyphs).max(1),
            glyphs,
            carets: glyphs
                .saturating_add(lines)
                .min(self.limits.glyphs.saturating_add(self.limits.lines)),
        };
        Ok(())
    }

    fn ensure_workspace(
        &mut self,
        kind: BufferKind,
        required: usize,
    ) -> Result<(), TextLayoutError> {
        let resident = self.resident_bytes();
        match kind {
            BufferKind::BidiRuns => reserve_slots(
                &mut self.workspace.bidi,
                required,
                BidiRun::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::LogicalRuns => reserve_slots(
                &mut self.workspace.logical,
                required,
                LogicalRun::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::InitialGlyphs => reserve_slots(
                &mut self.workspace.initial_glyphs,
                required,
                ShapedGlyph::default(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::InitialRuns => reserve_slots(
                &mut self.workspace.initial_runs,
                required,
                GlyphRun::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::BrokenLines => reserve_slots(
                &mut self.workspace.broken,
                required,
                BrokenLine::empty(),
                resident,
                self.limits.cache_bytes,
            ),
            BufferKind::ScratchGlyphs => reserve_slots(
                &mut self.workspace.scratch,
                required,
                ShapedGlyph::default(),
                resident,
                self.limits.cache_bytes,
            ),
            _ => Ok(()),
        }
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

struct LayoutWorkspace {
    bidi: Vec<BidiRun>,
    logical: Vec<LogicalRun>,
    initial_glyphs: Vec<ShapedGlyph>,
    initial_runs: Vec<GlyphRun>,
    broken: Vec<BrokenLine>,
    scratch: Vec<ShapedGlyph>,
    output_slots: OutputStarts,
}

impl LayoutWorkspace {
    const fn new() -> Self {
        Self {
            bidi: Vec::new(),
            logical: Vec::new(),
            initial_glyphs: Vec::new(),
            initial_runs: Vec::new(),
            broken: Vec::new(),
            scratch: Vec::new(),
            output_slots: OutputStarts::ZERO,
        }
    }

    fn resident_bytes(&self) -> usize {
        vec_bytes(&self.bidi)
            .saturating_add(vec_bytes(&self.logical))
            .saturating_add(vec_bytes(&self.initial_glyphs))
            .saturating_add(vec_bytes(&self.initial_runs))
            .saturating_add(vec_bytes(&self.broken))
            .saturating_add(vec_bytes(&self.scratch))
    }

    const fn slots(&self, kind: BufferKind) -> usize {
        match kind {
            BufferKind::PositionedGlyphs => self.output_slots.glyphs,
            BufferKind::VisualRuns => self.output_slots.runs,
            BufferKind::LayoutLines => self.output_slots.lines,
            BufferKind::Carets => self.output_slots.carets,
            _ => 0,
        }
    }

    fn set_slots(&mut self, kind: BufferKind, required: usize) {
        match kind {
            BufferKind::PositionedGlyphs => self.output_slots.glyphs = required,
            BufferKind::VisualRuns => self.output_slots.runs = required,
            BufferKind::LayoutLines => self.output_slots.lines = required,
            BufferKind::Carets => self.output_slots.carets = required,
            _ => {}
        }
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
}

struct LayoutEntry {
    lines: core::ops::Range<usize>,
    runs: core::ops::Range<usize>,
    glyphs: core::ops::Range<usize>,
    carets: core::ops::Range<usize>,
    measure: TextMeasure,
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
    BidiRuns,
    LogicalRuns,
    InitialGlyphs,
    InitialRuns,
    BrokenLines,
    ScratchGlyphs,
    PositionedGlyphs,
    VisualRuns,
    LayoutLines,
    Carets,
}

impl BufferKind {
    const fn limit(self, limits: TextLayoutLimits) -> usize {
        match self {
            Self::BidiRuns | Self::LogicalRuns | Self::InitialRuns => limits.clusters,
            Self::InitialGlyphs
            | Self::ScratchGlyphs
            | Self::PositionedGlyphs
            | Self::VisualRuns => limits.glyphs,
            Self::BrokenLines | Self::LayoutLines => limits.lines,
            Self::Carets => limits.glyphs + limits.lines,
        }
    }

    const fn limit_error(self, required: usize, limit: usize) -> TextLayoutError {
        match self {
            Self::BrokenLines | Self::LayoutLines => {
                TextLayoutError::Layout(LayoutError::InsufficientLineCapacity { required })
            }
            Self::BidiRuns | Self::LogicalRuns | Self::InitialRuns => {
                TextLayoutError::ClusterLimit { required, limit }
            }
            _ => TextLayoutError::Layout(LayoutError::InsufficientGlyphCapacity {
                minimum: required,
            }),
        }
    }
}

fn map_bidi(error: BidiError) -> AttemptError {
    match error {
        BidiError::InsufficientCapacity { required } => {
            AttemptError::Grow(BufferKind::BidiRuns, required)
        }
        error => AttemptError::Public(TextLayoutError::Bidi(error)),
    }
}

fn map_shape(error: LayoutError) -> AttemptError {
    match error {
        LayoutError::InsufficientGlyphCapacity { minimum } => {
            AttemptError::Grow(BufferKind::InitialGlyphs, minimum)
        }
        LayoutError::InsufficientRunCapacity { required } => {
            AttemptError::Grow(BufferKind::InitialRuns, required)
        }
        error => AttemptError::Public(TextLayoutError::Layout(error)),
    }
}

fn map_final(error: LayoutError) -> AttemptError {
    let growth = match error {
        LayoutError::InsufficientScratchCapacity { minimum } => {
            Some((BufferKind::ScratchGlyphs, minimum))
        }
        LayoutError::InsufficientPositionedCapacity { minimum } => {
            Some((BufferKind::PositionedGlyphs, minimum))
        }
        LayoutError::InsufficientCaretCapacity { minimum } => Some((BufferKind::Carets, minimum)),
        LayoutError::InsufficientRunCapacity { required } => {
            Some((BufferKind::VisualRuns, required))
        }
        LayoutError::InsufficientLineCapacity { required } => {
            Some((BufferKind::LayoutLines, required))
        }
        _ => None,
    };
    growth.map_or_else(
        || AttemptError::Public(TextLayoutError::Layout(error)),
        |(kind, required)| AttemptError::Grow(kind, required),
    )
}

fn map_layout(kind: BufferKind, error: LayoutError) -> AttemptError {
    match error {
        LayoutError::InsufficientRunCapacity { required }
        | LayoutError::InsufficientLineCapacity { required } => AttemptError::Grow(kind, required),
        error => AttemptError::Public(TextLayoutError::Layout(error)),
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

    fn request<'a>(text: &'a str, width: i32) -> TextLayoutRequest<'a> {
        TextLayoutRequest {
            text,
            max_width: width,
            max_lines: usize::MAX,
            line_height: 256,
            baseline: 192,
            direction: BaseDirection::Auto,
            features: &[],
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
}
