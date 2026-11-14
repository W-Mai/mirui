use super::types::{
    GRID_COLUMNS, GRID_ROWS, GridPoint, Level, MAX_MIRRORS, MAX_TRACE_POINTS, MirrorOrientation,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TraceStop {
    #[default]
    Edge,
    Wall,
    Receiver,
    Cycle,
    Budget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Trace {
    pub(super) points: [GridPoint; MAX_TRACE_POINTS],
    pub(super) len: u8,
    pub(crate) hit_mirrors: u8,
    pub(crate) lit_lamps: u8,
    pub(crate) solved: bool,
    pub(crate) stop: TraceStop,
    pub(crate) steps: u8,
}

impl Default for Trace {
    fn default() -> Self {
        Self {
            points: [GridPoint::default(); MAX_TRACE_POINTS],
            len: 0,
            hit_mirrors: 0,
            lit_lamps: 0,
            solved: false,
            stop: TraceStop::Edge,
            steps: 0,
        }
    }
}

impl Trace {
    pub(crate) fn points(&self) -> &[GridPoint] {
        &self.points[..usize::from(self.len)]
    }

    pub(super) fn push(&mut self, point: GridPoint) {
        let index = usize::from(self.len);
        if index < MAX_TRACE_POINTS {
            self.points[index] = point;
            self.len += 1;
        }
    }
}

/// The direction a beam leaves a mirror, for directions right, down, left, up.
const fn reflect(direction: u8, orientation: MirrorOrientation) -> u8 {
    const SLASH: [u8; 4] = [3, 2, 1, 0];
    const BACKSLASH: [u8; 4] = [1, 0, 3, 2];
    match orientation {
        MirrorOrientation::Slash => SLASH[direction as usize],
        MirrorOrientation::Backslash => BACKSLASH[direction as usize],
    }
}

/// Follows the beam from the source. It stops at the receiver, a wall, the
/// edge, or a cell it already crossed in the same direction; the receiver
/// only counts once every lamp on the way is lit.
pub(crate) fn trace(level: &Level, orientations: &[MirrorOrientation; MAX_MIRRORS]) -> Trace {
    const DX: [i8; 4] = [1, 0, -1, 0];
    const DY: [i8; 4] = [0, 1, 0, -1];

    let mut result = Trace::default();
    let mut visited = [false; (GRID_COLUMNS as usize) * (GRID_ROWS as usize) * 4];
    let mut x = -1;
    let mut y = level.source_row;
    let mut direction = 0_u8;
    result.push(GridPoint::new(x, y));

    for step in 0..140 {
        x += DX[usize::from(direction)];
        y += DY[usize::from(direction)];
        result.push(GridPoint::new(x, y));
        if !(0..GRID_COLUMNS).contains(&x) || !(0..GRID_ROWS).contains(&y) {
            result.stop = TraceStop::Edge;
            break;
        }

        let state =
            ((y as usize * GRID_COLUMNS as usize + x as usize) * 4) + usize::from(direction);
        if visited[state] {
            result.stop = TraceStop::Cycle;
            break;
        }
        visited[state] = true;
        result.steps = result.steps.saturating_add(1);

        let point = GridPoint::new(x, y);
        if point == level.target {
            result.solved = result.lit_lamps == level.all_lamps();
            result.stop = TraceStop::Receiver;
            break;
        }
        if level.walls.contains(&point) {
            result.stop = TraceStop::Wall;
            break;
        }
        if let Some(lamp) = level.lamp_at(point) {
            result.lit_lamps |= 1 << lamp;
        }
        if let Some(index) = level.mirror_at(point) {
            result.hit_mirrors |= 1 << index;
            direction = reflect(direction, orientations[index]);
        } else if let Some(orientation) = level.fixed_at(point) {
            direction = reflect(direction, orientation);
        }
        if step == 139 {
            result.stop = TraceStop::Budget;
        }
    }
    result
}
