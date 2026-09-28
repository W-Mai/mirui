use super::types::{BOARD_HEIGHT, BOARD_WIDTH, CELL_COUNT, ClockGate, EchoRoom};

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.0 = value;
        value
    }

    fn index(&mut self, len: u32) -> usize {
        ((u64::from(self.next()) * u64::from(len)) >> 32) as usize
    }

    fn chance_below_half(&mut self) -> bool {
        self.next() < (1 << 31)
    }
}
pub(crate) fn generate_room(seed: u32, level: u8) -> EchoRoom {
    let mut rng = Rng::new(
        seed.max(1)
            .wrapping_add(u32::from(level + 1).wrapping_mul(2_246_822_519)),
    );
    let gate_count = if level < 2 {
        1
    } else if level < 6 {
        2
    } else {
        3
    };
    let width = 3 * gate_count + 3;
    let mut room = EchoRoom::EMPTY;
    for y in 1..6 {
        for x in 1..width - 1 {
            room.walls[y * BOARD_WIDTH + x] = false;
        }
    }
    room.start = (3 * BOARD_WIDTH + 1) as u8;
    room.end = (3 * BOARD_WIDTH + width - 2) as u8;
    room.gate_count = gate_count as u8;
    for gate in 0..gate_count {
        let x = 3 * (gate + 1);
        let door_y = 1 + rng.index(5);
        for y in 1..6 {
            room.walls[y * BOARD_WIDTH + x] = true;
        }
        let door = door_y * BOARD_WIDTH + x;
        room.walls[door] = false;
        room.doors[gate] = door as u8;
        let mut choices = [0usize; 3];
        let mut choice_len = 0;
        for y in 1usize..6 {
            if y.abs_diff(door_y) >= 2 {
                choices[choice_len] = y;
                choice_len += 1;
            }
        }
        let plate_y = choices[rng.index(choice_len as u32)];
        room.plates[gate] = (plate_y * BOARD_WIDTH + x - 1) as u8;
    }
    room.gem_count = (gate_count + 1) as u8;
    for partition in 0..=gate_count {
        let x = 3 * partition + 1;
        let y = if partition % 2 == 0 { 1 } else { 5 };
        room.gems[partition] = (y * BOARD_WIDTH + x) as u8;
    }
    if level >= 3 {
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + 4) as u8;
        if room.plate_at(cell).is_none() && room.gem_at(cell).is_none() {
            room.clocks[usize::from(room.clock_count)] = ClockGate {
                cell,
                phase: rng.index(4) as u8,
            };
            room.clock_count += 1;
        }
    }
    if level >= 8 {
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + 7) as u8;
        if room.plate_at(cell).is_none() && room.gem_at(cell).is_none() {
            room.clocks[usize::from(room.clock_count)] = ClockGate {
                cell,
                phase: rng.index(4) as u8,
            };
            room.clock_count += 1;
        }
    }
    for partition in 0..=gate_count {
        let x = 3 * partition + 2;
        if x >= width - 1 {
            continue;
        }
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + x) as u8;
        let near_door = room.doors[..gate_count]
            .iter()
            .any(|door| cell == *door || cell + 1 == *door || cell == door.saturating_add(1));
        let protected = cell == room.start
            || cell == room.end
            || room.plate_at(cell).is_some()
            || room.gem_at(cell).is_some()
            || near_door
            || room.clock_at(cell).is_some();
        if !protected && rng.chance_below_half() {
            room.walls[usize::from(cell)] = true;
        }
    }
    let flip_x = rng.chance_below_half();
    let flip_y = rng.chance_below_half();
    if flip_x || flip_y {
        room = mirror_room(room, flip_x, flip_y);
    }
    room
}

fn mirror_room(room: EchoRoom, flip_x: bool, flip_y: bool) -> EchoRoom {
    let map = |cell: u8| {
        let x = usize::from(cell) % BOARD_WIDTH;
        let y = usize::from(cell) / BOARD_WIDTH;
        let mapped_x = if flip_x { BOARD_WIDTH - 1 - x } else { x };
        let mapped_y = if flip_y { BOARD_HEIGHT - 1 - y } else { y };
        (mapped_y * BOARD_WIDTH + mapped_x) as u8
    };
    let mut mirrored = EchoRoom::EMPTY;
    for cell in 0..CELL_COUNT {
        mirrored.walls[usize::from(map(cell as u8))] = room.walls[cell];
    }
    mirrored.start = map(room.start);
    mirrored.end = map(room.end);
    mirrored.gate_count = room.gate_count;
    mirrored.gem_count = room.gem_count;
    mirrored.clock_count = room.clock_count;
    for index in 0..usize::from(room.gate_count) {
        mirrored.doors[index] = map(room.doors[index]);
        mirrored.plates[index] = map(room.plates[index]);
    }
    for index in 0..usize::from(room.gem_count) {
        mirrored.gems[index] = map(room.gems[index]);
    }
    for index in 0..usize::from(room.clock_count) {
        mirrored.clocks[index] = ClockGate {
            cell: map(room.clocks[index].cell),
            phase: room.clocks[index].phase,
        };
    }
    mirrored
}
