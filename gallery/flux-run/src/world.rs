use mirui::types::Fixed;

pub const CORES: usize = 5;
pub const GUARDS: usize = 3;
const INITIAL_SIDE: usize = 12;
const MAX_SIDE: usize = 18;
const MAX_CELLS: usize = MAX_SIDE * MAX_SIDE;
pub(crate) const CELL_SIZE: i32 = 6;
pub(crate) const INITIAL_EXTENT: i32 = INITIAL_SIDE as i32 * CELL_SIZE;
pub const SPAWN: Location = Location::new(13, 13);
pub(crate) const MAX_DOCKS: usize = 4;
pub(crate) const MAX_BRIDGES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dock {
    pub min: [usize; 2],
    pub end: [usize; 2],
}

impl Dock {
    fn corner([far_x, far_z]: [bool; 2], near: usize, far: usize, side: usize) -> Self {
        let span = |distant| if distant { (far, side) } else { (0, near) };
        let (left, right) = span(far_x);
        let (top, bottom) = span(far_z);
        Self {
            min: [left, top],
            end: [right, bottom],
        }
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
        let mut end = [self.lane + 1; 2];
        min[self.axis] = self.start;
        end[self.axis] = self.end;
        Dock { min, end }
    }

    pub fn center(self) -> Location {
        self.footprint().center()
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
    deck: [u32; MAX_SIDE],
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

    pub(crate) fn on_deck(&self, x: usize, z: usize) -> bool {
        self.deck[z] & (1 << x) != 0
    }

    fn passable(&self, x: usize, z: usize) -> bool {
        self.on_deck(x, z) && !self.blocked(x, z)
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
            point = add(point, stride);
            if !open(point) {
                return false;
            }
        }
        open(to)
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
                if first[next] != UNSEEN || !self.passable(nx, nz) {
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
        let side = INITIAL_SIDE + level.saturating_sub(1).min(3) as usize * 2;
        let channel = 2 + usize::from(level >= 3 && next() & 1 == 0);
        let near = (side - channel) / 2;
        let far = near + channel;
        let edge = side as i32 * CELL_SIZE;
        let expanded = level > 1;
        let west = Dock::corner([false, false], near, far, side);
        let east = Dock::corner([true, false], near, far, side);
        let freight = Dock::corner([false, true], near, far, side);
        let refinery = Dock::corner([true, true], near, far, side);
        let docks = [
            Some(west),
            Some(east),
            Some(freight),
            expanded.then_some(refinery),
        ];

        // Choose a connected graph of existing quays. A missing link changes
        // which islands must be crossed, including the route to the exit.
        let variant = next() % 5;
        let bridges = [0, 1, 2, 3].map(|link| {
            let axis = link % 2;
            let western = link < 2;
            let lane = if western {
                near / 2 + next() % 2
            } else {
                (far + side) / 2 + next() % 2
            };
            (if expanded {
                district == District::Locks || variant != link
            } else {
                western
            })
            .then_some(Bridge {
                axis,
                start: near,
                end: far,
                lane,
            })
        });
        let mut layout = Layout {
            side: side as u8,
            deck: [0; MAX_SIDE],
            obstacles: [0; MAX_SIDE],
        };
        for dock in docks.into_iter().flatten() {
            dock.fill(&mut layout.deck);
        }
        for bridge in bridges.into_iter().flatten() {
            bridge.footprint().fill(&mut layout.deck);
        }

        // Put sheds in outer corners outside the central berths and approaches.
        // The two cells on either side of a bridge landing remain available.
        for dock in docks.into_iter().flatten() {
            let [x, z] = dock.min;
            let [right, bottom] = dock.end;
            let x = if x == 0 { x } else { right - 1 };
            let z = if z == 0 { z } else { bottom - 1 };
            layout.obstacles[z] |= 1 << x;

            // Leave a recessed slip at one of the non-landing corners.
            // The opposite outer corner carries the machinery.
            if next() & 1 == 0 {
                let inner_x = if x == 0 { right - 1 } else { dock.min[0] };
                layout.deck[z] &= !(1 << inner_x);
            } else {
                let inner_z = if z == 0 { bottom - 1 } else { dock.min[1] };
                layout.deck[inner_z] &= !(1 << x);
            }
        }
        let gate = Location::new(13, edge - 13);
        let exit = Location::new(edge - 13, 13);
        let spot = |center: Location, x: i32, z: i32| Location {
            point: add(center.point, [Fixed::from_int(x), Fixed::from_int(z)]),
        };
        let bridge = bridges
            .into_iter()
            .flatten()
            .next()
            .expect("connected harbor");
        let cores = [
            spot(west.center(), 7, -7),
            spot(east.center(), -7, 7),
            spot(freight.center(), 7, -7),
            if expanded {
                spot(refinery.center(), -7, -7)
            } else {
                spot(freight.center(), -7, 7)
            },
            bridge.center(),
        ];
        Self {
            district,
            cores,
            guards: [
                spot(east.center(), 0, 9),
                spot(freight.center(), 7, 5),
                spot(
                    if expanded {
                        refinery.center()
                    } else {
                        west.center()
                    },
                    -7,
                    8,
                ),
            ],
            guard_count: if level == 1 {
                1
            } else if level < 6 {
                2
            } else {
                3
            },
            phase: (next() % 240) as u16,
            gate,
            cargo_gate: expanded.then_some(Location::new(edge - 13, edge - 13)),
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
        self.docks[2].expect("freight dock").center()
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

    pub fn advance(&self, from: Location, desired: [Fixed; 2]) -> Location {
        let [mut x, mut z] =
            desired.map(|v| v.clamp(Fixed::from_int(2), Fixed::from_int(self.extent() - 2)));
        if !self.layout.open([x, from.point[1]]) {
            x = from.point[0];
        }
        if !self.layout.open([x, z]) {
            z = from.point[1];
        }
        Location { point: [x, z] }
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
