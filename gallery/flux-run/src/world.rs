use mirui::types::Fixed;

pub const CORES: usize = 5;
pub const GUARDS: usize = 3;
const INITIAL_SIDE: usize = 12;
const MAX_SIDE: usize = 22;
const MIN_DOCK: usize = 5;
const MAX_CELLS: usize = MAX_SIDE * MAX_SIDE;
pub(crate) const CELL_SIZE: i32 = 6;
pub(crate) const INITIAL_EXTENT: i32 = INITIAL_SIDE as i32 * CELL_SIZE;
pub const SPAWN: Location = Location::new(13, 13);
pub(crate) const MAX_DOCKS: usize = CORES;
pub(crate) const MAX_BRIDGES: usize = MAX_DOCKS - 1;
pub(crate) const BRIDGE_HEIGHT: i32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dock {
    pub min: [usize; 2],
    pub end: [usize; 2],
}

impl Dock {
    fn overlaps(self, other: Self) -> bool {
        (0..2).all(|i| self.min[i] < other.end[i] && other.min[i] < self.end[i])
    }

    pub fn center(self) -> Location {
        Location::new(
            ((self.min[0] + self.end[0]) as i32 * CELL_SIZE) / 2,
            ((self.min[1] + self.end[1]) as i32 * CELL_SIZE) / 2,
        )
    }

    fn fill(self, rows: &mut [u32; MAX_SIDE]) {
        let mask = ((1 << (self.end[0] - self.min[0])) - 1) << self.min[0];
        for row in &mut rows[self.min[1]..self.end[1]] {
            *row |= mask;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bridge {
    pub axis: usize,
    pub start: usize,
    pub end: usize,
    pub lane: usize,
}

impl Bridge {
    pub fn footprint(self) -> Dock {
        let mut min = [self.lane - 1; 2];
        let mut end = [self.lane + 2; 2];
        min[self.axis] = self.start;
        end[self.axis] = self.end;
        Dock { min, end }
    }

    pub fn center(self) -> Location {
        self.footprint().center()
    }

    pub fn height(self, at: [Fixed; 2]) -> Option<Fixed> {
        let bounds = self.footprint();
        let coordinate = |cell: usize| Fixed::from_int(cell as i32 * CELL_SIZE);
        if (0..2).any(|i| at[i] < coordinate(bounds.min[i]) || at[i] >= coordinate(bounds.end[i])) {
            return None;
        }
        let along = at[self.axis];
        Some(
            (along - coordinate(self.start))
                .min(coordinate(self.end) - along)
                .min(Fixed::from_int(BRIDGE_HEIGHT)),
        )
    }

    pub fn length(self) -> usize {
        (self.end - self.start) * CELL_SIZE as usize
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Location {
    pub point: [Fixed; 2],
}

impl Location {
    pub const fn new(x: i32, z: i32) -> Self {
        Self {
            point: [Fixed::from_int(x), Fixed::from_int(z)],
        }
    }

    pub fn within(self, other: Self, radius: Fixed) -> bool {
        length_squared(offset(self.point, other.point)) <= radius * radius
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum District {
    Outer,
    Locks,
    Storm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Harbor {
    pub district: District,
    pub cores: [Location; CORES],
    pub guards: [Location; GUARDS],
    pub guard_count: u8,
    pub phase: u16,
    pub gate: Location,
    pub(crate) cargo_gate: Option<Location>,
    pub exit: Location,
    pub(crate) docks: [Option<Dock>; MAX_DOCKS],
    pub(crate) bridges: [Option<Bridge>; MAX_BRIDGES],
    pub(crate) layout: Layout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Layout {
    side: u8,
    land: [u32; MAX_SIDE],
    deck: [u32; MAX_SIDE],
    crossings: [u32; MAX_SIDE],
    east_west: [u32; MAX_SIDE],
    obstacles: [u32; MAX_SIDE],
}

impl Layout {
    pub(crate) fn side(&self) -> usize {
        usize::from(self.side)
    }

    pub(crate) fn extent(&self) -> i32 {
        i32::from(self.side) * CELL_SIZE
    }

    pub(crate) fn blocked(&self, x: usize, z: usize) -> bool {
        self.obstacles[z] & (1 << x) != 0
    }

    pub(crate) fn cell(&self, [x, z]: [Fixed; 2]) -> [usize; 2] {
        [x, z]
            .map(|v| (v.to_int().div_euclid(CELL_SIZE)).clamp(0, i32::from(self.side) - 1) as usize)
    }

    pub(crate) fn on_land(&self, x: usize, z: usize) -> bool {
        self.land[z] & (1 << x) != 0
    }

    pub(crate) fn on_bridge(&self, [x, z]: [usize; 2]) -> bool {
        self.crossings[z] & (1 << x) != 0
    }

    fn side_crossing(&self, from: [usize; 2], to: [usize; 2]) -> bool {
        let axes = |[x, z]: [usize; 2]| {
            self.on_bridge([x, z])
                .then_some(usize::from(self.east_west[z] & (1 << x) == 0))
        };
        let a = axes(from);
        let b = axes(to);
        a != b && a.or(b).is_some_and(|axis| from[axis] == to[axis])
    }

    fn passable(&self, x: usize, z: usize) -> bool {
        self.deck[z] & (1 << x) != 0 && !self.blocked(x, z)
    }

    fn open(&self, point: [Fixed; 2]) -> bool {
        let radius = Fixed::from_ratio(5, 4);
        let [left, top] = self.cell(point.map(|v| v - radius));
        let [right, bottom] = self.cell(point.map(|v| v + radius));
        (top..=bottom).all(|z| (left..=right).all(|x| self.passable(x, z)))
    }

    pub(crate) fn center(&self, [x, z]: [usize; 2]) -> [Fixed; 2] {
        [x, z].map(|v| {
            Fixed::from_int(v as i32 * CELL_SIZE + CELL_SIZE / 2)
                .clamp(Fixed::from_int(2), Fixed::from_int(self.extent() - 2))
        })
    }

    pub(crate) fn direct(&self, from: [Fixed; 2], to: [Fixed; 2]) -> bool {
        self.clear_segment(from, to, |point| self.open(point))
    }

    pub(crate) fn rope_clear(&self, from: [Fixed; 2], to: [Fixed; 2]) -> bool {
        self.clear_segment(from, to, |point| {
            let [x, z] = self.cell(point);
            self.passable(x, z)
        })
    }

    fn clear_segment(
        &self,
        from: [Fixed; 2],
        to: [Fixed; 2],
        open: impl Fn([Fixed; 2]) -> bool,
    ) -> bool {
        let delta = offset(to, from);
        let steps = delta[0]
            .abs()
            .max(delta[1].abs())
            .ceil()
            .to_int()
            .div_euclid(2)
            + 1;
        let stride = delta.map(|v| v / steps);
        let mut point = from;
        for _ in 0..steps {
            let next = add(point, stride);
            if !open(next) || self.side_crossing(self.cell(point), self.cell(next)) {
                return false;
            }
            point = next;
        }
        open(to) && !self.side_crossing(self.cell(point), self.cell(to))
    }

    pub(crate) fn way_to(&self, from: [Fixed; 2], to: [Fixed; 2]) -> Option<[Fixed; 2]> {
        const UNSEEN: u16 = u16::MAX;
        let side = self.side();
        let idx = |[x, z]: [usize; 2]| x + z * side;
        let pos = |i: usize| [i % side, i / side];
        let start = idx(self.cell(from));
        let end = idx(self.cell(to));
        let mut first = [UNSEEN; MAX_CELLS];
        let mut queue = [0; MAX_CELLS];
        first[start] = start as u16;
        queue[0] = start as u16;
        let mut head = 0;
        let mut tail = 1;
        while head < tail {
            let current = usize::from(queue[head]);
            head += 1;
            if current == end {
                return Some(self.center(pos(usize::from(first[end]))));
            }
            let [x, z] = pos(current);
            for [nx, nz] in [
                [(x + 1).min(side - 1), z],
                [x, (z + 1).min(side - 1)],
                [x.saturating_sub(1), z],
                [x, z.saturating_sub(1)],
            ] {
                let next = idx([nx, nz]);
                if first[next] != UNSEEN
                    || !self.passable(nx, nz)
                    || self.side_crossing([x, z], [nx, nz])
                {
                    continue;
                }
                first[next] = if current == start {
                    next as u16
                } else {
                    first[current]
                };
                queue[tail] = next as u16;
                tail += 1;
            }
        }
        None
    }
}

impl Harbor {
    pub fn generate(seed: u32, level: u32) -> Self {
        let mut random = seed ^ level.wrapping_mul(0x9e37_79b9);
        let mut next = || {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (random >> 16) as usize
        };
        let district = match level.saturating_sub(1) % 3 {
            0 => District::Outer,
            1 => District::Locks,
            _ => District::Storm,
        };
        let growth = level.saturating_sub(1) as usize;
        let side = (INITIAL_SIDE + growth.min((MAX_SIDE - INITIAL_SIDE) / 2) * 2).min(MAX_SIDE);
        let budget = (1 + growth / 2).min(MAX_DOCKS);
        let edge = side as i32 * CELL_SIZE;
        let mut docks = [None; MAX_DOCKS];
        docks[0] = Some(Dock {
            min: [0; 2],
            end: [side; 2],
        });
        let mut bridges = [None; MAX_BRIDGES];
        let mut count = 1;
        while count < budget {
            let mut cut = None;
            let mut seen = 0;
            // Sample from valid cuts only. Each cut adds one island and one bridge.
            for (slot, &dock) in docks[..count].iter().enumerate() {
                let dock = dock.expect("active dock");
                for axis in 0..2 {
                    let across = 1 - axis;
                    let largest_gap = if budget >= 4 { 4 } else { 3 };
                    for gap in 2..=largest_gap {
                        let first = dock.min[axis] + MIN_DOCK;
                        let last = dock.end[axis].saturating_sub(MIN_DOCK + gap);
                        for start in first..=last {
                            let mut channel = dock;
                            channel.min[axis] = start;
                            channel.end[axis] = start + gap;
                            for lane in dock.min[across] + 2..dock.end[across] - 2 {
                                let bridge = Bridge {
                                    axis,
                                    start: start - 1,
                                    end: start + gap + 1,
                                    lane,
                                };
                                if bridges.into_iter().flatten().any(|old: Bridge| {
                                    let landings = old.footprint();
                                    channel.overlaps(landings)
                                        || bridge.footprint().overlaps(landings)
                                }) {
                                    continue;
                                }
                                seen += 1;
                                if next() % seen == 0 {
                                    cut = Some((slot, channel, bridge));
                                }
                            }
                        }
                    }
                }
            }
            let Some((slot, channel, bridge)) = cut else {
                break;
            };
            let mut a = docks[slot].unwrap();
            let mut b = a;
            a.end[bridge.axis] = channel.min[bridge.axis];
            b.min[bridge.axis] = channel.end[bridge.axis];
            docks[slot] = Some(a);
            docks[count] = Some(b);
            bridges[count - 1] = Some(bridge);
            count += 1;
        }

        let mut layout = Layout {
            side: side as u8,
            land: [0; MAX_SIDE],
            deck: [0; MAX_SIDE],
            crossings: [0; MAX_SIDE],
            east_west: [0; MAX_SIDE],
            obstacles: [0; MAX_SIDE],
        };
        for dock in docks.into_iter().flatten() {
            dock.fill(&mut layout.land);
        }
        if count == 1 {
            // The inner basin has a land route around it and a raised crossing.
            let axis = next() % 2;
            let bridge = Bridge {
                axis,
                start: side / 2 - 3,
                end: side / 2 + 3,
                lane: side / 2,
            };
            let mut basin = bridge.footprint();
            basin.min[axis] += 1;
            basin.end[axis] -= 1;
            basin.min[1 - axis] -= 1;
            basin.end[1 - axis] += 1;
            let mut water = [0; MAX_SIDE];
            basin.fill(&mut water);
            for (land, water) in layout.land.iter_mut().zip(water) {
                *land &= !water;
            }
            bridges[0] = Some(bridge);
        }
        layout.deck = layout.land;
        for bridge in bridges.into_iter().flatten() {
            let deck = bridge.footprint();
            deck.fill(&mut layout.deck);
            deck.fill(&mut layout.crossings);
            if bridge.axis == 0 {
                deck.fill(&mut layout.east_west);
            }
        }

        for dock in docks.into_iter().flatten() {
            let corner = [0, 1].map(|i| if dock.min[i] == 0 { 0 } else { dock.end[i] - 1 });
            if layout.on_land(corner[0], corner[1]) && !layout.on_bridge(corner) {
                layout.obstacles[corner[1]] |= 1 << corner[0];
            }
        }
        // Keep the three outer berths away from every admissible cut and ramp.
        let gate = Location::new(13, edge - 13);
        let exit = Location::new(edge - 13, 13);
        let cargo_gate = (count > 1).then_some(Location::new(edge - 13, edge - 13));
        let mut cores = [SPAWN; CORES];
        for slot in 0..CORES {
            let dock = docks[slot % count].unwrap();
            cores[slot] = find_berth(&layout, dock, next(), |point| {
                [SPAWN, gate, exit]
                    .into_iter()
                    .chain(cargo_gate)
                    .chain(cores[..slot].iter().copied())
                    .all(|other| !point.within(other, Fixed::from_int(8)))
            });
        }
        let guards = core::array::from_fn(|slot| {
            let dock = docks[(slot + 1) % count].unwrap();
            find_berth(&layout, dock, next(), |point| {
                !point.within(SPAWN, Fixed::from_int(14))
            })
        });
        Self {
            district,
            cores,
            guards,
            guard_count: if level == 1 {
                1
            } else if level < 6 {
                2
            } else {
                3
            },
            phase: (next() % 240) as u16,
            gate,
            cargo_gate,
            exit,
            docks,
            bridges,
            layout,
        }
    }

    pub(crate) fn extent(&self) -> i32 {
        self.layout.extent()
    }

    pub(crate) fn recovery_gates(&self) -> impl Iterator<Item = Location> {
        [Some(self.gate), self.cargo_gate].into_iter().flatten()
    }

    pub(crate) fn nearest_gate(&self, cargo: Location) -> Location {
        self.recovery_gates()
            .min_by_key(|gate| length_squared(offset(cargo.point, gate.point)))
            .unwrap_or(self.gate)
    }

    pub(crate) fn recovery_open(&self, gate: Location, tick: u32) -> bool {
        self.district != District::Locks
            || gate != self.gate
            || (tick.wrapping_add(u32::from(self.phase))) % 240 < 156
    }

    pub(crate) fn field(&self) -> Location {
        self.cores[CORES / 2]
    }

    pub fn magnetic_field(&self, position: Location, tick: u32) -> [Fixed; 2] {
        let phase = tick.wrapping_add(u32::from(self.phase)) % 240;
        if self.district == District::Storm
            && phase >= 140
            && position.within(self.field(), Fixed::from_int(10))
        {
            [
                Fixed::from_ratio(if phase < 190 { 1 } else { -1 }, 15),
                Fixed::from_ratio(1, 40),
            ]
        } else {
            [Fixed::ZERO; 2]
        }
    }

    pub(crate) fn height(&self, at: Location) -> Fixed {
        if !self.layout.on_bridge(self.layout.cell(at.point)) {
            return Fixed::ZERO;
        }
        self.bridges
            .into_iter()
            .flatten()
            .find_map(|bridge| bridge.height(at.point))
            .unwrap_or(Fixed::ZERO)
    }

    pub fn advance(&self, from: Location, desired: [Fixed; 2]) -> Location {
        let [mut x, mut z] =
            desired.map(|v| v.clamp(Fixed::from_int(2), Fixed::from_int(self.extent() - 2)));
        let can_move = |from, to| {
            self.layout.open(to)
                && !self
                    .layout
                    .side_crossing(self.layout.cell(from), self.layout.cell(to))
        };
        if !can_move(from.point, [x, from.point[1]]) {
            x = from.point[0];
        }
        if !can_move([x, from.point[1]], [x, z]) {
            z = from.point[1];
        }
        Location { point: [x, z] }
    }
}

fn find_berth(
    layout: &Layout,
    dock: Dock,
    seed: usize,
    clear: impl Fn(Location) -> bool,
) -> Location {
    let width = dock.end[0] - dock.min[0] - 2;
    let height = dock.end[1] - dock.min[1] - 2;
    for i in 0..width * height {
        let index = (i + seed) % (width * height);
        let cell = [
            dock.min[0] + 1 + index % width,
            dock.min[1] + 1 + index / width,
        ];
        let at = Location {
            point: layout.center(cell),
        };
        if layout.passable(cell[0], cell[1]) && !layout.on_bridge(cell) && clear(at) {
            return at;
        }
    }
    // Include shoreline berths if the interior is reserved for ramps and gates.
    for z in dock.min[1]..dock.end[1] {
        for x in dock.min[0]..dock.end[0] {
            let cell = [x, z];
            let at = Location {
                point: layout.center(cell),
            };
            if layout.passable(x, z) && !layout.on_bridge(cell) && clear(at) {
                return at;
            }
        }
    }
    unreachable!("generated dock has no room for its reserved berths")
}

pub fn offset(a: [Fixed; 2], b: [Fixed; 2]) -> [Fixed; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn add(a: [Fixed; 2], b: [Fixed; 2]) -> [Fixed; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn scaled(a: [Fixed; 2], scale: Fixed) -> [Fixed; 2] {
    a.map(|v| v * scale)
}

pub fn dot(a: [Fixed; 2], b: [Fixed; 2]) -> Fixed {
    a[0] * b[0] + a[1] * b[1]
}

pub fn length_squared(v: [Fixed; 2]) -> Fixed {
    dot(v, v)
}

pub fn direction(v: [Fixed; 2]) -> [Fixed; 2] {
    let length = length_squared(v).sqrt();
    if length.is_zero() {
        [Fixed::ZERO; 2]
    } else {
        v.map(|p| p / length)
    }
}
