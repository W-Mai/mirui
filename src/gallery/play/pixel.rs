use crate::gallery::play::change::ChangeSet;

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
    const EMPTY: Self = Self {
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

    fn set(&mut self, frame: u8, x: u8, y: u8, color: u8) -> bool {
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

    fn clear_frame(&mut self, frame: u8) {
        let start = usize::from(frame) * PIXELS_PER_FRAME / 2;
        self.bytes[start..start + PIXELS_PER_FRAME / 2].fill(0);
    }

    fn copy_frame(&mut self, destination: u8, source: u8) {
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
struct Snapshot {
    frames: PixelFrames,
    frame: u8,
    template_id: u8,
}

impl Snapshot {
    const EMPTY: Self = Self {
        frames: PixelFrames::EMPTY,
        frame: 0,
        template_id: 0,
    };
}

#[derive(Clone, Copy, Debug)]
struct History {
    entries: [Snapshot; HISTORY_CAPACITY],
    start: u8,
    len: u8,
}

impl History {
    const fn new() -> Self {
        Self {
            entries: [Snapshot::EMPTY; HISTORY_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn push(&mut self, snapshot: Snapshot) {
        if usize::from(self.len) == HISTORY_CAPACITY {
            self.entries[usize::from(self.start)] = snapshot;
            self.start = (self.start + 1) % HISTORY_CAPACITY as u8;
        } else {
            let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
            self.entries[usize::from(index)] = snapshot;
            self.len += 1;
        }
    }

    fn pop(&mut self) -> Option<Snapshot> {
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
struct PaintTransaction {
    snapshot: Snapshot,
    last_x: u8,
    last_y: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PixelModel {
    frames: PixelFrames,
    history: History,
    transaction: Option<PaintTransaction>,
    frame: u8,
    display_frame: u8,
    color: u8,
    tool: PixelTool,
    modal: PixelModal,
    template_id: u8,
    fps_index: u8,
    playback_ms: u16,
    mirror: bool,
    onion: bool,
    playing: bool,
}

impl Default for PixelModel {
    fn default() -> Self {
        Self {
            frames: PixelFrames::template(0).expect("built-in template"),
            history: History::new(),
            transaction: None,
            frame: 0,
            display_frame: 0,
            color: 1,
            tool: PixelTool::Brush,
            modal: PixelModal::None,
            template_id: 0,
            fps_index: 1,
            playback_ms: 0,
            mirror: false,
            onion: false,
            playing: false,
        }
    }
}

impl PixelModel {
    pub(crate) const fn frames(&self) -> &PixelFrames {
        &self.frames
    }

    pub(crate) const fn frame(&self) -> u8 {
        self.frame
    }

    pub(crate) const fn visible_frame(&self) -> u8 {
        if self.playing {
            self.display_frame
        } else {
            self.frame
        }
    }

    pub(crate) const fn color(&self) -> u8 {
        self.color
    }

    pub(crate) const fn tool(&self) -> PixelTool {
        self.tool
    }

    pub(crate) const fn modal(&self) -> PixelModal {
        self.modal
    }

    pub(crate) const fn template_id(&self) -> u8 {
        self.template_id
    }

    pub(crate) const fn fps(&self) -> u8 {
        [2, 4, 6, 8][self.fps_index as usize]
    }

    pub(crate) const fn mirror(&self) -> bool {
        self.mirror
    }

    pub(crate) const fn onion(&self) -> bool {
        self.onion
    }

    pub(crate) const fn playing(&self) -> bool {
        self.playing
    }

    pub(crate) const fn history_len(&self) -> u8 {
        self.history.len
    }

    const fn snapshot(&self) -> Snapshot {
        Snapshot {
            frames: self.frames,
            frame: self.frame,
            template_id: self.template_id,
        }
    }

    fn push_current(&mut self) {
        self.history.push(self.snapshot());
    }

    pub(crate) fn begin_stroke(&mut self, x: u8, y: u8) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None || x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return ChangeSet::NONE;
        }
        if self.transaction.is_none() {
            self.transaction = Some(PaintTransaction {
                snapshot: self.snapshot(),
                last_x: x,
                last_y: y,
            });
        }
        self.paint_pixel(x, y)
    }

    pub(crate) fn continue_stroke(&mut self, x: u8, y: u8) -> ChangeSet {
        if x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return ChangeSet::NONE;
        }
        let Some(transaction) = self.transaction.as_mut() else {
            return ChangeSet::NONE;
        };
        let (mut x0, mut y0) = (i16::from(transaction.last_x), i16::from(transaction.last_y));
        let (x1, y1) = (i16::from(x), i16::from(y));
        transaction.last_x = x;
        transaction.last_y = y;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let mut changed = false;
        for _ in 0..25 {
            changed |= self.paint_pixel_inner(x0 as u8, y0 as u8);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = error * 2;
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
        if changed {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    pub(crate) fn end_stroke(&mut self, cancelled: bool) -> ChangeSet {
        let Some(transaction) = self.transaction.take() else {
            return ChangeSet::NONE;
        };
        if cancelled {
            let changed = self.frames != transaction.snapshot.frames;
            self.frames = transaction.snapshot.frames;
            return if changed {
                ChangeSet::VISUAL
            } else {
                ChangeSet::NONE
            };
        }
        if self.frames == transaction.snapshot.frames {
            return ChangeSet::NONE;
        }
        self.history.push(transaction.snapshot);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn paint_pixel(&mut self, x: u8, y: u8) -> ChangeSet {
        if self.paint_pixel_inner(x, y) {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    fn paint_pixel_inner(&mut self, x: u8, y: u8) -> bool {
        let color = if self.tool == PixelTool::Erase {
            0
        } else {
            self.color
        };
        let mut changed = self.frames.set(self.frame, x, y, color);
        if self.mirror {
            changed |= self.frames.set(self.frame, GRID_WIDTH - 1 - x, y, color);
        }
        changed
    }

    pub(crate) fn select_frame(&mut self, frame: u8) -> ChangeSet {
        if self.playing
            || self.modal != PixelModal::None
            || frame >= FRAME_COUNT
            || frame == self.frame
        {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.frame = frame;
        self.display_frame = frame;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_color(&mut self, color: u8) -> ChangeSet {
        if self.playing || !(1..=COLOR_COUNT).contains(&color) {
            return ChangeSet::NONE;
        }
        self.color = color;
        self.tool = PixelTool::Brush;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_tool(&mut self, tool: PixelTool) -> ChangeSet {
        if self.playing || self.tool == tool {
            return ChangeSet::NONE;
        }
        self.tool = tool;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_mirror(&mut self) -> ChangeSet {
        if self.playing {
            return ChangeSet::NONE;
        }
        self.mirror = !self.mirror;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_onion(&mut self) -> ChangeSet {
        if self.playing {
            return ChangeSet::NONE;
        }
        self.onion = !self.onion;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn copy_previous(&mut self) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.push_current();
        self.frames
            .copy_frame(self.frame, (self.frame + FRAME_COUNT - 1) % FRAME_COUNT);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn open_clear(&mut self) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.modal = PixelModal::Clear;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn open_templates(&mut self) -> ChangeSet {
        self.end_stroke(true);
        self.playing = false;
        self.playback_ms = 0;
        self.modal = PixelModal::Templates;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        if self.modal == PixelModal::None {
            return ChangeSet::NONE;
        }
        self.modal = PixelModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn confirm_clear(&mut self) -> ChangeSet {
        if self.modal != PixelModal::Clear {
            return ChangeSet::NONE;
        }
        self.push_current();
        self.frames.clear_frame(self.frame);
        self.modal = PixelModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT | ChangeSet::PERSISTENCE
    }

    pub(crate) fn load_template(&mut self, template_id: u8) -> ChangeSet {
        let Some(frames) = PixelFrames::template(template_id) else {
            return ChangeSet::NONE;
        };
        self.end_stroke(true);
        self.push_current();
        self.frames = frames;
        self.frame = 0;
        self.display_frame = 0;
        self.template_id = template_id;
        self.playing = false;
        self.playback_ms = 0;
        self.modal = PixelModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT | ChangeSet::PERSISTENCE
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        let Some(snapshot) = self.history.pop() else {
            return ChangeSet::NONE;
        };
        self.frames = snapshot.frames;
        self.frame = snapshot.frame;
        self.display_frame = snapshot.frame;
        self.template_id = snapshot.template_id;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_playback(&mut self) -> ChangeSet {
        if self.modal != PixelModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.playing = !self.playing;
        self.playback_ms = 0;
        self.display_frame = self.frame;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn cycle_fps(&mut self) -> ChangeSet {
        self.fps_index = (self.fps_index + 1) % 4;
        self.playback_ms = 0;
        ChangeSet::MODEL
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        if !self.playing || self.modal != PixelModal::None {
            self.playback_ms = 0;
            return ChangeSet::NONE;
        }
        let period = [500_u16, 250, 166, 125][self.fps_index as usize];
        self.playback_ms = self.playback_ms.saturating_add(elapsed_ms.min(250));
        let mut advanced = false;
        for _ in 0..4 {
            if self.playback_ms < period {
                break;
            }
            self.playback_ms -= period;
            self.display_frame = (self.display_frame + 1) % FRAME_COUNT;
            advanced = true;
        }
        if advanced {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_have_four_independent_legal_frames() {
        for template in 0..3 {
            let mut frames = PixelFrames::template(template).unwrap();
            for frame in 0..FRAME_COUNT {
                for y in 0..GRID_HEIGHT {
                    for x in 0..GRID_WIDTH {
                        assert!(frames.get(frame, x, y) <= COLOR_COUNT);
                    }
                }
            }
            let before = frames.get(1, 0, 0);
            frames.set(0, 0, 0, 5);
            assert_eq!(frames.get(1, 0, 0), before);
        }
    }

    #[test]
    fn one_drag_is_one_undo_step_and_bresenham_has_no_gaps() {
        let mut model = PixelModel::default();
        model.frames.clear_frame(0);
        let original = model.frames;
        model.begin_stroke(0, 0);
        model.continue_stroke(11, 11);
        model.end_stroke(false);
        assert_eq!(model.history_len(), 1);
        for coordinate in 0..12 {
            assert_eq!(model.frames.get(0, coordinate, coordinate), 1);
        }
        model.undo();
        assert_eq!(model.frames, original);
    }

    #[test]
    fn cancelled_stroke_restores_preview_without_history() {
        let mut model = PixelModel::default();
        let original = model.frames;
        model.begin_stroke(0, 0);
        model.continue_stroke(11, 0);
        model.end_stroke(true);
        assert_eq!(model.frames, original);
        assert_eq!(model.history_len(), 0);
    }

    #[test]
    fn mirror_and_eraser_apply_to_both_sides() {
        let mut model = PixelModel::default();
        model.frames.clear_frame(0);
        model.select_color(3);
        model.toggle_mirror();
        model.begin_stroke(0, 0);
        model.end_stroke(false);
        assert_eq!(model.frames.get(0, 0, 0), 3);
        assert_eq!(model.frames.get(0, 11, 0), 3);
        model.set_tool(PixelTool::Erase);
        model.begin_stroke(0, 0);
        model.end_stroke(false);
        assert_eq!(model.frames.get(0, 0, 0), 0);
        assert_eq!(model.frames.get(0, 11, 0), 0);
    }

    #[test]
    fn copy_clear_and_template_load_are_atomic_and_undoable() {
        let mut model = PixelModel::default();
        model.select_frame(1);
        model.copy_previous();
        assert_eq!(model.frames.get(1, 5, 1), model.frames.get(0, 5, 1));
        model.open_clear();
        model.confirm_clear();
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                assert_eq!(model.frames.get(1, x, y), 0);
            }
        }
        model.undo();
        assert_ne!(model.frames.get(1, 5, 1), 0);
        let before = model.frames;
        model.open_templates();
        model.load_template(1);
        assert_ne!(model.frames, before);
        model.undo();
        assert_eq!(model.frames, before);
    }

    #[test]
    fn no_op_stroke_does_not_consume_history() {
        let mut model = PixelModel::default();
        model.set_tool(PixelTool::Erase);
        model.begin_stroke(0, 0);
        model.end_stroke(false);
        assert_eq!(model.history_len(), 0);
    }

    #[test]
    fn history_is_capped_at_sixteen_entries() {
        let mut model = PixelModel::default();
        for index in 0..50 {
            model.open_templates();
            model.load_template((index % 3) as u8);
        }
        assert_eq!(model.history_len(), 16);
        for _ in 0..16 {
            assert!(model.undo().contains(ChangeSet::MODEL));
        }
        assert_eq!(model.undo(), ChangeSet::NONE);
    }

    #[test]
    fn playback_advances_preview_without_mutating_pixels() {
        let mut model = PixelModel::default();
        let frames = model.frames;
        model.toggle_playback();
        assert_eq!(model.advance_ms(249), ChangeSet::NONE);
        assert!(model.advance_ms(1).contains(ChangeSet::VISUAL));
        assert_eq!(model.visible_frame(), 1);
        assert_eq!(model.frames, frames);
        model.toggle_playback();
        assert_eq!(model.advance_ms(500), ChangeSet::NONE);
    }

    #[test]
    fn model_storage_is_bounded() {
        assert!(core::mem::size_of::<PixelModel>() <= 5_500);
        assert_eq!(core::mem::size_of::<PixelFrames>(), 288);
    }
}
