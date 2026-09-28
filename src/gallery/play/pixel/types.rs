pub(crate) const GRID_WIDTH: u8 = 12;
pub(crate) const GRID_HEIGHT: u8 = 12;
pub(crate) const FRAME_COUNT: u8 = 4;
pub(crate) const COLOR_COUNT: u8 = 6;
const PIXELS_PER_FRAME: usize = GRID_WIDTH as usize * GRID_HEIGHT as usize;
const PACKED_BYTES: usize = PIXELS_PER_FRAME * FRAME_COUNT as usize / 2;
const HISTORY_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PixelFrames {
    bytes: [u8; PACKED_BYTES],
}

impl PixelFrames {
    pub(super) const EMPTY: Self = Self {
        bytes: [0; PACKED_BYTES],
    };

    fn index(frame: u8, x: u8, y: u8) -> Option<usize> {
        if frame >= FRAME_COUNT || x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return None;
        }
        Some(
            usize::from(frame) * PIXELS_PER_FRAME
                + usize::from(y) * usize::from(GRID_WIDTH)
                + usize::from(x),
        )
    }

    pub(crate) fn get(&self, frame: u8, x: u8, y: u8) -> u8 {
        let Some(index) = Self::index(frame, x, y) else {
            return 0;
        };
        let packed = self.bytes[index / 2];
        if index & 1 == 0 {
            packed & 0x0f
        } else {
            packed >> 4
        }
    }

    pub(super) fn set(&mut self, frame: u8, x: u8, y: u8, color: u8) -> bool {
        let Some(index) = Self::index(frame, x, y) else {
            return false;
        };
        let color = color.min(COLOR_COUNT);
        let byte = &mut self.bytes[index / 2];
        let previous = *byte;
        if index & 1 == 0 {
            *byte = (*byte & 0xf0) | color;
        } else {
            *byte = (*byte & 0x0f) | (color << 4);
        }
        *byte != previous
    }

    pub(super) fn clear_frame(&mut self, frame: u8) {
        let start = usize::from(frame) * PIXELS_PER_FRAME / 2;
        self.bytes[start..start + PIXELS_PER_FRAME / 2].fill(0);
    }

    pub(super) fn copy_frame(&mut self, destination: u8, source: u8) {
        const FRAME_BYTES: usize = PIXELS_PER_FRAME / 2;
        let source_start = usize::from(source) * PIXELS_PER_FRAME / 2;
        let destination_start = usize::from(destination) * PIXELS_PER_FRAME / 2;
        let mut copy = [0_u8; FRAME_BYTES];
        copy.copy_from_slice(&self.bytes[source_start..source_start + FRAME_BYTES]);
        self.bytes[destination_start..destination_start + FRAME_BYTES].copy_from_slice(&copy);
    }

    pub(crate) fn template(kind: u8) -> Option<Self> {
        if kind > 2 {
            return None;
        }
        let mut frames = Self::EMPTY;
        for frame in 0..FRAME_COUNT {
            let set = |frames: &mut Self, x: i8, y: i8, color| {
                if (0..GRID_WIDTH as i8).contains(&x) && (0..GRID_HEIGHT as i8).contains(&y) {
                    frames.set(frame, x as u8, y as u8, color);
                }
            };
            match kind {
                0 => {
                    for y in 3..=7 {
                        for x in 3..=8 {
                            set(&mut frames, x, y, 1);
                        }
                    }
                    for x in 4..8 {
                        set(&mut frames, x, 2, 1);
                    }
                    set(&mut frames, 5, 1, 3);
                    set(&mut frames, 6, 1, 3);
                    for y in 4..=5 {
                        set(&mut frames, 4, y, if frame == 2 { 1 } else { 6 });
                        set(&mut frames, 7, y, if frame == 2 { 1 } else { 6 });
                    }
                    if frame == 2 {
                        set(&mut frames, 4, 5, 6);
                        set(&mut frames, 7, 5, 6);
                    }
                    set(&mut frames, 5, 7, 4);
                    set(&mut frames, 6, 7, 4);
                    set(&mut frames, 2, 5, 1);
                    set(&mut frames, 2, 6, 1);
                    set(&mut frames, 9, 5, 1);
                    set(&mut frames, 9, if frame & 1 == 1 { 4 } else { 6 }, 1);
                    set(&mut frames, 10, if frame & 1 == 1 { 3 } else { 7 }, 1);
                    for x in [4, 7] {
                        set(&mut frames, x, 8, 1);
                        set(&mut frames, x, 9, 6);
                        set(&mut frames, x + if frame & 1 == 1 { 1 } else { 0 }, 10, 6);
                    }
                }
                1 => {
                    for y in 7..=10 {
                        for x in 3..=8 {
                            set(&mut frames, x, y, if y == 7 { 3 } else { 4 });
                        }
                    }
                    for x in 2..=9 {
                        set(&mut frames, x, 7, 3);
                    }
                    for y in 2..=6 {
                        set(&mut frames, 5 + if frame == 1 { 1 } else { 0 }, y, 2);
                    }
                    for (x, y) in [
                        (3, 3),
                        (4, 3),
                        (4, 4),
                        (7, 2),
                        (8, 2),
                        (7, 3),
                        (6, 4),
                        (4, 5),
                    ] {
                        set(&mut frames, x - if frame == 3 { 1 } else { 0 }, y, 2);
                    }
                    set(&mut frames, 7, 1, if frame == 2 { 3 } else { 2 });
                }
                _ => {
                    for y in 3..=8 {
                        for x in 2..=9 {
                            set(&mut frames, x, y, 5);
                        }
                    }
                    for x in 2..=9 {
                        set(&mut frames, x, 3, 6);
                    }
                    for x in 3..=8 {
                        set(&mut frames, x, 4, 6);
                    }
                    for x in 4..=7 {
                        set(&mut frames, x, 5, 6);
                    }
                    set(&mut frames, 5, 6, 6);
                    set(&mut frames, 6, 6, 6);
                    let wing_y = if frame & 1 == 1 { 5 } else { 4 };
                    let tip_y = if frame & 1 == 1 { 6 } else { 3 };
                    set(&mut frames, 1, wing_y, 3);
                    set(&mut frames, 0, tip_y, 3);
                    set(&mut frames, 10, wing_y, 3);
                    set(&mut frames, 11, tip_y, 3);
                    set(&mut frames, 5, 2, 4);
                    set(&mut frames, 6, 2, 4);
                }
            }
        }
        Some(frames)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Snapshot {
    pub(super) frames: PixelFrames,
    pub(super) frame: u8,
    pub(super) template_id: u8,
}

impl Snapshot {
    pub(super) const EMPTY: Self = Self {
        frames: PixelFrames::EMPTY,
        frame: 0,
        template_id: 0,
    };
}

#[derive(Clone, Copy, Debug)]
pub(super) struct History {
    entries: [Snapshot; HISTORY_CAPACITY],
    start: u8,
    pub(super) len: u8,
}

impl History {
    pub(super) const fn new() -> Self {
        Self {
            entries: [Snapshot::EMPTY; HISTORY_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    pub(super) fn push(&mut self, snapshot: Snapshot) {
        if usize::from(self.len) == HISTORY_CAPACITY {
            self.entries[usize::from(self.start)] = snapshot;
            self.start = (self.start + 1) % HISTORY_CAPACITY as u8;
        } else {
            let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
            self.entries[usize::from(index)] = snapshot;
            self.len += 1;
        }
    }

    pub(super) fn pop(&mut self) -> Option<Snapshot> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
        Some(self.entries[usize::from(index)])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PixelTool {
    Brush,
    Erase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PixelModal {
    None,
    Templates,
    Clear,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PaintTransaction {
    pub(super) snapshot: Snapshot,
    pub(super) last_x: u8,
    pub(super) last_y: u8,
}
