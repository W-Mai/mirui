use mirui::types::Fixed;

pub const CORES: usize = 5;
pub const GUARDS: usize = 3;
const INITIAL_SIDE: usize = 12;
const MAX_SIDE: usize = 18;
const MAX_CELLS: usize = MAX_SIDE * MAX_SIDE;
pub(crate) const CELL_SIZE: i32 = 6;
pub(crate) const INITIAL_EXTENT: i32 = INITIAL_SIDE as i32 * CELL_SIZE;
pub const SPAWN: Location = Location::new(13, 13, false);
pub(crate) const BRIDGE_START: i32 = 13;
const INITIAL_BRIDGE_END: i32 = 64;
pub(crate) const LOCK_POSITION: i32 = 40;
const RAMP_ENTRY: Fixed = Fixed::from_int(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bridge {
    pub x: i32,
    pub end: i32,
}

impl Bridge {
    pub fn length(self) -> usize {
        (self.end - BRIDGE_START + 1) as usize
    }

    fn height(self, z: Fixed) -> Fixed {
        (z - Fixed::from_int(BRIDGE_START))
            .min(Fixed::from_int(self.end) - z)
            .clamp(Fixed::ZERO, Fixed::from_int(8))
    }

    pub fn gate(self) -> Location {
        Location::new(self.x, self.end - 10, true)
    }

    pub fn ramp(self, front: bool) -> Location {
        Location::new(
            self.x,
            if front {
                BRIDGE_START - 5
            } else {
                self.end + 3
            },
            false,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Location {
    pub point: [Fixed; 2],
    pub upper: bool,
}

impl Location {
    pub const fn new(x: i32, z: i32, upper: bool) -> Self {
        Self {
            point: [Fixed::from_int(x), Fixed::from_int(z)],
            upper,
        }
    }

    pub fn within(self, other: Self, radius: Fixed) -> bool {
        self.upper == other.upper
            && length_squared(offset(self.point, other.point)) <= radius * radius
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
    pub quota: u8,
    pub guard_count: u8,
    pub phase: u16,
    pub gate: Location,
    pub(crate) cargo_gate: Option<Location>,
    pub exit: Location,
    pub(crate) bridge: Bridge,
    secondary_bridge: Option<Bridge>,
    pub(crate) layout: Layout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Layout {
    side: u8,
    rows: [u32; MAX_SIDE],
}

impl Layout {
    fn generate(side: usize, west: bool, next: &mut impl FnMut() -> i32) -> Self {
        let mut rows = [0; MAX_SIDE];
        // Two open columns separate the warehouse bays even when both shift inward.
        for (front, left) in [(4, 2), (4, side - 5), (8, 2), (8, side - 5)] {
            let row = front + usize::from(next() & 1 != 0);
            let column = left + usize::from(next() & 1 != 0);
            for rz in &mut rows[row..row + 2.min(side - 2 - row)] {
                *rz |= 3 << column;
            }
        }
        if let Some(start) = Self::freight_start(side) {
            // Freight sheds leave a central alley and wider routes around both quays.
            // The larger docks have sheds on both sides of the alley.
            let west_shed = 4;
            let east_shed = side - 6;
            let sheds = if side >= INITIAL_SIDE + 4 {
                (3 << west_shed) | (3 << east_shed)
            } else {
                3 << if west { west_shed } else { east_shed }
            };
            for row in &mut rows[start..start + 2] {
                *row |= sheds;
            }
        }
        Self {
            side: side as u8,
            rows,
        }
    }

    fn clear_ramp(&mut self, bridge: Bridge) {
        let x = bridge.x as usize / CELL_SIZE as usize;
        let z = bridge.end as usize / CELL_SIZE as usize;
        let last = (z + 1).min(self.side() - 1);
        for row in &mut self.rows[z.saturating_sub(1)..=last] {
            *row &= !(7 << (x - 1));
        }
    }

    fn freight_start(side: usize) -> Option<usize> {
        (side > INITIAL_SIDE).then_some(side - 4)
    }

    pub(crate) fn freight_row(&self) -> Option<usize> {
        Self::freight_start(self.side())
    }

    pub(crate) fn side(&self) -> usize {
        usize::from(self.side)
    }

    pub(crate) fn extent(&self) -> i32 {
        i32::from(self.side) * CELL_SIZE
    }

    pub(crate) fn blocked(&self, x: usize, z: usize) -> bool {
        self.rows[z] & (1 << x) != 0
    }

    pub(crate) fn cell(&self, [x, z]: [Fixed; 2]) -> [usize; 2] {
        [x, z]
            .map(|v| (v.to_int().div_euclid(CELL_SIZE)).clamp(0, i32::from(self.side) - 1) as usize)
    }

    fn open(&self, point: [Fixed; 2]) -> bool {
        let radius = Fixed::from_ratio(5, 4);
        let [left, top] = self.cell(point.map(|v| v - radius));
        let [right, bottom] = self.cell(point.map(|v| v + radius));
        (top..=bottom).all(|z| (left..=right).all(|x| !self.blocked(x, z)))
    }

    fn nearby_open(&self, location: Location) -> Location {
        let [x, z] = self.cell(location.point);
        let side = self.side();
        for radius in 0..side {
            for row in z.saturating_sub(radius)..=(z + radius).min(side - 1) {
                for column in x.saturating_sub(radius)..=(x + radius).min(side - 1) {
                    let point = self.center([column, row]);
                    if self.open(point) {
                        return Location {
                            point,
                            upper: false,
                        };
                    }
                }
            }
        }
        SPAWN
    }

    pub(crate) fn center(&self, [x, z]: [usize; 2]) -> [Fixed; 2] {
        [x, z].map(|v| {
            Fixed::from_int(v as i32 * CELL_SIZE + CELL_SIZE / 2)
                .clamp(Fixed::from_int(5), Fixed::from_int(self.extent() - 5))
        })
    }

    pub(crate) fn direct(&self, from: [Fixed; 2], to: [Fixed; 2]) -> bool {
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
            point = add(point, stride);
            if !self.open(point) {
                return false;
            }
        }
        self.open(to)
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
                if first[next] != UNSEEN || self.blocked(nx, nz) {
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
        let district = match level.saturating_sub(1) % 3 {
            0 => District::Outer,
            1 => District::Locks,
            _ => District::Storm,
        };
        let mut next = || {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (random >> 16) as i32
        };
        let west = next() & 1 == 0;
        let growth = level.saturating_sub(1).min(3) as usize * 2;
        let side = INITIAL_SIDE + growth;
        let edge = side as i32 * CELL_SIZE;
        let added = edge - INITIAL_EXTENT;
        let bridge = Bridge {
            x: edge / 2 + [-12, 0, 12][next() as usize % 3],
            end: INITIAL_BRIDGE_END + added,
        };
        let secondary_bridge = (level >= 3).then_some(Bridge {
            x: if bridge.x <= edge / 2 { edge - 18 } else { 18 },
            end: bridge.end - 12,
        });
        let mut layout = Layout::generate(side, west, &mut next);
        layout.clear_ramp(bridge);
        if let Some(second) = secondary_bridge {
            layout.clear_ramp(second);
        }
        let west_core = Location::new(8 + next() % 4, 24 + next() % 12, false);
        let east_core = Location::new(61 + added + next() % 5, 28 + next() % 10, false);
        let cross_bay = 32 + added / 2 + next() % 8;
        let inland = 43 + next() % 9;
        let cargo = Location::new(cross_bay, if added == 0 { inland } else { edge - 9 }, false);
        Self {
            district,
            cores: [
                west_core,
                secondary_bridge.map_or(east_core, |second| {
                    Location::new(second.x, second.end - 22, true)
                }),
                cargo,
                Location::new(
                    bridge.x - 2 + next() % 5,
                    bridge.end - 31 + next() % 12,
                    true,
                ),
                Location::new(8 + next() % 4, 48 + added + next() % 10, false),
            ]
            .map(|location| {
                if location.upper {
                    location
                } else {
                    layout.nearby_open(location)
                }
            }),
            guards: [
                Location::new(60, 34 + next() % 6, false),
                Location::new(24, 35 + next() % 5, false),
                bridge.gate(),
            ]
            .map(|location| {
                if location.upper {
                    location
                } else {
                    layout.nearby_open(location)
                }
            }),
            quota: if level == 1 { 2 } else { 3 },
            guard_count: if level == 1 {
                1
            } else if level < 6 {
                2
            } else {
                3
            },
            phase: (next() % 240) as u16,
            gate: Location::new(if west { 13 } else { edge - 13 }, edge - 10, false),
            cargo_gate: (added != 0)
                .then(|| Location::new(if west { edge - 13 } else { 13 }, edge - 10, false)),
            exit: Location::new(edge - 14, 13, false),
            bridge,
            secondary_bridge,
            layout,
        }
    }

    pub(crate) fn extent(&self) -> i32 {
        self.layout.extent()
    }

    pub(crate) fn bridges(&self) -> [Option<Bridge>; 2] {
        [Some(self.bridge), self.secondary_bridge]
    }

    pub(crate) fn bridge_at(&self, x: Fixed) -> Bridge {
        self.secondary_bridge
            .filter(|second| {
                (x - Fixed::from_int(second.x)).abs() < (x - Fixed::from_int(self.bridge.x)).abs()
            })
            .unwrap_or(self.bridge)
    }

    pub(crate) fn height(&self, location: Location) -> Fixed {
        if location.upper {
            self.bridge_at(location.point[0]).height(location.point[1])
        } else {
            Fixed::ZERO
        }
    }

    pub(crate) fn recovery_gates(&self, upper: bool) -> [Option<Location>; 2] {
        if upper {
            self.bridges().map(|bridge| bridge.map(Bridge::gate))
        } else {
            [Some(self.gate), self.cargo_gate]
        }
    }

    pub(crate) fn nearest_gate(&self, cargo: Location) -> Location {
        if cargo.upper {
            return self.bridge_at(cargo.point[0]).gate();
        }
        let distance = |gate: Location| length_squared(offset(cargo.point, gate.point));
        self.cargo_gate
            .filter(|&gate| distance(gate) < distance(self.gate))
            .unwrap_or(self.gate)
    }

    pub fn lock_closed(&self, tick: u32) -> bool {
        self.district == District::Locks && (tick.wrapping_add(u32::from(self.phase))) % 240 >= 156
    }

    pub fn lock_warning(&self, tick: u32) -> bool {
        self.district == District::Locks
            && (128..156).contains(&(tick.wrapping_add(u32::from(self.phase)) % 240))
    }

    pub fn magnetic_field(&self, position: Location, tick: u32) -> [Fixed; 2] {
        let phase = tick.wrapping_add(u32::from(self.phase)) % 240;
        if self.district == District::Storm
            && !position.upper
            && phase >= 140
            && position.within(Location::new(38, 43, false), Fixed::from_int(10))
        {
            [
                Fixed::from_ratio(if phase < 190 { 1 } else { -1 }, 15),
                Fixed::from_ratio(1, 40),
            ]
        } else {
            [Fixed::ZERO; 2]
        }
    }

    pub fn advance(&self, from: Location, desired: [Fixed; 2], tick: u32) -> Location {
        let [mut x, mut z] =
            desired.map(|v| v.clamp(Fixed::from_int(5), Fixed::from_int(self.extent() - 5)));
        let south = Fixed::from_int(BRIDGE_START);
        let mut upper = from.upper;
        if from.upper {
            let bridge = self.bridge_at(from.point[0]);
            x = x.clamp(Fixed::from_int(bridge.x - 7), Fixed::from_int(bridge.x + 7));
            if z < south || z > Fixed::from_int(bridge.end) {
                upper = false;
            } else if bridge == self.bridge && self.lock_closed(tick) {
                let lock = Fixed::from_int(LOCK_POSITION);
                if (from.point[1] < lock) != (z < lock) {
                    z = from.point[1];
                }
            }
        } else {
            if !self.layout.open([x, from.point[1]]) {
                x = from.point[0];
            }
            if !self.layout.open([x, z]) {
                z = from.point[1];
            }
            let bridge = self.bridge_at(x);
            let north = Fixed::from_int(bridge.end);
            let crossed_end =
                (from.point[1] < south && z >= south) || (from.point[1] > north && z <= north);
            upper = crossed_end && (x - Fixed::from_int(bridge.x)).abs() <= RAMP_ENTRY;
        }
        Location {
            point: [x, z],
            upper,
        }
    }
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
