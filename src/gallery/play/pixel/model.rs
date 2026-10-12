use super::types::{
    COLOR_COUNT, FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, History, PaintTransaction,
    PixelClipboardStatus, PixelFrame, PixelFrames, PixelModal, PixelTool, Snapshot,
};
use crate::gallery::play::change::ChangeSet;

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PixelModel {
    pub(super) frames: PixelFrames,
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
    edit_revision: u64,
    clipboard_status: PixelClipboardStatus,
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
            edit_revision: 0,
            clipboard_status: PixelClipboardStatus::Ready,
        }
    }
}

impl PixelModel {
    fn edited(&mut self) {
        self.edit_revision = self.edit_revision.saturating_add(1);
    }

    pub(crate) const fn frames(&self) -> &PixelFrames {
        &self.frames
    }

    #[cfg(test)]
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
        if changed {
            self.edited();
        }
        changed
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;

    #[test]
    fn exhausted_edit_revision_never_revalidates_an_old_snapshot() {
        let mut model = PixelModel {
            edit_revision: u64::MAX - 1,
            ..PixelModel::default()
        };
        model.select_frame(1);
        assert_eq!(model.selection_revision(), None);
        model.select_frame(0);
        assert_eq!(model.selection_revision(), None);
        assert!(model.replace_frame(PixelFrame::EMPTY).is_err());
    }
}

#[crate::model]
impl PixelModel {
    pub(crate) fn selection_revision(&self) -> Option<u64> {
        (self.transaction.is_none()
            && self.modal == PixelModal::None
            && self.edit_revision != u64::MAX)
            .then_some(self.edit_revision)
    }

    pub(crate) fn selected_frame(&self) -> PixelFrame {
        self.frames.frame(self.frame)
    }

    pub(crate) fn replace_frame(&mut self, pixels: PixelFrame) -> Result<ChangeSet, ()> {
        self.selection_revision().ok_or(())?;
        if self.selected_frame() == pixels {
            return Ok(ChangeSet::NONE);
        }
        self.push_current();
        self.frames.replace_frame(self.frame, pixels);
        self.edited();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE)
    }

    #[observe]
    pub(crate) fn clipboard_status(&self) -> PixelClipboardStatus {
        self.clipboard_status
    }

    pub(crate) fn set_clipboard_status(&mut self, status: PixelClipboardStatus) -> ChangeSet {
        self.clipboard_status = status;
        ChangeSet::MODEL
    }

    #[observe]
    pub(crate) fn frame(&self) -> u8 {
        self.frame
    }

    #[observe]
    pub(crate) fn visible_frame(&self) -> u8 {
        if self.playing {
            self.display_frame
        } else {
            self.frame
        }
    }

    #[observe]
    pub(crate) fn color(&self) -> u8 {
        self.color
    }

    #[observe]
    pub(crate) fn tool(&self) -> PixelTool {
        self.tool
    }

    #[observe]
    pub(crate) fn modal(&self) -> PixelModal {
        self.modal
    }

    #[observe]
    pub(crate) fn template_id(&self) -> u8 {
        self.template_id
    }

    #[observe]
    pub(crate) fn fps(&self) -> u8 {
        [2, 4, 6, 8][self.fps_index as usize]
    }

    #[observe]
    pub(crate) fn mirror(&self) -> bool {
        self.mirror
    }

    #[observe]
    pub(crate) fn onion(&self) -> bool {
        self.onion
    }

    #[observe]
    pub(crate) fn playing(&self) -> bool {
        self.playing
    }

    #[observe]
    pub(crate) fn can_undo(&self) -> bool {
        self.history.len > 0
    }

    pub(crate) fn begin_stroke(&mut self, x: u8, y: u8) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None || x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return ChangeSet::NONE;
        }
        if self.transaction.is_none() {
            self.edited();
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
            if changed {
                self.edited();
            }
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

    pub(crate) fn paint_cell(&mut self, x: u8, y: u8) -> ChangeSet {
        let changes = self.begin_stroke(x, y);
        changes | self.end_stroke(false)
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
        self.edited();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_color(&mut self, color: u8) -> ChangeSet {
        if self.playing || !(1..=COLOR_COUNT).contains(&color) {
            return ChangeSet::NONE;
        }
        self.color = color;
        self.tool = PixelTool::Brush;
        ChangeSet::MODEL
    }

    pub(crate) fn set_tool(&mut self, tool: PixelTool) -> ChangeSet {
        if self.playing || self.tool == tool {
            return ChangeSet::NONE;
        }
        self.tool = tool;
        ChangeSet::MODEL
    }

    pub(crate) fn toggle_mirror(&mut self) -> ChangeSet {
        if self.playing {
            return ChangeSet::NONE;
        }
        self.mirror = !self.mirror;
        ChangeSet::MODEL
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
        self.edited();
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn open_clear(&mut self) -> ChangeSet {
        if self.playing || self.modal != PixelModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.modal = PixelModal::Clear;
        self.edited();
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn open_templates(&mut self) -> ChangeSet {
        self.end_stroke(true);
        self.playing = false;
        self.playback_ms = 0;
        self.modal = PixelModal::Templates;
        self.edited();
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
        self.edited();
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
        self.edited();
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
        self.edited();
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
