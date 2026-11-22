use mirui::types::Fixed;

pub const CORES: usize = 5;
pub const GUARDS: usize = 3;
pub const EDGE: i32 = 72;
pub const SPAWN: Location = Location::new(13, 13, false);
pub const RAMP: Location = Location::new(36, 13, false);
pub const WEST_GATE: Location = Location::new(13, 60, false);
pub const EAST_GATE: Location = Location::new(59, 60, false);
pub const HIGH_GATE: Location = Location::new(36, 56, true);
pub const EXIT: Location = Location::new(58, 13, false);

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
            (z - Fixed::from_int(13)).clamp(Fixed::ZERO, Fixed::from_int(8))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Harbor {
    pub district: District,
    pub cores: [Location; CORES],
    pub guards: [Location; GUARDS],
    pub quota: u8,
    pub guard_count: u8,
    pub phase: u16,
    pub gate: Location,
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
        Self {
            district,
            cores: [
                Location::new(16 + next() % 8, 24 + next() % 5, false),
                Location::new(46 + next() % 10, 28 + next() % 8, false),
                Location::new(27 + next() % 9, 45 + next() % 8, false),
                Location::new(34 + next() % 5, 33 + next() % 12, true),
                Location::new(17 + next() % 6, 50 + next() % 7, false),
            ],
            guards: [
                Location::new(60, 34 + next() % 6, false),
                Location::new(24, 35 + next() % 5, false),
                Location::new(36, 54, true),
            ],
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
        }
    }

    pub fn lock_closed(self, tick: u32) -> bool {
        self.district == District::Locks && (tick.wrapping_add(u32::from(self.phase))) % 240 >= 156
    }

    pub fn lock_warning(self, tick: u32) -> bool {
        self.district == District::Locks
            && (128..156).contains(&(tick.wrapping_add(u32::from(self.phase)) % 240))
    }

    pub fn magnetic_field(self, position: Location, tick: u32) -> [Fixed; 2] {
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

    pub fn advance(self, from: Location, desired: [Fixed; 2], tick: u32) -> Location {
        let [mut x, mut z] =
            desired.map(|v| v.clamp(Fixed::from_int(5), Fixed::from_int(EDGE - 5)));
        let left = Fixed::from_int(29);
        let right = Fixed::from_int(43);
        let entry = RAMP.point[1];
        let mut upper = from.upper;
        if from.upper {
            x = x.clamp(left, right);
            z = z.min(Fixed::from_int(64));
            if z < entry {
                upper = false;
            } else if self.lock_closed(tick) {
                let lock = Fixed::from_int(40);
                if (from.point[1] < lock) != (z < lock) {
                    z = from.point[1];
                }
            }
        } else if from.point[1] < entry && z >= entry && (left..=right).contains(&x) {
            upper = true;
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
