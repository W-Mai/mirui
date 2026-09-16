use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::Range;

use crate::render::path::{Path, PathCmd};
use crate::render::path::{PathId, PathRevision, PathStore, PathStoreError};
use crate::text::{PathDirection, TextLayout, TextLayoutHandle, TextPath};
use crate::types::fixed::{from_textflow, to_textflow};
use crate::types::{Fixed, Point};

const MAX_SUBDIVISION_DEPTH: u8 = 12;
const MIN_CURVE_SUBDIVISION_DEPTH: u8 = 3;
pub(crate) const DEFAULT_TOLERANCE: Fixed = Fixed::from_ratio(1, 4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathBaselineError {
    MissingSubpath(u16),
    Empty,
    InvalidTolerance,
    CurvesRequireMeasurement,
    LengthOverflow,
    SubdivisionLimit,
    InsufficientCapacity { required: usize, provided: usize },
    DistanceOutOfRange { distance: i32, length: i32 },
    DistanceOrder { previous: i32, requested: i32 },
    CacheBudget { required: usize, budget: usize },
    Allocation,
    Unavailable,
    Store(PathStoreError),
    InvalidRange,
    InvalidSeam,
    OpenSeam,
    Layout(textflow::layout::LayoutError),
    Placement(textflow::placement::PlacementError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BaselineSample {
    pub position: Point,
    pub unit_tangent: Point,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MeasuredSegment {
    pub end: Point,
    pub end_distance: Fixed,
}

const EMPTY_SEGMENT: MeasuredSegment = MeasuredSegment {
    end: Point::ZERO,
    end_distance: Fixed::ZERO,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MeasurementRequirements {
    pub segments: usize,
    pub length: Fixed,
}

pub(crate) struct PathMeasure<'a> {
    commands: &'a [PathCmd],
    start: Point,
    tolerance: Fixed,
    has_curves: bool,
    closed: bool,
}

impl<'a> PathMeasure<'a> {
    pub fn new(path: &'a Path, subpath: u16, tolerance: Fixed) -> Result<Self, PathBaselineError> {
        if tolerance <= Fixed::ZERO {
            return Err(PathBaselineError::InvalidTolerance);
        }

        let commands = path.commands();
        let mut found = None;
        let mut seen = 0u16;
        for (index, command) in commands.iter().enumerate() {
            if let PathCmd::MoveTo(start) = command {
                if seen == subpath {
                    found = Some((index, *start));
                    break;
                }
                seen = seen
                    .checked_add(1)
                    .ok_or(PathBaselineError::MissingSubpath(subpath))?;
            }
        }
        let (start_index, start) = found.ok_or(PathBaselineError::MissingSubpath(subpath))?;
        let rest = &commands[start_index + 1..];
        let end = rest
            .iter()
            .position(|command| matches!(command, PathCmd::MoveTo(_)))
            .unwrap_or(rest.len());
        let commands = &rest[..end];
        let has_curves = commands
            .iter()
            .any(|command| matches!(command, PathCmd::QuadTo { .. } | PathCmd::CubicTo { .. }));
        let closed = commands
            .iter()
            .any(|command| matches!(command, PathCmd::Close));

        Ok(Self {
            commands,
            start,
            tolerance,
            has_curves,
            closed,
        })
    }

    pub const fn is_line_only(&self) -> bool {
        !self.has_curves
    }

    pub fn line(&self) -> Result<LineBaseline<'a>, PathBaselineError> {
        if self.has_curves {
            return Err(PathBaselineError::CurvesRequireMeasurement);
        }
        let length = accumulate(
            self.start,
            self.commands,
            self.tolerance,
            CountSink::default(),
        )?
        .1;
        if length == Fixed::ZERO {
            return Err(PathBaselineError::Empty);
        }
        Ok(LineBaseline {
            commands: self.commands,
            start: self.start,
            length,
            closed: self.closed,
        })
    }

    pub fn requirements(&self) -> Result<MeasurementRequirements, PathBaselineError> {
        let (sink, length) = accumulate(
            self.start,
            self.commands,
            self.tolerance,
            CountSink::default(),
        )?;
        if length == Fixed::ZERO {
            return Err(PathBaselineError::Empty);
        }
        Ok(MeasurementRequirements {
            segments: sink.segments,
            length,
        })
    }

    pub fn measure_into<'b>(
        &self,
        output: &'b mut [MeasuredSegment],
    ) -> Result<MeasuredBaseline<'b>, PathBaselineError> {
        let requirements = self.requirements()?;
        if output.len() < requirements.segments {
            return Err(PathBaselineError::InsufficientCapacity {
                required: requirements.segments,
                provided: output.len(),
            });
        }

        let sink = WriteSink {
            output,
            written: 0,
            distance_raw: 0,
        };
        let (sink, length) = accumulate(self.start, self.commands, self.tolerance, sink)?;
        debug_assert_eq!(sink.written, requirements.segments);
        debug_assert_eq!(length, requirements.length);
        Ok(MeasuredBaseline {
            start: self.start,
            segments: &sink.output[..sink.written],
            length,
            closed: self.closed,
            smooth_tangents: self.has_curves,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LineBaseline<'a> {
    commands: &'a [PathCmd],
    start: Point,
    length: Fixed,
    closed: bool,
}

impl<'a> LineBaseline<'a> {
    pub const fn length(&self) -> Fixed {
        self.length
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn cursor(&self) -> LineCursor<'a> {
        LineCursor {
            commands: self.commands,
            command: 0,
            subpath_start: self.start,
            current: self.start,
            distance_raw: 0,
            length_raw: to_textflow(self.length),
            last_query_raw: None,
            active: None,
            terminal: None,
        }
    }

    fn reverse_cursor(&self) -> ReverseLineCursor<'a> {
        let mut current = self.start;
        for command in self.commands {
            match command {
                PathCmd::LineTo(end) => current = *end,
                PathCmd::Close => current = self.start,
                PathCmd::QuadTo { .. } | PathCmd::CubicTo { .. } | PathCmd::MoveTo(_) => {}
            }
        }
        ReverseLineCursor {
            commands: self.commands,
            command: self.commands.len(),
            subpath_start: self.start,
            current,
            distance_raw: to_textflow(self.length),
            length_raw: to_textflow(self.length),
            last_query_raw: None,
            active: None,
            terminal: None,
        }
    }
}

#[derive(Clone, Copy)]
struct ActiveSegment {
    start: Point,
    end: Point,
    start_distance_raw: i32,
    end_distance_raw: i32,
    length_raw: i32,
}

pub(crate) struct LineCursor<'a> {
    commands: &'a [PathCmd],
    command: usize,
    subpath_start: Point,
    current: Point,
    distance_raw: i32,
    length_raw: i32,
    last_query_raw: Option<i32>,
    active: Option<ActiveSegment>,
    terminal: Option<BaselineSample>,
}

impl LineCursor<'_> {
    pub fn sample_forward(&mut self, distance: Fixed) -> Result<BaselineSample, PathBaselineError> {
        let query_raw = to_textflow(distance);
        if query_raw < 0 {
            return Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: self.length_raw,
            });
        }
        if let Some(previous) = self.last_query_raw {
            if query_raw < previous {
                return Err(PathBaselineError::DistanceOrder {
                    previous,
                    requested: query_raw,
                });
            }
        }
        self.last_query_raw = Some(query_raw);

        loop {
            if let Some(segment) = self.active {
                if query_raw < segment.end_distance_raw {
                    return Ok(sample_segment(
                        segment.start,
                        segment.end,
                        segment.length_raw,
                        query_raw - segment.start_distance_raw,
                    ));
                }
                self.current = segment.end;
                self.distance_raw = segment.end_distance_raw;
                self.terminal = Some(sample_segment(
                    segment.start,
                    segment.end,
                    segment.length_raw,
                    segment.length_raw,
                ));
                self.active = None;
            }

            let Some(command) = self.commands.get(self.command) else {
                break;
            };
            self.command += 1;
            let end = match command {
                PathCmd::LineTo(end) => *end,
                PathCmd::Close => self.subpath_start,
                PathCmd::QuadTo { .. } | PathCmd::CubicTo { .. } | PathCmd::MoveTo(_) => {
                    continue;
                }
            };
            let segment_raw = distance_raw(self.current, end)?;
            if segment_raw != 0 {
                self.active = Some(ActiveSegment {
                    start: self.current,
                    end,
                    start_distance_raw: self.distance_raw,
                    end_distance_raw: self
                        .distance_raw
                        .checked_add(segment_raw)
                        .ok_or(PathBaselineError::LengthOverflow)?,
                    length_raw: segment_raw,
                });
            } else {
                self.current = end;
            }
        }

        if query_raw == self.distance_raw {
            self.terminal.ok_or(PathBaselineError::Empty)
        } else {
            Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: self.length_raw,
            })
        }
    }
}

pub(crate) struct ReverseLineCursor<'a> {
    commands: &'a [PathCmd],
    command: usize,
    subpath_start: Point,
    current: Point,
    distance_raw: i32,
    length_raw: i32,
    last_query_raw: Option<i32>,
    active: Option<ActiveSegment>,
    terminal: Option<BaselineSample>,
}

impl ReverseLineCursor<'_> {
    fn sample_backward(&mut self, distance: Fixed) -> Result<BaselineSample, PathBaselineError> {
        let query_raw = to_textflow(distance);
        if query_raw < 0 || query_raw > self.length_raw {
            return Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: self.length_raw,
            });
        }
        if let Some(previous) = self.last_query_raw {
            if query_raw > previous {
                return Err(PathBaselineError::DistanceOrder {
                    previous,
                    requested: query_raw,
                });
            }
        }
        self.last_query_raw = Some(query_raw);

        loop {
            if let Some(segment) = self.active {
                if query_raw > segment.start_distance_raw {
                    return Ok(reverse_sample(sample_segment(
                        segment.start,
                        segment.end,
                        segment.length_raw,
                        query_raw - segment.start_distance_raw,
                    )));
                }
                self.current = segment.start;
                self.distance_raw = segment.start_distance_raw;
                self.terminal = Some(reverse_sample(sample_segment(
                    segment.start,
                    segment.end,
                    segment.length_raw,
                    0,
                )));
                self.active = None;
            }

            let Some(command) = self.command.checked_sub(1) else {
                break;
            };
            self.command = command;
            let start = if command == 0 {
                self.subpath_start
            } else {
                match self.commands[command - 1] {
                    PathCmd::LineTo(point) => point,
                    PathCmd::Close => self.subpath_start,
                    PathCmd::QuadTo { end, .. } | PathCmd::CubicTo { end, .. } => end,
                    PathCmd::MoveTo(point) => point,
                }
            };
            if !matches!(self.commands[command], PathCmd::LineTo(_) | PathCmd::Close) {
                continue;
            }
            let segment_raw = distance_raw(start, self.current)?;
            if segment_raw != 0 {
                self.active = Some(ActiveSegment {
                    start,
                    end: self.current,
                    start_distance_raw: self
                        .distance_raw
                        .checked_sub(segment_raw)
                        .ok_or(PathBaselineError::LengthOverflow)?,
                    end_distance_raw: self.distance_raw,
                    length_raw: segment_raw,
                });
            } else {
                self.current = start;
            }
        }

        if query_raw == 0 {
            self.terminal.ok_or(PathBaselineError::Empty)
        } else {
            Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: self.length_raw,
            })
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MeasuredBaseline<'a> {
    start: Point,
    segments: &'a [MeasuredSegment],
    length: Fixed,
    closed: bool,
    smooth_tangents: bool,
}

impl<'a> MeasuredBaseline<'a> {
    pub const fn length(&self) -> Fixed {
        self.length
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn cursor(&self) -> MeasuredCursor<'a> {
        MeasuredCursor {
            start: self.start,
            segments: self.segments,
            segment: 0,
            last_query_raw: None,
            smooth_tangents: self.smooth_tangents,
        }
    }

    fn reverse_cursor(&self) -> ReverseMeasuredCursor<'a> {
        ReverseMeasuredCursor {
            start: self.start,
            segments: self.segments,
            segment: self.segments.len(),
            last_query_raw: None,
            smooth_tangents: self.smooth_tangents,
        }
    }
}

pub(crate) struct MeasuredCursor<'a> {
    start: Point,
    segments: &'a [MeasuredSegment],
    segment: usize,
    last_query_raw: Option<i32>,
    smooth_tangents: bool,
}

impl MeasuredCursor<'_> {
    pub fn sample_forward(&mut self, distance: Fixed) -> Result<BaselineSample, PathBaselineError> {
        let query_raw = to_textflow(distance);
        if query_raw < 0 {
            return Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: self.total_length_raw(),
            });
        }
        if let Some(previous) = self.last_query_raw {
            if query_raw < previous {
                return Err(PathBaselineError::DistanceOrder {
                    previous,
                    requested: query_raw,
                });
            }
        }
        self.last_query_raw = Some(query_raw);

        while let Some(segment) = self.segments.get(self.segment) {
            let start_distance_raw = if self.segment == 0 {
                0
            } else {
                to_textflow(self.segments[self.segment - 1].end_distance)
            };
            let end_distance_raw = to_textflow(segment.end_distance);
            let segment_raw = end_distance_raw - start_distance_raw;
            if query_raw < end_distance_raw {
                return Ok(sample_measured_segment(
                    self.start,
                    self.segments,
                    self.segment,
                    segment_raw,
                    query_raw - start_distance_raw,
                    self.smooth_tangents,
                ));
            }
            if query_raw == end_distance_raw && self.segment + 1 == self.segments.len() {
                return Ok(sample_measured_segment(
                    self.start,
                    self.segments,
                    self.segment,
                    segment_raw,
                    segment_raw,
                    self.smooth_tangents,
                ));
            }
            self.segment += 1;
        }
        Err(PathBaselineError::DistanceOutOfRange {
            distance: query_raw,
            length: self.total_length_raw(),
        })
    }

    fn total_length_raw(&self) -> i32 {
        self.segments
            .last()
            .map(|segment| to_textflow(segment.end_distance))
            .unwrap_or(0)
    }
}

pub(crate) struct ReverseMeasuredCursor<'a> {
    start: Point,
    segments: &'a [MeasuredSegment],
    segment: usize,
    last_query_raw: Option<i32>,
    smooth_tangents: bool,
}

impl ReverseMeasuredCursor<'_> {
    fn sample_backward(&mut self, distance: Fixed) -> Result<BaselineSample, PathBaselineError> {
        let query_raw = to_textflow(distance);
        let length_raw = self
            .segments
            .last()
            .map(|segment| to_textflow(segment.end_distance))
            .unwrap_or(0);
        if query_raw < 0 || query_raw > length_raw {
            return Err(PathBaselineError::DistanceOutOfRange {
                distance: query_raw,
                length: length_raw,
            });
        }
        if let Some(previous) = self.last_query_raw {
            if query_raw > previous {
                return Err(PathBaselineError::DistanceOrder {
                    previous,
                    requested: query_raw,
                });
            }
        }
        self.last_query_raw = Some(query_raw);

        while let Some(index) = self.segment.checked_sub(1) {
            let segment = self.segments[index];
            let start_distance_raw = if index == 0 {
                0
            } else {
                to_textflow(self.segments[index - 1].end_distance)
            };
            let end_distance_raw = to_textflow(segment.end_distance);
            if query_raw > start_distance_raw {
                return Ok(reverse_sample(sample_measured_segment(
                    self.start,
                    self.segments,
                    index,
                    end_distance_raw - start_distance_raw,
                    query_raw - start_distance_raw,
                    self.smooth_tangents,
                )));
            }
            self.segment = index;
        }

        if query_raw == 0 {
            let Some(first) = self.segments.first() else {
                return Err(PathBaselineError::Empty);
            };
            return Ok(reverse_sample(sample_measured_segment(
                self.start,
                self.segments,
                0,
                to_textflow(first.end_distance),
                0,
                self.smooth_tangents,
            )));
        }

        Err(PathBaselineError::DistanceOutOfRange {
            distance: query_raw,
            length: length_raw,
        })
    }
}

fn reverse_sample(mut sample: BaselineSample) -> BaselineSample {
    sample.unit_tangent.x = Fixed::ZERO - sample.unit_tangent.x;
    sample.unit_tangent.y = Fixed::ZERO - sample.unit_tangent.y;
    sample
}

fn flow_sample(sample: BaselineSample) -> textflow::placement::BaselineSample {
    textflow::placement::BaselineSample {
        position: textflow::shaping::FlowPoint {
            x: to_textflow(sample.position.x),
            y: to_textflow(sample.position.y),
        },
        unit_tangent: textflow::shaping::FlowPoint {
            x: to_textflow(sample.unit_tangent.x),
            y: to_textflow(sample.unit_tangent.y),
        },
    }
}

fn flow_error(error: PathBaselineError) -> textflow::placement::BaselineError {
    use textflow::placement::BaselineError;

    match error {
        PathBaselineError::LengthOverflow => BaselineError::CoordinateOverflow,
        PathBaselineError::DistanceOutOfRange { distance, length } => {
            BaselineError::OutOfRange { distance, length }
        }
        PathBaselineError::DistanceOrder {
            previous,
            requested,
        } => BaselineError::NonMonotonic {
            previous,
            requested,
        },
        PathBaselineError::MissingSubpath(_)
        | PathBaselineError::Empty
        | PathBaselineError::InvalidTolerance
        | PathBaselineError::CurvesRequireMeasurement
        | PathBaselineError::SubdivisionLimit
        | PathBaselineError::InsufficientCapacity { .. }
        | PathBaselineError::CacheBudget { .. }
        | PathBaselineError::Allocation
        | PathBaselineError::Unavailable
        | PathBaselineError::Store(_)
        | PathBaselineError::InvalidRange
        | PathBaselineError::InvalidSeam
        | PathBaselineError::OpenSeam
        | PathBaselineError::Layout(_)
        | PathBaselineError::Placement(_) => BaselineError::InvalidGeometry,
    }
}

impl textflow::placement::TextBaseline for LineBaseline<'_> {
    type Cursor<'a>
        = LineCursor<'a>
    where
        Self: 'a;

    fn length(&self) -> i32 {
        to_textflow(self.length)
    }

    fn cursor(&self) -> Self::Cursor<'_> {
        LineBaseline::cursor(self)
    }
}

impl textflow::placement::BaselineCursor for LineCursor<'_> {
    fn length(&self) -> i32 {
        self.length_raw
    }

    fn sample_forward(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        LineCursor::sample_forward(self, from_textflow(distance))
            .map(flow_sample)
            .map_err(flow_error)
    }
}

impl textflow::placement::TextBaseline for MeasuredBaseline<'_> {
    type Cursor<'a>
        = MeasuredCursor<'a>
    where
        Self: 'a;

    fn length(&self) -> i32 {
        to_textflow(self.length)
    }

    fn cursor(&self) -> Self::Cursor<'_> {
        MeasuredBaseline::cursor(self)
    }
}

impl textflow::placement::BaselineCursor for MeasuredCursor<'_> {
    fn length(&self) -> i32 {
        self.total_length_raw()
    }

    fn sample_forward(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        MeasuredCursor::sample_forward(self, from_textflow(distance))
            .map(flow_sample)
            .map_err(flow_error)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PathBaseline<'a> {
    Line(LineBaseline<'a>),
    Measured(MeasuredBaseline<'a>),
}

impl PathBaseline<'_> {
    pub fn length(&self) -> Fixed {
        match self {
            Self::Line(baseline) => baseline.length(),
            Self::Measured(baseline) => baseline.length(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Self::Line(baseline) => baseline.is_closed(),
            Self::Measured(baseline) => baseline.is_closed(),
        }
    }
}

impl<'a> PathBaseline<'a> {
    fn cursor_owned(self) -> PathCursor<'a> {
        match self {
            Self::Line(baseline) => PathCursor::Line(baseline.cursor()),
            Self::Measured(baseline) => PathCursor::Measured(baseline.cursor()),
        }
    }

    fn reverse_cursor_owned(self) -> ReversePathCursor<'a> {
        match self {
            Self::Line(baseline) => ReversePathCursor::Line(baseline.reverse_cursor()),
            Self::Measured(baseline) => ReversePathCursor::Measured(baseline.reverse_cursor()),
        }
    }
}

pub(crate) enum PathCursor<'a> {
    Line(LineCursor<'a>),
    Measured(MeasuredCursor<'a>),
}

enum ReversePathCursor<'a> {
    Line(ReverseLineCursor<'a>),
    Measured(ReverseMeasuredCursor<'a>),
}

impl ReversePathCursor<'_> {
    fn sample_backward(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        match self {
            Self::Line(cursor) => cursor.sample_backward(from_textflow(distance)),
            Self::Measured(cursor) => cursor.sample_backward(from_textflow(distance)),
        }
        .map(flow_sample)
        .map_err(flow_error)
    }
}

impl textflow::placement::TextBaseline for PathBaseline<'_> {
    type Cursor<'a>
        = PathCursor<'a>
    where
        Self: 'a;

    fn length(&self) -> i32 {
        to_textflow(PathBaseline::length(self))
    }

    fn cursor(&self) -> Self::Cursor<'_> {
        (*self).cursor_owned()
    }
}

impl textflow::placement::BaselineCursor for PathCursor<'_> {
    fn length(&self) -> i32 {
        match self {
            Self::Line(cursor) => textflow::placement::BaselineCursor::length(cursor),
            Self::Measured(cursor) => textflow::placement::BaselineCursor::length(cursor),
        }
    }

    fn sample_forward(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        match self {
            Self::Line(cursor) => {
                textflow::placement::BaselineCursor::sample_forward(cursor, distance)
            }
            Self::Measured(cursor) => {
                textflow::placement::BaselineCursor::sample_forward(cursor, distance)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BaselineWindow<'a> {
    baseline: PathBaseline<'a>,
    anchor: Fixed,
    start: Fixed,
    length: Fixed,
    direction: PathDirection,
    wraps: bool,
}

impl<'a> BaselineWindow<'a> {
    fn new(baseline: PathBaseline<'a>, path: TextPath) -> Result<Self, PathBaselineError> {
        let full_length = baseline.length();
        let end = path.end().unwrap_or(full_length);
        let start = to_textflow(path.start())
            .checked_add(to_textflow(path.offset()))
            .map(from_textflow)
            .ok_or(PathBaselineError::InvalidRange)?;
        if path.start() < Fixed::ZERO
            || path.offset() < Fixed::ZERO
            || end <= start
            || end > full_length
        {
            return Err(PathBaselineError::InvalidRange);
        }
        let anchor = match path.seam() {
            Some(_) if !baseline.is_closed() => return Err(PathBaselineError::OpenSeam),
            Some(seam) if seam < Fixed::ZERO || seam >= full_length => {
                return Err(PathBaselineError::InvalidSeam);
            }
            Some(seam) => seam,
            None if path.direction() == PathDirection::Reverse => full_length,
            None => Fixed::ZERO,
        };
        let wraps = match path.direction() {
            PathDirection::Forward => anchor + end > full_length,
            PathDirection::Reverse => anchor - end < Fixed::ZERO,
        };
        Ok(Self {
            baseline,
            anchor,
            start,
            length: end - start,
            direction: path.direction(),
            wraps,
        })
    }

    fn cursor_owned(self) -> WindowCursor<'a> {
        let primary = match self.direction {
            PathDirection::Forward => TraversalCursor::Forward(self.baseline.cursor_owned()),
            PathDirection::Reverse => {
                TraversalCursor::Reverse(self.baseline.reverse_cursor_owned())
            }
        };
        let wrapped = self.wraps.then(|| match self.direction {
            PathDirection::Forward => TraversalCursor::Forward(self.baseline.cursor_owned()),
            PathDirection::Reverse => {
                TraversalCursor::Reverse(self.baseline.reverse_cursor_owned())
            }
        });
        WindowCursor {
            primary,
            wrapped,
            anchor_raw: to_textflow(self.anchor),
            start_raw: to_textflow(self.start),
            full_length_raw: to_textflow(self.baseline.length()),
            length_raw: to_textflow(self.length),
            direction: self.direction,
            previous: None,
        }
    }
}

enum TraversalCursor<'a> {
    Forward(PathCursor<'a>),
    Reverse(ReversePathCursor<'a>),
}

impl TraversalCursor<'_> {
    fn sample(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        match self {
            Self::Forward(cursor) => {
                textflow::placement::BaselineCursor::sample_forward(cursor, distance)
            }
            Self::Reverse(cursor) => cursor.sample_backward(distance),
        }
    }
}

pub(crate) struct WindowCursor<'a> {
    primary: TraversalCursor<'a>,
    wrapped: Option<TraversalCursor<'a>>,
    anchor_raw: i32,
    start_raw: i32,
    full_length_raw: i32,
    length_raw: i32,
    direction: PathDirection,
    previous: Option<i32>,
}

impl textflow::placement::TextBaseline for BaselineWindow<'_> {
    type Cursor<'a>
        = WindowCursor<'a>
    where
        Self: 'a;

    fn length(&self) -> i32 {
        to_textflow(self.length)
    }

    fn cursor(&self) -> Self::Cursor<'_> {
        (*self).cursor_owned()
    }
}

impl textflow::placement::BaselineCursor for WindowCursor<'_> {
    fn length(&self) -> i32 {
        self.length_raw
    }

    fn sample_forward(
        &mut self,
        distance: i32,
    ) -> Result<textflow::placement::BaselineSample, textflow::placement::BaselineError> {
        if let Some(previous) = self.previous {
            if distance < previous {
                return Err(textflow::placement::BaselineError::NonMonotonic {
                    previous,
                    requested: distance,
                });
            }
        }
        if distance < 0 || distance > self.length_raw {
            return Err(textflow::placement::BaselineError::OutOfRange {
                distance,
                length: self.length_raw,
            });
        }
        self.previous = Some(distance);
        let offset = self
            .start_raw
            .checked_add(distance)
            .ok_or(textflow::placement::BaselineError::CoordinateOverflow)?;
        let source = match self.direction {
            PathDirection::Forward => self.anchor_raw.checked_add(offset),
            PathDirection::Reverse => self.anchor_raw.checked_sub(offset),
        }
        .ok_or(textflow::placement::BaselineError::CoordinateOverflow)?;
        if source > self.full_length_raw {
            return self
                .wrapped
                .as_mut()
                .ok_or(textflow::placement::BaselineError::InvalidGeometry)?
                .sample(source - self.full_length_raw);
        }
        if source < 0 {
            return self
                .wrapped
                .as_mut()
                .ok_or(textflow::placement::BaselineError::InvalidGeometry)?
                .sample(source + self.full_length_raw);
        }
        self.primary.sample(source)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MeasurementKey {
    path: PathId,
    revision: Option<PathRevision>,
    subpath: u16,
    tolerance: Fixed,
}

struct MeasurementEntry {
    key: MeasurementKey,
    start: Point,
    length: Fixed,
    closed: bool,
    segments: Range<usize>,
    last_used: u32,
}

pub(crate) struct PathBaselineCache {
    entries: Vec<MeasurementEntry>,
    segments: Vec<MeasuredSegment>,
    frame: u32,
    budget_bytes: usize,
    entry_limit: usize,
}

impl PathBaselineCache {
    const EMBEDDED_BUDGET: usize = 8 * 1_024;
    const HOST_BUDGET: usize = 8 * 1_024 * 1_024;
    const EMBEDDED_ENTRIES: usize = 32;
    const HOST_ENTRIES: usize = 4_096;

    pub const fn new(budget_bytes: usize, entry_limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            segments: Vec::new(),
            frame: 0,
            budget_bytes,
            entry_limit,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
    }

    fn resolve<'a>(
        &'a mut self,
        key: MeasurementKey,
        path: &'a Path,
    ) -> Result<PathBaseline<'a>, PathBaselineError> {
        let measure = PathMeasure::new(path, key.subpath, key.tolerance)?;
        if measure.is_line_only() {
            return measure.line().map(PathBaseline::Line);
        }

        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries[index].last_used = self.frame;
            return Ok(PathBaseline::Measured(self.baseline(index)));
        }

        if let Some(index) = self.entries.iter().position(|entry| {
            entry.last_used != self.frame
                && entry.key.path == key.path
                && entry.key.subpath == key.subpath
                && entry.key.tolerance == key.tolerance
        }) {
            self.entries.swap_remove(index);
            self.compact();
        }

        let requirements = measure.requirements()?;
        self.make_room(requirements.segments)?;
        self.reserve(requirements.segments)?;
        let start = self.segments.len();
        self.segments
            .resize(start + requirements.segments, EMPTY_SEGMENT);
        let measured = match measure.measure_into(&mut self.segments[start..]) {
            Ok(measured) => measured,
            Err(error) => {
                self.segments.truncate(start);
                return Err(error);
            }
        };
        let entry = MeasurementEntry {
            key,
            start: measured.start,
            length: measured.length,
            closed: measured.closed,
            segments: start..start + requirements.segments,
            last_used: self.frame,
        };
        self.entries.push(entry);
        Ok(PathBaseline::Measured(
            self.baseline(self.entries.len() - 1),
        ))
    }

    fn baseline(&self, index: usize) -> MeasuredBaseline<'_> {
        let entry = &self.entries[index];
        MeasuredBaseline {
            start: entry.start,
            segments: &self.segments[entry.segments.clone()],
            length: entry.length,
            closed: entry.closed,
            smooth_tangents: true,
        }
    }

    fn prepared<'a>(&'a self, key: MeasurementKey, path: &'a Path) -> Option<PathBaseline<'a>> {
        let measure = PathMeasure::new(path, key.subpath, key.tolerance).ok()?;
        if measure.is_line_only() {
            return measure.line().ok().map(PathBaseline::Line);
        }
        let index = self.entries.iter().position(|entry| entry.key == key)?;
        Some(PathBaseline::Measured(self.baseline(index)))
    }

    fn make_room(&mut self, incoming_segments: usize) -> Result<(), PathBaselineError> {
        let single = core::mem::size_of::<MeasurementEntry>().saturating_add(
            incoming_segments.saturating_mul(core::mem::size_of::<MeasuredSegment>()),
        );
        if self.entry_limit == 0 || single > self.budget_bytes {
            return Err(PathBaselineError::CacheBudget {
                required: single,
                budget: self.budget_bytes,
            });
        }

        let mut changed = false;
        while self.entries.len() >= self.entry_limit
            || self.live_bytes(incoming_segments) > self.budget_bytes
        {
            let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.last_used != self.frame)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
            else {
                return Err(PathBaselineError::CacheBudget {
                    required: single,
                    budget: self.budget_bytes,
                });
            };
            self.entries.swap_remove(oldest);
            changed = true;
        }
        if changed {
            self.compact();
        }
        Ok(())
    }

    fn live_bytes(&self, incoming_segments: usize) -> usize {
        self.entries
            .len()
            .saturating_add(1)
            .saturating_mul(core::mem::size_of::<MeasurementEntry>())
            .saturating_add(
                self.segments
                    .len()
                    .saturating_add(incoming_segments)
                    .saturating_mul(core::mem::size_of::<MeasuredSegment>()),
            )
    }

    fn reserve(&mut self, incoming_segments: usize) -> Result<(), PathBaselineError> {
        let needed_entries = self.entries.len() + 1;
        let needed_segments = self.segments.len() + incoming_segments;
        let projected = capacity_bytes(
            self.entries.capacity().max(needed_entries),
            self.segments.capacity().max(needed_segments),
        );
        if projected > self.budget_bytes {
            return Err(PathBaselineError::CacheBudget {
                required: projected,
                budget: self.budget_bytes,
            });
        }
        if needed_entries > self.entries.capacity() {
            self.entries
                .try_reserve_exact(needed_entries - self.entries.len())
                .map_err(|_| PathBaselineError::Allocation)?;
        }
        if needed_segments > self.segments.capacity() {
            self.segments
                .try_reserve_exact(needed_segments - self.segments.len())
                .map_err(|_| PathBaselineError::Allocation)?;
        }
        Ok(())
    }

    fn compact(&mut self) {
        self.entries
            .sort_unstable_by_key(|entry| entry.segments.start);
        let mut next = 0;
        for entry in &mut self.entries {
            let len = entry.segments.len();
            if entry.segments.start != next {
                self.segments.copy_within(entry.segments.clone(), next);
            }
            entry.segments = next..next + len;
            next += len;
        }
        self.segments.truncate(next);
    }
}

pub(crate) struct PathLineProvider<'a> {
    cache: &'a PathBaselineCache,
    path: &'a Path,
    path_id: PathId,
    revision: Option<PathRevision>,
    text_path: TextPath,
    tolerance: Fixed,
    line_count: usize,
}

impl PathLineProvider<'_> {
    pub(crate) const fn line_count(&self) -> usize {
        self.line_count
    }

    fn line_path(&self, line: usize) -> Option<TextPath> {
        let offset = u16::try_from(line).ok()?;
        let subpath = self.text_path.subpath().checked_add(offset)?;
        Some(self.text_path.with_subpath(subpath))
    }

    fn window(&self, line: usize) -> Option<BaselineWindow<'_>> {
        if line >= self.line_count {
            return None;
        }
        let text_path = self.line_path(line)?;
        let key = MeasurementKey {
            path: self.path_id,
            revision: self.revision,
            subpath: text_path.subpath(),
            tolerance: self.tolerance,
        };
        BaselineWindow::new(self.cache.prepared(key, self.path)?, text_path).ok()
    }
}

impl textflow::layout::LineWidthProvider for PathLineProvider<'_> {
    fn line_count(&self) -> usize {
        self.line_count
    }

    fn width(&self, line: usize) -> Option<usize> {
        usize::try_from(to_textflow(self.window(line)?.length)).ok()
    }
}

impl textflow::placement::BaselineProvider for PathLineProvider<'_> {
    type Cursor<'a>
        = WindowCursor<'a>
    where
        Self: 'a;

    fn line_count(&self) -> usize {
        self.line_count
    }

    fn cursor(&self, line: usize) -> Option<Self::Cursor<'_>> {
        self.window(line).map(BaselineWindow::cursor_owned)
    }
}

fn prepare_path_lines<'a>(
    cache: &'a mut PathBaselineCache,
    path: &'a Path,
    path_id: PathId,
    revision: Option<PathRevision>,
    text_path: TextPath,
    tolerance: Fixed,
    line_limit: usize,
) -> Result<PathLineProvider<'a>, PathBaselineError> {
    let total = path
        .commands()
        .iter()
        .filter(|command| matches!(command, PathCmd::MoveTo(_)))
        .count();
    let first = usize::from(text_path.subpath());
    if first >= total {
        return Err(PathBaselineError::MissingSubpath(text_path.subpath()));
    }
    let addressable = usize::from(u16::MAX - text_path.subpath()) + 1;
    let line_count = total.saturating_sub(first).min(addressable).min(line_limit);
    for line in 0..line_count {
        let subpath = text_path
            .subpath()
            .checked_add(u16::try_from(line).map_err(|_| PathBaselineError::LengthOverflow)?)
            .ok_or(PathBaselineError::LengthOverflow)?;
        let line_path = text_path.with_subpath(subpath);
        let key = MeasurementKey {
            path: path_id,
            revision,
            subpath,
            tolerance,
        };
        let baseline = cache.resolve(key, path)?;
        let _ = BaselineWindow::new(baseline, line_path)?;
    }
    let provider = PathLineProvider {
        cache,
        path,
        path_id,
        revision,
        text_path,
        tolerance,
        line_count,
    };
    for line in 0..line_count {
        if provider.window(line).is_none() {
            return Err(PathBaselineError::Unavailable);
        }
    }
    Ok(provider)
}

fn capacity_bytes(entries: usize, segments: usize) -> usize {
    entries
        .saturating_mul(core::mem::size_of::<MeasurementEntry>())
        .saturating_add(segments.saturating_mul(core::mem::size_of::<MeasuredSegment>()))
}

impl Default for PathBaselineCache {
    fn default() -> Self {
        if cfg!(feature = "std") {
            Self::new(Self::HOST_BUDGET, Self::HOST_ENTRIES)
        } else {
            Self::new(Self::EMBEDDED_BUDGET, Self::EMBEDDED_ENTRIES)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlacementKey {
    layout: TextLayoutHandle,
    path: TextPath,
    revision: Option<PathRevision>,
    tolerance: Fixed,
}

struct FrameEntry {
    key: PlacementKey,
    values: Range<usize>,
    last_used: u32,
}

struct FrameArena<T> {
    entries: Vec<FrameEntry>,
    values: Vec<T>,
    frame: u32,
    budget_bytes: usize,
    entry_limit: usize,
}

impl<T> FrameArena<T> {
    const fn new(budget_bytes: usize, entry_limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            values: Vec::new(),
            frame: 0,
            budget_bytes,
            entry_limit,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
    }
}

impl<T: Copy + Default> FrameArena<T> {
    fn resolve(
        &mut self,
        key: PlacementKey,
        count: usize,
        write: impl FnOnce(&mut [T]) -> Result<(), PathBaselineError>,
    ) -> Result<&[T], PathBaselineError> {
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries[index].last_used = self.frame;
            return Ok(&self.values[self.entries[index].values.clone()]);
        }
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.last_used != self.frame
                && entry.key.path.path() == key.path.path()
                && entry.key.tolerance == key.tolerance
        }) {
            self.entries.swap_remove(index);
            self.compact();
        }
        self.make_room(count)?;
        self.reserve(count)?;
        let start = self.values.len();
        self.values.resize(start + count, T::default());
        if let Err(error) = write(&mut self.values[start..]) {
            self.values.truncate(start);
            return Err(error);
        }
        let values = start..start + count;
        self.entries.push(FrameEntry {
            key,
            values: values.clone(),
            last_used: self.frame,
        });
        Ok(&self.values[values])
    }

    fn make_room(&mut self, incoming_values: usize) -> Result<(), PathBaselineError> {
        let single = core::mem::size_of::<FrameEntry>()
            .saturating_add(incoming_values.saturating_mul(core::mem::size_of::<T>()));
        if self.entry_limit == 0 || single > self.budget_bytes {
            return Err(PathBaselineError::CacheBudget {
                required: single,
                budget: self.budget_bytes,
            });
        }
        let mut changed = false;
        while self.entries.len() >= self.entry_limit
            || self.live_bytes(incoming_values) > self.budget_bytes
        {
            let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.last_used != self.frame)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
            else {
                return Err(PathBaselineError::CacheBudget {
                    required: single,
                    budget: self.budget_bytes,
                });
            };
            self.entries.swap_remove(oldest);
            changed = true;
        }
        if changed {
            self.compact();
        }
        Ok(())
    }

    fn live_bytes(&self, incoming_values: usize) -> usize {
        self.entries
            .len()
            .saturating_add(1)
            .saturating_mul(core::mem::size_of::<FrameEntry>())
            .saturating_add(
                self.values
                    .len()
                    .saturating_add(incoming_values)
                    .saturating_mul(core::mem::size_of::<T>()),
            )
    }

    fn reserve(&mut self, incoming_values: usize) -> Result<(), PathBaselineError> {
        let needed_entries = self.entries.len() + 1;
        let needed_values = self.values.len() + incoming_values;
        let projected = frame_capacity_bytes::<T>(
            self.entries.capacity().max(needed_entries),
            self.values.capacity().max(needed_values),
        );
        if projected > self.budget_bytes {
            return Err(PathBaselineError::CacheBudget {
                required: projected,
                budget: self.budget_bytes,
            });
        }
        if needed_entries > self.entries.capacity() {
            self.entries
                .try_reserve_exact(needed_entries - self.entries.len())
                .map_err(|_| PathBaselineError::Allocation)?;
        }
        if needed_values > self.values.capacity() {
            self.values
                .try_reserve_exact(needed_values - self.values.len())
                .map_err(|_| PathBaselineError::Allocation)?;
        }
        Ok(())
    }

    fn compact(&mut self) {
        self.entries
            .sort_unstable_by_key(|entry| entry.values.start);
        let mut next = 0;
        for entry in &mut self.entries {
            let len = entry.values.len();
            if entry.values.start != next {
                self.values.copy_within(entry.values.clone(), next);
            }
            entry.values = next..next + len;
            next += len;
        }
        self.values.truncate(next);
    }
}

fn frame_capacity_bytes<T>(entries: usize, values: usize) -> usize {
    entries
        .saturating_mul(core::mem::size_of::<FrameEntry>())
        .saturating_add(values.saturating_mul(core::mem::size_of::<T>()))
}

struct PathPlacementCache {
    glyphs: FrameArena<textflow::placement::GlyphFrame>,
    carets: FrameArena<textflow::placement::CaretFrame>,
}

impl PathPlacementCache {
    const EMBEDDED_GLYPH_BUDGET: usize = 16 * 1_024;
    const HOST_GLYPH_BUDGET: usize = 16 * 1_024 * 1_024;
    const EMBEDDED_CARET_BUDGET: usize = 8 * 1_024;
    const HOST_CARET_BUDGET: usize = 8 * 1_024 * 1_024;
    const EMBEDDED_ENTRIES: usize = 32;
    const HOST_ENTRIES: usize = 4_096;

    const fn new(glyph_budget: usize, caret_budget: usize, entry_limit: usize) -> Self {
        Self {
            glyphs: FrameArena::new(glyph_budget, entry_limit),
            carets: FrameArena::new(caret_budget, entry_limit),
        }
    }

    fn begin_frame(&mut self) {
        self.glyphs.begin_frame();
        self.carets.begin_frame();
    }

    fn resolve_glyphs<'a>(
        &'a mut self,
        key: PlacementKey,
        layout: &TextLayout<'_>,
        baselines: &PathLineProvider<'_>,
    ) -> Result<&'a [textflow::placement::GlyphFrame], PathBaselineError> {
        let paragraph = layout.paragraph().map_err(PathBaselineError::Layout)?;
        let placement = paragraph.place_with(baselines);
        let requirements = placement
            .preflight()
            .map_err(PathBaselineError::Placement)?;
        self.glyphs.resolve(key, requirements.glyphs, |output| {
            placement
                .place_into(textflow::placement::PlacementOutput::new(output))
                .map(|_| ())
                .map_err(PathBaselineError::Placement)
        })
    }

    fn resolve_carets<'a>(
        &'a mut self,
        key: PlacementKey,
        layout: &TextLayout<'_>,
        baselines: &PathLineProvider<'_>,
    ) -> Result<&'a [textflow::placement::CaretFrame], PathBaselineError> {
        let paragraph = layout.paragraph().map_err(PathBaselineError::Layout)?;
        let placement = paragraph.place_with(baselines);
        let requirements = placement
            .preflight()
            .map_err(PathBaselineError::Placement)?;
        self.carets.resolve(key, requirements.carets, |output| {
            placement
                .place_carets_into(output)
                .map(|_| ())
                .map_err(PathBaselineError::Placement)
        })
    }
}

impl Default for PathPlacementCache {
    fn default() -> Self {
        if cfg!(feature = "std") {
            Self::new(
                Self::HOST_GLYPH_BUDGET,
                Self::HOST_CARET_BUDGET,
                Self::HOST_ENTRIES,
            )
        } else {
            Self::new(
                Self::EMBEDDED_GLYPH_BUDGET,
                Self::EMBEDDED_CARET_BUDGET,
                Self::EMBEDDED_ENTRIES,
            )
        }
    }
}

struct PathTextRuntime {
    baselines: PathBaselineCache,
    placements: PathPlacementCache,
}

pub(crate) struct PathBaselineResource(RefCell<PathTextRuntime>);

impl PathBaselineResource {
    pub fn new(cache: PathBaselineCache) -> Self {
        Self(RefCell::new(PathTextRuntime {
            baselines: cache,
            placements: PathPlacementCache::default(),
        }))
    }

    pub fn begin_frame(&self) {
        let runtime = &mut *self.0.borrow_mut();
        runtime.baselines.begin_frame();
        runtime.placements.begin_frame();
    }

    #[cfg(test)]
    pub fn with<R>(
        &self,
        store: &PathStore,
        text_path: crate::text::TextPath,
        tolerance: Fixed,
        inspect: impl FnOnce(&PathBaseline<'_>) -> R,
    ) -> Result<R, PathBaselineError> {
        let path = store
            .get(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let revision = store
            .revision(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let key = MeasurementKey {
            path: text_path.path(),
            revision,
            subpath: text_path.subpath(),
            tolerance,
        };
        let mut runtime = self.0.borrow_mut();
        let baseline = runtime.baselines.resolve(key, path)?;
        Ok(inspect(&baseline))
    }

    pub fn with_lines<R>(
        &self,
        store: &PathStore,
        text_path: TextPath,
        tolerance: Fixed,
        line_limit: usize,
        inspect: impl FnOnce(&PathLineProvider<'_>) -> R,
    ) -> Result<R, PathBaselineError> {
        let path = store
            .get(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let revision = store
            .revision(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let runtime = &mut *self.0.borrow_mut();
        let lines = prepare_path_lines(
            &mut runtime.baselines,
            path,
            text_path.path(),
            revision,
            text_path,
            tolerance,
            line_limit,
        )?;
        Ok(inspect(&lines))
    }

    pub fn with_glyph_frames<R>(
        &self,
        store: &PathStore,
        text_path: TextPath,
        tolerance: Fixed,
        layout_handle: TextLayoutHandle,
        layout: &TextLayout<'_>,
        inspect: impl FnOnce(&[textflow::placement::GlyphFrame]) -> R,
    ) -> Result<R, PathBaselineError> {
        let path = store
            .get(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let revision = store
            .revision(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let placement_key = PlacementKey {
            layout: layout_handle,
            path: text_path,
            revision,
            tolerance,
        };
        let runtime = &mut *self.0.borrow_mut();
        let PathTextRuntime {
            baselines,
            placements,
        } = runtime;
        let lines = prepare_path_lines(
            baselines,
            path,
            text_path.path(),
            revision,
            text_path,
            tolerance,
            layout.lines().len(),
        )?;
        let frames = placements.resolve_glyphs(placement_key, layout, &lines)?;
        Ok(inspect(frames))
    }

    pub fn with_caret_frames<R>(
        &self,
        store: &PathStore,
        text_path: TextPath,
        tolerance: Fixed,
        layout_handle: TextLayoutHandle,
        layout: &TextLayout<'_>,
        inspect: impl FnOnce(&[textflow::placement::CaretFrame]) -> R,
    ) -> Result<R, PathBaselineError> {
        let path = store
            .get(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let revision = store
            .revision(text_path.path())
            .map_err(PathBaselineError::Store)?;
        let placement_key = PlacementKey {
            layout: layout_handle,
            path: text_path,
            revision,
            tolerance,
        };
        let runtime = &mut *self.0.borrow_mut();
        let PathTextRuntime {
            baselines,
            placements,
        } = runtime;
        let lines = prepare_path_lines(
            baselines,
            path,
            text_path.path(),
            revision,
            text_path,
            tolerance,
            layout.lines().len(),
        )?;
        let frames = placements.resolve_carets(placement_key, layout, &lines)?;
        Ok(inspect(frames))
    }
}

impl Default for PathBaselineResource {
    fn default() -> Self {
        Self::new(PathBaselineCache::default())
    }
}

trait SegmentSink: Sized {
    fn segment(self, start: Point, end: Point) -> Result<Self, PathBaselineError>;
    fn distance_raw(&self) -> i32;
}

#[derive(Default)]
struct CountSink {
    segments: usize,
    distance_raw: i32,
}

impl SegmentSink for CountSink {
    fn segment(mut self, start: Point, end: Point) -> Result<Self, PathBaselineError> {
        let length_raw = distance_raw(start, end)?;
        if length_raw == 0 {
            return Ok(self);
        }
        self.segments = self
            .segments
            .checked_add(1)
            .ok_or(PathBaselineError::LengthOverflow)?;
        self.distance_raw = self
            .distance_raw
            .checked_add(length_raw)
            .ok_or(PathBaselineError::LengthOverflow)?;
        Ok(self)
    }

    fn distance_raw(&self) -> i32 {
        self.distance_raw
    }
}

struct WriteSink<'a> {
    output: &'a mut [MeasuredSegment],
    written: usize,
    distance_raw: i32,
}

impl<'a> SegmentSink for WriteSink<'a> {
    fn segment(mut self, start: Point, end: Point) -> Result<Self, PathBaselineError> {
        let length_raw = distance_raw(start, end)?;
        if length_raw == 0 {
            return Ok(self);
        }
        self.distance_raw = self
            .distance_raw
            .checked_add(length_raw)
            .ok_or(PathBaselineError::LengthOverflow)?;
        self.output[self.written] = MeasuredSegment {
            end,
            end_distance: from_textflow(self.distance_raw),
        };
        self.written += 1;
        Ok(self)
    }

    fn distance_raw(&self) -> i32 {
        self.distance_raw
    }
}

fn accumulate<S: SegmentSink>(
    start: Point,
    commands: &[PathCmd],
    tolerance: Fixed,
    mut sink: S,
) -> Result<(S, Fixed), PathBaselineError> {
    let mut current = start;
    for command in commands {
        match command {
            PathCmd::LineTo(end) => {
                sink = sink.segment(current, *end)?;
                current = *end;
            }
            PathCmd::QuadTo { ctrl, end } => {
                sink = flatten_quad(current, *ctrl, *end, tolerance, 0, sink)?;
                current = *end;
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                sink = flatten_cubic(current, *ctrl1, *ctrl2, *end, tolerance, 0, sink)?;
                current = *end;
            }
            PathCmd::Close => {
                sink = sink.segment(current, start)?;
                current = start;
            }
            PathCmd::MoveTo(_) => {}
        }
    }

    let length = sink.distance_raw();
    Ok((sink, from_textflow(length)))
}

fn flatten_quad<S: SegmentSink>(
    start: Point,
    ctrl: Point,
    end: Point,
    tolerance: Fixed,
    depth: u8,
    sink: S,
) -> Result<S, PathBaselineError> {
    if depth >= MIN_CURVE_SUBDIVISION_DEPTH
        && curve_excess(&[start, ctrl, end])? <= to_textflow(tolerance)
    {
        return sink.segment(start, end);
    }
    if depth == MAX_SUBDIVISION_DEPTH {
        return Err(PathBaselineError::SubdivisionLimit);
    }
    let start_ctrl = midpoint(start, ctrl);
    let ctrl_end = midpoint(ctrl, end);
    let middle = midpoint(start_ctrl, ctrl_end);
    let sink = flatten_quad(start, start_ctrl, middle, tolerance, depth + 1, sink)?;
    flatten_quad(middle, ctrl_end, end, tolerance, depth + 1, sink)
}

fn flatten_cubic<S: SegmentSink>(
    start: Point,
    ctrl1: Point,
    ctrl2: Point,
    end: Point,
    tolerance: Fixed,
    depth: u8,
    sink: S,
) -> Result<S, PathBaselineError> {
    if depth >= MIN_CURVE_SUBDIVISION_DEPTH
        && curve_excess(&[start, ctrl1, ctrl2, end])? <= to_textflow(tolerance)
    {
        return sink.segment(start, end);
    }
    if depth == MAX_SUBDIVISION_DEPTH {
        return Err(PathBaselineError::SubdivisionLimit);
    }
    let p01 = midpoint(start, ctrl1);
    let p12 = midpoint(ctrl1, ctrl2);
    let p23 = midpoint(ctrl2, end);
    let p012 = midpoint(p01, p12);
    let p123 = midpoint(p12, p23);
    let middle = midpoint(p012, p123);
    let sink = flatten_cubic(start, p01, p012, middle, tolerance, depth + 1, sink)?;
    flatten_cubic(middle, p123, p23, end, tolerance, depth + 1, sink)
}

fn curve_excess(points: &[Point]) -> Result<i32, PathBaselineError> {
    let mut control_length = 0i64;
    for pair in points.windows(2) {
        control_length = control_length
            .checked_add(i64::from(distance_raw(pair[0], pair[1])?))
            .ok_or(PathBaselineError::LengthOverflow)?;
    }
    let chord = i64::from(distance_raw(points[0], points[points.len() - 1])?);
    i32::try_from(control_length - chord).map_err(|_| PathBaselineError::LengthOverflow)
}

fn midpoint(a: Point, b: Point) -> Point {
    Point {
        x: from_textflow(midpoint_raw(to_textflow(a.x), to_textflow(b.x))),
        y: from_textflow(midpoint_raw(to_textflow(a.y), to_textflow(b.y))),
    }
}

fn midpoint_raw(a: i32, b: i32) -> i32 {
    ((i64::from(a) + i64::from(b)) / 2) as i32
}

fn distance_raw(a: Point, b: Point) -> Result<i32, PathBaselineError> {
    let dx = i128::from(to_textflow(b.x)) - i128::from(to_textflow(a.x));
    let dy = i128::from(to_textflow(b.y)) - i128::from(to_textflow(a.y));
    let squared = (dx * dx + dy * dy) as u128;
    i32::try_from(rounded_sqrt(squared)).map_err(|_| PathBaselineError::LengthOverflow)
}

fn rounded_sqrt(value: u128) -> u128 {
    let floor = integer_sqrt(value);
    let lower = value - floor * floor;
    let next = floor + 1;
    let upper = next * next - value;
    if upper < lower { next } else { floor }
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut result = 1u128 << ((128 - value.leading_zeros() as usize).div_ceil(2));
    loop {
        let next = (result + value / result) / 2;
        if next >= result {
            return result;
        }
        result = next;
    }
}

fn sample_segment(start: Point, end: Point, length_raw: i32, offset_raw: i32) -> BaselineSample {
    let dx = i64::from(to_textflow(end.x)) - i64::from(to_textflow(start.x));
    let dy = i64::from(to_textflow(end.y)) - i64::from(to_textflow(start.y));
    let length = i64::from(length_raw);
    let x = i64::from(to_textflow(start.x)) + dx * i64::from(offset_raw) / length;
    let y = i64::from(to_textflow(start.y)) + dy * i64::from(offset_raw) / length;
    BaselineSample {
        position: Point {
            x: from_textflow(x as i32),
            y: from_textflow(y as i32),
        },
        unit_tangent: segment_tangent(start, end, length_raw),
    }
}

fn sample_measured_segment(
    path_start: Point,
    segments: &[MeasuredSegment],
    index: usize,
    length_raw: i32,
    offset_raw: i32,
    smooth_tangents: bool,
) -> BaselineSample {
    let start = if index == 0 {
        path_start
    } else {
        segments[index - 1].end
    };
    let end = segments[index].end;
    let mut sample = sample_segment(start, end, length_raw, offset_raw);
    if !smooth_tangents {
        return sample;
    }

    let current = sample.unit_tangent;
    let incoming = index.checked_sub(1).map_or(current, |previous| {
        measured_segment_tangent(path_start, segments, previous)
    });
    let outgoing = segments.get(index + 1).map_or(current, |_| {
        measured_segment_tangent(path_start, segments, index + 1)
    });
    let start_tangent = Point {
        x: incoming.x + current.x,
        y: incoming.y + current.y,
    };
    let end_tangent = Point {
        x: current.x + outgoing.x,
        y: current.y + outgoing.y,
    };
    let remaining = i64::from(length_raw - offset_raw);
    let offset = i64::from(offset_raw);
    let length = i64::from(length_raw);
    let interpolated = Point {
        x: from_textflow(
            ((i64::from(to_textflow(start_tangent.x)) * remaining
                + i64::from(to_textflow(end_tangent.x)) * offset)
                / length) as i32,
        ),
        y: from_textflow(
            ((i64::from(to_textflow(start_tangent.y)) * remaining
                + i64::from(to_textflow(end_tangent.y)) * offset)
                / length) as i32,
        ),
    };
    sample.unit_tangent = normalize_q8(interpolated, current);
    sample
}

fn measured_segment_tangent(
    path_start: Point,
    segments: &[MeasuredSegment],
    index: usize,
) -> Point {
    let start = index
        .checked_sub(1)
        .map_or(path_start, |previous| segments[previous].end);
    let start_distance = index
        .checked_sub(1)
        .map_or(0, |previous| to_textflow(segments[previous].end_distance));
    let end = segments[index];
    segment_tangent(
        start,
        end.end,
        to_textflow(end.end_distance) - start_distance,
    )
}

fn segment_tangent(start: Point, end: Point, length_raw: i32) -> Point {
    if length_raw <= 0 {
        return Point::new(Fixed::ONE, Fixed::ZERO);
    }
    let dx = i64::from(to_textflow(end.x)) - i64::from(to_textflow(start.x));
    let dy = i64::from(to_textflow(end.y)) - i64::from(to_textflow(start.y));
    let length = i64::from(length_raw);
    Point {
        x: from_textflow((dx * 256 / length) as i32),
        y: from_textflow((dy * 256 / length) as i32),
    }
}

fn normalize_q8(direction: Point, fallback: Point) -> Point {
    let dx = i64::from(to_textflow(direction.x));
    let dy = i64::from(to_textflow(direction.y));
    let squared = (dx * dx + dy * dy) as u64;
    if squared == 0 {
        return fallback;
    }
    let length = rounded_sqrt_u64(squared) as i64;
    Point {
        x: from_textflow((dx * 256 / length) as i32),
        y: from_textflow((dy * 256 / length) as i32),
    }
}

fn rounded_sqrt_u64(value: u64) -> u64 {
    let floor = integer_sqrt_u64(value);
    let lower = value - floor * floor;
    let next = floor + 1;
    let upper = next * next - value;
    if upper < lower { next } else { floor }
}

fn integer_sqrt_u64(mut value: u64) -> u64 {
    let mut result = 0;
    let mut bit = 1_u64 << 62;
    while bit > value {
        bit >>= 2;
    }
    while bit != 0 {
        if value >= result + bit {
            value -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::layout::{TextLayoutCache, TextLayoutRequest};
    use textflow::bidi::BaseDirection;
    use textflow::layout::{Alignment, Overflow, TextSpacing, WrapMode};
    use textflow::shaping::{
        FlowPoint, FontAccessError, FontId, FontMetrics, GlyphId, GlyphSource, SimpleTypeface,
    };

    fn point(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    fn path_id(path: Path) -> PathId {
        let mut store = PathStore::new(1).unwrap();
        store.insert(path).unwrap()
    }

    struct Mono;

    impl GlyphSource for Mono {
        fn id(&self) -> FontId {
            FontId::new(1)
        }

        fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
            Ok(FontMetrics {
                units_per_em: 256,
                ascender: 192,
                descender: -64,
                line_gap: 0,
            })
        }

        fn glyph_for(&self, character: char) -> Result<Option<GlyphId>, FontAccessError> {
            Ok(Some(GlyphId::new(character as u16)))
        }

        fn glyph_advance(&self, _glyph: GlyphId) -> Result<FlowPoint, FontAccessError> {
            Ok(FlowPoint { x: 256, y: 0 })
        }
    }

    #[test]
    fn idle_path_runtime_reserves_no_heap_storage() {
        let resource = PathBaselineResource::default();
        let runtime = resource.0.borrow();

        assert_eq!(runtime.baselines.entries.capacity(), 0);
        assert_eq!(runtime.baselines.segments.capacity(), 0);
        assert_eq!(runtime.placements.glyphs.entries.capacity(), 0);
        assert_eq!(runtime.placements.glyphs.values.capacity(), 0);
        assert_eq!(runtime.placements.carets.entries.capacity(), 0);
        assert_eq!(runtime.placements.carets.values.capacity(), 0);
        assert_eq!(core::mem::size_of::<textflow::placement::GlyphFrame>(), 16);
        assert_eq!(core::mem::size_of::<textflow::placement::CaretFrame>(), 16);
    }

    #[test]
    fn line_path_samples_commands_without_measured_storage() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
        ]);
        let measure = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 4)).unwrap();
        assert!(measure.is_line_only());
        let line = measure.line().unwrap();
        assert_eq!(line.length(), Fixed::from_int(20));

        let mut cursor = line.cursor();
        assert_eq!(
            cursor.sample_forward(Fixed::from_int(5)).unwrap(),
            BaselineSample {
                position: point(5, 0),
                unit_tangent: point(1, 0),
            }
        );
        assert_eq!(
            cursor.sample_forward(Fixed::from_int(10)).unwrap(),
            BaselineSample {
                position: point(10, 0),
                unit_tangent: point(0, 1),
            }
        );
    }

    #[test]
    fn reverse_window_scans_segments_once_in_reverse_order() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
        ]);
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );
        let id = path_id(path.clone());
        let window = BaselineWindow::new(
            baseline,
            TextPath::new(id).with_direction(PathDirection::Reverse),
        )
        .unwrap();
        let mut cursor = window.cursor();

        assert_eq!(
            cursor.sample_forward(0).unwrap(),
            textflow::placement::BaselineSample {
                position: FlowPoint {
                    x: 10 << 8,
                    y: 10 << 8,
                },
                unit_tangent: FlowPoint { x: 0, y: -256 },
            }
        );
        assert_eq!(
            cursor.sample_forward(10 << 8).unwrap(),
            textflow::placement::BaselineSample {
                position: FlowPoint { x: 10 << 8, y: 0 },
                unit_tangent: FlowPoint { x: -256, y: 0 },
            }
        );
    }

    #[test]
    fn offset_advances_forward_and_reverse_window_origins() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(20, 0)),
        ]);
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );
        let id = path_id(path.clone());
        let forward =
            BaselineWindow::new(baseline, TextPath::new(id).with_offset(Fixed::from_int(4)))
                .unwrap();
        let reverse = BaselineWindow::new(
            baseline,
            TextPath::new(id)
                .with_offset(Fixed::from_int(4))
                .with_direction(PathDirection::Reverse),
        )
        .unwrap();

        assert_eq!(forward.length(), 16 << 8);
        assert_eq!(reverse.length(), 16 << 8);
        assert_eq!(
            forward.cursor().sample_forward(0).unwrap().position,
            FlowPoint { x: 4 << 8, y: 0 }
        );
        assert_eq!(
            reverse.cursor().sample_forward(0).unwrap().position,
            FlowPoint { x: 16 << 8, y: 0 }
        );
    }

    #[test]
    fn reverse_window_supports_measured_curves() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(5, 10),
                end: point(10, 0),
            },
        ]);
        let measure = PathMeasure::new(&path, 0, DEFAULT_TOLERANCE).unwrap();
        let requirements = measure.requirements().unwrap();
        let mut segments = alloc::vec![EMPTY_SEGMENT; requirements.segments];
        let measured = measure.measure_into(&mut segments).unwrap();
        let id = path_id(path.clone());
        let window = BaselineWindow::new(
            PathBaseline::Measured(measured),
            TextPath::new(id).with_direction(PathDirection::Reverse),
        )
        .unwrap();
        let mut cursor = window.cursor();

        let start = cursor.sample_forward(0).unwrap();
        let end = cursor
            .sample_forward(to_textflow(requirements.length))
            .unwrap();
        assert_eq!(start.position, FlowPoint { x: 10 << 8, y: 0 });
        assert_eq!(end.position, FlowPoint { x: 0, y: 0 });
        assert!(start.unit_tangent.x < 0);
        assert!(end.unit_tangent.x < 0);
    }

    #[test]
    fn closed_seam_wraps_without_restarting_each_sample() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
            PathCmd::LineTo(point(0, 10)),
            PathCmd::Close,
        ]);
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );
        let id = path_id(path.clone());
        let window =
            BaselineWindow::new(baseline, TextPath::new(id).with_seam(Fixed::from_int(30)))
                .unwrap();
        let mut cursor = window.cursor();

        assert_eq!(
            cursor.sample_forward(0).unwrap().position,
            FlowPoint { x: 0, y: 10 << 8 }
        );
        assert_eq!(
            cursor.sample_forward(20 << 8).unwrap().position,
            FlowPoint { x: 10 << 8, y: 0 }
        );
    }

    #[test]
    fn offset_is_applied_after_the_closed_seam() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
            PathCmd::LineTo(point(0, 10)),
            PathCmd::Close,
        ]);
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );
        let id = path_id(path.clone());
        let window = BaselineWindow::new(
            baseline,
            TextPath::new(id)
                .with_seam(Fixed::from_int(38))
                .with_offset(Fixed::from_int(4)),
        )
        .unwrap();

        assert_eq!(
            window.cursor().sample_forward(0).unwrap().position,
            FlowPoint { x: 2 << 8, y: 0 }
        );
    }

    #[test]
    fn reverse_closed_seam_wraps_in_one_monotonic_domain() {
        use textflow::placement::{BaselineCursor as _, TextBaseline as _};

        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
            PathCmd::LineTo(point(0, 10)),
            PathCmd::Close,
        ]);
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );
        let id = path_id(path.clone());
        let window = BaselineWindow::new(
            baseline,
            TextPath::new(id)
                .with_direction(PathDirection::Reverse)
                .with_seam(Fixed::from_int(10)),
        )
        .unwrap();
        let mut cursor = window.cursor();

        assert_eq!(
            cursor.sample_forward(0).unwrap().position,
            FlowPoint { x: 10 << 8, y: 0 }
        );
        assert_eq!(
            cursor.sample_forward(20 << 8).unwrap().position,
            FlowPoint { x: 0, y: 10 << 8 }
        );
    }

    #[test]
    fn seam_requires_closed_geometry() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
        ]);
        let id = path_id(path.clone());
        let baseline = PathBaseline::Line(
            PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
                .unwrap()
                .line()
                .unwrap(),
        );

        assert!(matches!(
            BaselineWindow::new(baseline, TextPath::new(id).with_seam(Fixed::from_int(5)),),
            Err(PathBaselineError::OpenSeam)
        ));
    }

    #[test]
    fn selected_subpath_does_not_measure_siblings() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(100, 0)),
            PathCmd::MoveTo(point(7, 9)),
            PathCmd::LineTo(point(7, 19)),
        ]);
        let measure = PathMeasure::new(&path, 1, Fixed::ONE).unwrap();
        let line = measure.line().unwrap();
        assert_eq!(line.length(), Fixed::from_int(10));
        assert_eq!(
            line.cursor().sample_forward(Fixed::ZERO).unwrap().position,
            point(7, 9)
        );
    }

    #[test]
    fn close_returns_to_the_selected_subpath_start() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(3, 0)),
            PathCmd::LineTo(point(3, 4)),
            PathCmd::Close,
        ]);
        let line = PathMeasure::new(&path, 0, Fixed::ONE)
            .unwrap()
            .line()
            .unwrap();
        assert_eq!(line.length(), Fixed::from_int(12));
        assert_eq!(
            line.cursor()
                .sample_forward(Fixed::from_int(12))
                .unwrap()
                .position,
            point(0, 0)
        );
    }

    #[test]
    fn curve_measurement_is_atomic_when_capacity_is_short() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(50, 100),
                end: point(100, 0),
            },
        ]);
        let measure = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 8)).unwrap();
        let requirements = measure.requirements().unwrap();
        assert!(requirements.segments > 1);
        let sentinel = MeasuredSegment {
            end: point(-1, -1),
            end_distance: Fixed::MAX,
        };
        let mut output = alloc::vec![sentinel; requirements.segments - 1];
        match measure.measure_into(&mut output) {
            Err(PathBaselineError::InsufficientCapacity { required, provided }) => {
                assert_eq!(required, requirements.segments);
                assert_eq!(provided, requirements.segments - 1);
            }
            _ => panic!("short output must fail before writing"),
        }
        assert!(output.iter().all(|segment| *segment == sentinel));
    }

    #[test]
    fn tighter_tolerance_produces_at_least_as_many_segments() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::CubicTo {
                ctrl1: point(0, 100),
                ctrl2: point(100, 100),
                end: point(100, 0),
            },
        ]);
        let loose = PathMeasure::new(&path, 0, Fixed::from_int(4))
            .unwrap()
            .requirements()
            .unwrap();
        let tight = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 8))
            .unwrap()
            .requirements()
            .unwrap();
        assert!(tight.segments >= loose.segments);
        assert!(tight.length >= loose.length);
    }

    #[test]
    fn measured_curve_interpolates_tangents_across_flattened_segments() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::CubicTo {
                ctrl1: point(0, 100),
                ctrl2: point(100, 100),
                end: point(100, 0),
            },
        ]);
        let measure = PathMeasure::new(&path, 0, Fixed::from_int(2)).unwrap();
        let requirements = measure.requirements().unwrap();
        let mut output = alloc::vec![EMPTY_SEGMENT; requirements.segments];
        let baseline = measure.measure_into(&mut output).unwrap();
        let boundary = baseline.segments[requirements.segments / 2 - 1].end_distance;
        let epsilon = Fixed::from_ratio(1, 256);
        let mut cursor = baseline.cursor();
        let before = cursor.sample_forward(boundary - epsilon).unwrap();
        let at = cursor.sample_forward(boundary).unwrap();
        let after = cursor.sample_forward(boundary + epsilon).unwrap();

        for adjacent in [(before, at), (at, after)] {
            assert!((adjacent.0.unit_tangent.x - adjacent.1.unit_tangent.x).abs() <= epsilon);
            assert!((adjacent.0.unit_tangent.y - adjacent.1.unit_tangent.y).abs() <= epsilon);
        }
    }

    #[test]
    fn measured_cursor_uses_outgoing_tangent_at_segment_boundary() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(10, 10)),
        ]);
        let measure = PathMeasure::new(&path, 0, Fixed::ONE).unwrap();
        let requirements = measure.requirements().unwrap();
        let mut output = alloc::vec![
            MeasuredSegment {
                end: Point::ZERO,
                end_distance: Fixed::ZERO,
            };
            requirements.segments
        ];
        let baseline = measure.measure_into(&mut output).unwrap();
        assert_eq!(baseline.length(), Fixed::from_int(20));
        assert_eq!(
            baseline
                .cursor()
                .sample_forward(Fixed::from_int(10))
                .unwrap()
                .unit_tangent,
            point(0, 1)
        );
    }

    #[test]
    fn degenerate_and_out_of_order_queries_fail_explicitly() {
        let empty = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(1, 1)),
            PathCmd::LineTo(point(1, 1)),
        ]);
        assert!(matches!(
            PathMeasure::new(&empty, 0, Fixed::ONE)
                .unwrap()
                .requirements(),
            Err(PathBaselineError::Empty)
        ));

        let line = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
        ]);
        let baseline = PathMeasure::new(&line, 0, Fixed::ONE)
            .unwrap()
            .line()
            .unwrap();
        let mut cursor = baseline.cursor();
        cursor.sample_forward(Fixed::from_int(8)).unwrap();
        assert!(matches!(
            cursor.sample_forward(Fixed::from_int(7)),
            Err(PathBaselineError::DistanceOrder { .. })
        ));
    }

    #[test]
    fn zero_segments_and_cusps_use_the_next_nonzero_tangent() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(0, 0)),
            PathCmd::LineTo(point(10, 0)),
            PathCmd::LineTo(point(0, 0)),
            PathCmd::LineTo(point(0, 0)),
        ]);
        let baseline = PathMeasure::new(&path, 0, Fixed::ONE)
            .unwrap()
            .line()
            .unwrap();
        let mut cursor = baseline.cursor();

        assert_eq!(
            cursor.sample_forward(Fixed::ZERO).unwrap().unit_tangent,
            point(1, 0)
        );
        assert_eq!(
            cursor
                .sample_forward(Fixed::from_int(10))
                .unwrap()
                .unit_tangent,
            point(-1, 0)
        );
        assert_eq!(
            cursor
                .sample_forward(Fixed::from_int(20))
                .unwrap()
                .unit_tangent,
            point(-1, 0)
        );
    }

    #[test]
    fn extreme_coordinate_span_reports_length_overflow() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(Point {
                x: Fixed::MIN,
                y: Fixed::ZERO,
            }),
            PathCmd::LineTo(Point {
                x: Fixed::MAX,
                y: Fixed::ZERO,
            }),
        ]);
        assert_eq!(
            PathMeasure::new(&path, 0, Fixed::ONE)
                .unwrap()
                .requirements(),
            Err(PathBaselineError::LengthOverflow)
        );
    }

    #[test]
    fn measured_segment_storage_is_compact() {
        assert_eq!(core::mem::size_of::<MeasuredSegment>(), 12);
    }

    #[test]
    fn line_paths_never_enter_the_measured_arena() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::LineTo(point(100, 0)),
        ]);
        let mut cache = PathBaselineCache::new(1_024, 4);
        let key = MeasurementKey {
            path: path_id(path.clone()),
            revision: None,
            subpath: 0,
            tolerance: DEFAULT_TOLERANCE,
        };
        assert!(matches!(
            cache.resolve(key, &path).unwrap(),
            PathBaseline::Line(_)
        ));
        assert!(cache.entries.is_empty());
        assert!(cache.segments.is_empty());
    }

    #[test]
    fn measured_cache_reuses_matching_revision_and_tolerance() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(50, 100),
                end: point(100, 0),
            },
        ]);
        let mut cache = PathBaselineCache::new(8 * 1_024, 4);
        let key = MeasurementKey {
            path: path_id(path.clone()),
            revision: Some(PathRevision::default()),
            subpath: 0,
            tolerance: DEFAULT_TOLERANCE,
        };
        let first_length = cache.resolve(key, &path).unwrap().length();
        let segment_count = cache.segments.len();
        let second_length = cache.resolve(key, &path).unwrap().length();
        assert_eq!(second_length, first_length);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.segments.len(), segment_count);
    }

    #[test]
    fn animated_path_revisions_replace_stale_measurement_and_placement_entries() {
        let face = SimpleTypeface::new(&Mono);
        let mut layouts = TextLayoutCache::default();
        let handle = layouts
            .layout(
                TextLayoutRequest {
                    text: "animated",
                    max_width: 100 << 8,
                    width: Some(100 << 8),
                    max_lines: 1,
                    line_height: 256,
                    baseline: 192,
                    direction: BaseDirection::LeftToRight,
                    wrap: WrapMode::NoWrap,
                    alignment: Alignment::Start,
                    overflow: Overflow::Clip,
                    spacing: TextSpacing::default(),
                    features: &[],
                    line_widths: None,
                },
                &[&face],
            )
            .unwrap();
        let layout = layouts.get(handle).unwrap();
        let mut store = PathStore::new(1).unwrap();
        let path = store
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(point(0, 0)),
                PathCmd::QuadTo {
                    ctrl: point(50, 40),
                    end: point(100, 0),
                },
            ]))
            .unwrap();
        let text_path = TextPath::new(path).with_range(Fixed::ZERO..Fixed::from_int(100));
        let resource = PathBaselineResource::default();

        for frame in 0..64 {
            resource.begin_frame();
            store
                .edit(path, |path| {
                    path.set_command(
                        1,
                        PathCmd::QuadTo {
                            ctrl: point(50, 40 + frame),
                            end: point(100, 0),
                        },
                    )
                    .unwrap();
                })
                .unwrap();
            resource
                .with_glyph_frames(
                    &store,
                    text_path,
                    DEFAULT_TOLERANCE,
                    handle,
                    &layout,
                    |_| {},
                )
                .unwrap();
        }

        let runtime = resource.0.borrow();
        assert_eq!(runtime.baselines.entries.len(), 1);
        assert_eq!(runtime.placements.glyphs.entries.len(), 1);
    }

    #[test]
    fn measured_cache_compacts_after_lru_eviction() {
        let first = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(20, 40),
                end: point(40, 0),
            },
        ]);
        let second = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::CubicTo {
                ctrl1: point(0, 40),
                ctrl2: point(40, 40),
                end: point(40, 0),
            },
        ]);
        let mut cache = PathBaselineCache::new(8 * 1_024, 1);
        let first_id = path_id(first.clone());
        let mut store = PathStore::new(2).unwrap();
        store.insert(Path::new()).unwrap();
        let second_id = store.insert(second.clone()).unwrap();
        let key = |path| MeasurementKey {
            path,
            revision: None,
            subpath: 0,
            tolerance: DEFAULT_TOLERANCE,
        };
        cache.resolve(key(first_id), &first).unwrap();
        cache.begin_frame();
        cache.resolve(key(second_id), &second).unwrap();

        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.entries[0].key, key(second_id));
        assert_eq!(cache.entries[0].segments.start, 0);
        assert_eq!(cache.entries[0].segments.end, cache.segments.len());
    }

    #[test]
    fn measured_cache_rejects_an_entry_larger_than_its_budget() {
        let path = Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(50, 100),
                end: point(100, 0),
            },
        ]);
        let mut cache = PathBaselineCache::new(1, 1);
        let key = MeasurementKey {
            path: path_id(path.clone()),
            revision: None,
            subpath: 0,
            tolerance: DEFAULT_TOLERANCE,
        };
        assert!(matches!(
            cache.resolve(key, &path),
            Err(PathBaselineError::CacheBudget { budget: 1, .. })
        ));
        assert!(cache.entries.is_empty());
        assert!(cache.segments.is_empty());
    }

    #[test]
    fn placement_cache_pairs_layout_glyphs_with_path_frames() {
        let face = SimpleTypeface::new(&Mono);
        let mut layouts = TextLayoutCache::default();
        let handle = layouts
            .layout(
                TextLayoutRequest {
                    text: "ab",
                    max_width: 100 << 8,
                    width: Some(100 << 8),
                    max_lines: 1,
                    line_height: 256,
                    baseline: 192,
                    direction: BaseDirection::LeftToRight,
                    wrap: WrapMode::NoWrap,
                    alignment: Alignment::Start,
                    overflow: Overflow::Clip,
                    spacing: TextSpacing::default(),
                    features: &[],
                    line_widths: None,
                },
                &[&face],
            )
            .unwrap();
        let layout = layouts.get(handle).unwrap();
        let mut store = PathStore::new(1).unwrap();
        let path = store
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(point(10, 20)),
                PathCmd::LineTo(point(110, 20)),
            ]))
            .unwrap();
        let resource = PathBaselineResource::default();
        let text_path = TextPath::new(path);

        resource
            .with_glyph_frames(
                &store,
                text_path,
                DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| {
                    assert_eq!(frames.len(), 2);
                    assert_eq!(
                        frames[0].local_origin,
                        FlowPoint {
                            x: 10 << 8,
                            y: 20 << 8
                        }
                    );
                    assert_eq!(frames[0].unit_tangent, FlowPoint { x: 256, y: 0 });
                    assert_eq!(
                        frames[1].local_origin,
                        FlowPoint {
                            x: 11 << 8,
                            y: 20 << 8
                        }
                    );
                },
            )
            .unwrap();

        {
            let runtime = resource.0.borrow();
            assert_eq!(runtime.placements.glyphs.entries.len(), 1);
            assert!(runtime.placements.carets.entries.is_empty());
            assert!(runtime.placements.carets.values.is_empty());
        }

        resource
            .with_caret_frames(
                &store,
                text_path,
                DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| {
                    assert_eq!(frames.len(), 3);
                    assert_eq!(frames[0].local_origin.x, 10 << 8);
                    assert_eq!(frames[1].local_origin.x, 11 << 8);
                    assert_eq!(frames[2].local_origin.x, 12 << 8);
                    assert!(
                        frames
                            .iter()
                            .all(|frame| frame.unit_tangent == FlowPoint { x: 256, y: 0 })
                    );
                },
            )
            .unwrap();

        let runtime = resource.0.borrow();
        assert_eq!(runtime.placements.glyphs.entries.len(), 1);
        assert_eq!(runtime.placements.carets.entries.len(), 1);
    }

    #[test]
    fn consecutive_subpaths_constrain_and_place_independent_lines() {
        let face = SimpleTypeface::new(&Mono);
        let mut layouts = TextLayoutCache::default();
        let mut store = PathStore::new(1).unwrap();
        let path = store
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(point(10, 20)),
                PathCmd::LineTo(point(12, 20)),
                PathCmd::MoveTo(point(30, 40)),
                PathCmd::LineTo(point(31, 40)),
            ]))
            .unwrap();
        let resource = PathBaselineResource::default();
        let text_path = TextPath::new(path);
        let handle = resource
            .with_lines(&store, text_path, DEFAULT_TOLERANCE, 2, |widths| {
                layouts.layout(
                    TextLayoutRequest {
                        text: "abc",
                        max_width: i32::MAX,
                        width: None,
                        max_lines: 2,
                        line_height: 256,
                        baseline: 192,
                        direction: BaseDirection::LeftToRight,
                        wrap: WrapMode::Grapheme,
                        alignment: Alignment::Start,
                        overflow: Overflow::Clip,
                        spacing: TextSpacing::default(),
                        features: &[],
                        line_widths: Some(widths),
                    },
                    &[&face],
                )
            })
            .unwrap()
            .unwrap();
        let layout = layouts.get(handle).unwrap();
        assert_eq!(layout.lines().len(), 2);
        assert_eq!(layout.lines()[0].advance(), 2 << 8);
        assert_eq!(layout.lines()[1].advance(), 1 << 8);

        resource
            .with_glyph_frames(
                &store,
                text_path,
                DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| {
                    assert_eq!(frames.len(), 3);
                    assert_eq!(
                        frames[0].local_origin,
                        FlowPoint {
                            x: 10 << 8,
                            y: 20 << 8
                        }
                    );
                    assert_eq!(
                        frames[1].local_origin,
                        FlowPoint {
                            x: 11 << 8,
                            y: 20 << 8
                        }
                    );
                    assert_eq!(
                        frames[2].local_origin,
                        FlowPoint {
                            x: 30 << 8,
                            y: 40 << 8
                        }
                    );
                },
            )
            .unwrap();
    }

    #[test]
    fn available_subpaths_bound_ellipsis_to_the_last_path_line() {
        let face = SimpleTypeface::new(&Mono);
        let mut layouts = TextLayoutCache::default();
        let mut store = PathStore::new(1).unwrap();
        let path = store
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(point(10, 20)),
                PathCmd::LineTo(point(13, 20)),
            ]))
            .unwrap();
        let resource = PathBaselineResource::default();
        let text_path = TextPath::new(path);
        let handle = resource
            .with_lines(&store, text_path, DEFAULT_TOLERANCE, 2, |widths| {
                layouts.layout(
                    TextLayoutRequest {
                        text: "ab cd",
                        max_width: i32::MAX,
                        width: None,
                        max_lines: widths.line_count(),
                        line_height: 256,
                        baseline: 192,
                        direction: BaseDirection::LeftToRight,
                        wrap: WrapMode::Word,
                        alignment: Alignment::Start,
                        overflow: Overflow::Ellipsis,
                        spacing: TextSpacing::default(),
                        features: &[],
                        line_widths: Some(widths),
                    },
                    &[&face],
                )
            })
            .unwrap()
            .unwrap();
        let layout = layouts.get(handle).unwrap();

        assert_eq!(layout.lines().len(), 1);
        assert_eq!(layout.glyphs().len(), 3);
        assert_eq!(
            layout.glyphs()[2].glyph_id(),
            GlyphId::new('\u{2026}' as u16)
        );
        resource
            .with_glyph_frames(
                &store,
                text_path,
                DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| assert_eq!(frames.len(), 3),
            )
            .unwrap();
    }

    #[test]
    fn rtl_ellipsis_occupies_the_visual_start_of_the_path() {
        let face = SimpleTypeface::new(&Mono);
        let mut layouts = TextLayoutCache::default();
        let mut store = PathStore::new(1).unwrap();
        let path = store
            .insert(Path::from_owned(alloc::vec![
                PathCmd::MoveTo(point(10, 20)),
                PathCmd::LineTo(point(13, 20)),
            ]))
            .unwrap();
        let resource = PathBaselineResource::default();
        let text_path = TextPath::new(path);
        let handle = resource
            .with_lines(&store, text_path, DEFAULT_TOLERANCE, 1, |widths| {
                layouts.layout(
                    TextLayoutRequest {
                        text: "ab cd",
                        max_width: i32::MAX,
                        width: None,
                        max_lines: widths.line_count(),
                        line_height: 256,
                        baseline: 192,
                        direction: BaseDirection::RightToLeft,
                        wrap: WrapMode::Word,
                        alignment: Alignment::Start,
                        overflow: Overflow::Ellipsis,
                        spacing: TextSpacing::default(),
                        features: &[],
                        line_widths: Some(widths),
                    },
                    &[&face],
                )
            })
            .unwrap()
            .unwrap();
        let layout = layouts.get(handle).unwrap();

        assert_eq!(
            layout.glyphs()[0].glyph_id(),
            GlyphId::new('\u{2026}' as u16)
        );
        resource
            .with_glyph_frames(
                &store,
                text_path,
                DEFAULT_TOLERANCE,
                handle,
                &layout,
                |frames| {
                    assert_eq!(frames[0].local_origin.x, 10 << 8);
                    assert!(
                        frames
                            .windows(2)
                            .all(|pair| pair[0].local_origin.x <= pair[1].local_origin.x)
                    );
                },
            )
            .unwrap();
    }
}
