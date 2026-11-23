use mirui::types::Fixed;

pub const CORES: usize = 5;
pub const GUARDS: usize = 3;
pub(crate) const GRID: usize = 12;
pub(crate) const CELL_SIZE: i32 = 6;
pub const EDGE: i32 = 72;
pub const SPAWN: Location = Location::new(13, 13, false);
pub const WEST_GATE: Location = Location::new(13, 62, false);
pub const EAST_GATE: Location = Location::new(59, 62, false);
pub const EXIT: Location = Location::new(58, 13, false);
pub(crate) const BRIDGE_START: i32 = 13;
pub(crate) const BRIDGE_END: i32 = 64;

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

    pub fn height(self) -> Fixed {
        let z = self.point[1];
        if self.upper {
            (z - Fixed::from_int(BRIDGE_START))
                .min(Fixed::from_int(BRIDGE_END) - z)
                .clamp(Fixed::ZERO, Fixed::from_int(8))
        } else {
            Fixed::ZERO
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
    pub(crate) bridge: i32,
    pub(crate) layout: Layout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Layout([u16; GRID]);

impl Layout {
    fn generate(next: &mut impl FnMut() -> i32) -> Self {
        let mut rows = [0; GRID];
        // Two open columns separate the warehouse bays even when both shift inward.
        for (front, left) in [(4, 2), (4, 7), (8, 2), (8, 7)] {
            let row = front + usize::from(next() & 1 != 0);
            let column = left + usize::from(next() & 1 != 0);
            for rz in &mut rows[row..row + 2.min(GRID - 2 - row)] {
                *rz |= 3 << column;
            }
        }
        Self(rows)
    }

    pub(crate) fn blocked(&self, x: usize, z: usize) -> bool {
        self.0[z] & (1 << x) != 0
    }

    pub(crate) fn cell([x, z]: [Fixed; 2]) -> [usize; 2] {
        [x, z].map(|v| (v.to_int().div_euclid(CELL_SIZE)).clamp(0, GRID as i32 - 1) as usize)
    }

    fn open(&self, point: [Fixed; 2]) -> bool {
        let radius = Fixed::from_ratio(5, 4);
        let [left, top] = Self::cell(point.map(|v| v - radius));
        let [right, bottom] = Self::cell(point.map(|v| v + radius));
        (top..=bottom).all(|z| (left..=right).all(|x| !self.blocked(x, z)))
    }

    fn nearby_open(&self, location: Location) -> Location {
        let [x, z] = Self::cell(location.point);
        for radius in 0..GRID {
            for row in z.saturating_sub(radius)..=(z + radius).min(GRID - 1) {
                for column in x.saturating_sub(radius)..=(x + radius).min(GRID - 1) {
                    let point = Self::center([column, row]);
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

    pub(crate) fn center([x, z]: [usize; 2]) -> [Fixed; 2] {
        [x, z].map(|v| {
            Fixed::from_int(v as i32 * CELL_SIZE + CELL_SIZE / 2)
                .clamp(Fixed::from_int(5), Fixed::from_int(EDGE - 5))
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
        const UNSEEN: u8 = u8::MAX;
        let idx = |[x, z]: [usize; 2]| x + z * GRID;
        let pos = |i: usize| [i % GRID, i / GRID];
        let start = idx(Self::cell(from));
        let end = idx(Self::cell(to));
        let mut first = [UNSEEN; GRID * GRID];
        let mut queue = [0; GRID * GRID];
        first[start] = start as u8;
        queue[0] = start as u8;
        let mut head = 0;
        let mut tail = 1;
        while head < tail {
            let current = usize::from(queue[head]);
            head += 1;
            if current == end {
                return Some(Self::center(pos(usize::from(first[end]))));
            }
            let [x, z] = pos(current);
            for [nx, nz] in [
                [(x + 1).min(GRID - 1), z],
                [x, (z + 1).min(GRID - 1)],
                [x.saturating_sub(1), z],
                [x, z.saturating_sub(1)],
            ] {
                let next = idx([nx, nz]);
                if first[next] != UNSEEN || self.blocked(nx, nz) {
                    continue;
                }
                first[next] = if current == start {
                    next as u8
                } else {
                    first[current]
                };
                queue[tail] = next as u8;
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
        let bridge = [24, 36, 48][next() as usize % 3];
        let layout = Layout::generate(&mut next);
        Self {
            district,
            cores: [
                Location::new(8 + next() % 4, 24 + next() % 12, false),
                Location::new(61 + next() % 5, 28 + next() % 10, false),
                Location::new(32 + next() % 8, 43 + next() % 9, false),
                Location::new(bridge - 2 + next() % 5, 33 + next() % 12, true),
                Location::new(8 + next() % 4, 48 + next() % 10, false),
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
                Location::new(bridge, 54, true),
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
            gate: if west { WEST_GATE } else { EAST_GATE },
            bridge,
            layout,
        }
    }

    pub(crate) fn high_gate(&self) -> Location {
        Location::new(self.bridge, 54, true)
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
            desired.map(|v| v.clamp(Fixed::from_int(5), Fixed::from_int(EDGE - 5)));
        let left = Fixed::from_int(self.bridge - 7);
        let right = Fixed::from_int(self.bridge + 7);
        let south = Fixed::from_int(BRIDGE_START);
        let north = Fixed::from_int(BRIDGE_END);
        let mut upper = from.upper;
        if from.upper {
            x = x.clamp(left, right);
            if z < south || z > north {
                upper = false;
            } else if self.lock_closed(tick) {
                let lock = Fixed::from_int(40);
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
            let crossed_end =
                (from.point[1] < south && z >= south) || (from.point[1] > north && z <= north);
            upper = crossed_end && (x - Fixed::from_int(self.bridge)).abs() <= Fixed::from_int(3);
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
