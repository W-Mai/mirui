use super::presets;
use super::transport::{MAX_RECORDED_NOTES, MAX_SOUND_EVENTS, RecordedNote};
use super::types::{
    Ball, MAX_BALLS, MAX_PADS, MAX_PARTICLES, MAX_RINGS, MarbleSound, PAD_PITCHES, PALETTE, Pad,
    PadTimbre, Page, Particle, Ring, THEMES, Theme, Vec2,
};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::gallery::play::input::DragTransaction;
use crate::types::Fixed64;

#[derive(Clone, Copy, Debug)]
struct Drag {
    slot: Option<usize>,
    start: Vec2,
    offset: Vec2,
    position: Option<DragTransaction<Vec2>>,
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Debug)]
pub(crate) struct MarbleModel {
    #[observe]
    pub(crate) page: Page,
    #[observe]
    pub(crate) paused: bool,
    #[observe]
    pub(crate) scene: usize,
    #[observe]
    pub(crate) gravity: Fixed64,
    #[observe]
    pub(crate) trails: bool,
    #[observe]
    pub(crate) feedback: bool,
    #[observe]
    pub(crate) inspector: bool,
    #[observe]
    pub(crate) add_mode: bool,
    pub(crate) pads: [Option<Pad>; MAX_PADS],
    pub(crate) balls: [Option<Ball>; MAX_BALLS],
    pub(crate) rings: [Option<Ring>; MAX_RINGS],
    pub(crate) particles: [Option<Particle>; MAX_PARTICLES],
    #[observe]
    pub(crate) selected: usize,
    #[observe]
    pub(crate) hits: u32,
    pub(crate) flash: Fixed64,
    pub(crate) tilt: Vec2,
    pub(crate) target: Vec2,
    #[observe]
    pub(crate) recording: bool,
    #[observe]
    pub(crate) looping: bool,
    #[observe]
    pub(crate) bpm: u16,
    pub(crate) toast: &'static str,
    pub(crate) toast_left: Fixed64,
    pub(super) sim_time: Fixed64,
    pub(super) next_id: u32,
    pub(super) rng: u32,
    pub(super) keys: u8,
    drag: Option<Drag>,
    clock: BoundedClock,
    pub(super) ring_cursor: usize,
    pub(super) particle_cursor: usize,
    pub(super) sounds: [Option<MarbleSound>; MAX_SOUND_EVENTS],
    pub(super) sound_len: u8,
    pub(super) transport_steps: Fixed64,
    pub(super) last_audio_step: u32,
    pub(super) record_start_step: u32,
    pub(super) loop_start_step: u32,
    pub(super) loop_length_steps: u8,
    pub(super) recorded: [Option<RecordedNote>; MAX_RECORDED_NOTES],
    pub(super) recorded_len: u8,
}

impl Default for MarbleModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MarbleModel {
    pub(crate) fn new() -> Self {
        let mut model = Self {
            page: Page::Play,
            paused: false,
            scene: 0,
            gravity: THEMES[0].gravity,
            trails: true,
            feedback: true,
            inspector: false,
            add_mode: false,
            pads: [None; MAX_PADS],
            balls: [None; MAX_BALLS],
            rings: [None; MAX_RINGS],
            particles: [None; MAX_PARTICLES],
            selected: 0,
            hits: 0,
            flash: Fixed64::ZERO,
            tilt: Vec2::default(),
            target: Vec2::default(),
            recording: false,
            looping: false,
            bpm: 96,
            toast: "",
            toast_left: Fixed64::ZERO,
            sim_time: Fixed64::ZERO,
            next_id: 6,
            rng: 0x2a71_b893,
            keys: 0,
            drag: None,
            clock: BoundedClock::new(120, 60, 8),
            ring_cursor: 0,
            particle_cursor: 0,
            sounds: [None; MAX_SOUND_EVENTS],
            sound_len: 0,
            transport_steps: Fixed64::ZERO,
            last_audio_step: 0,
            record_start_step: 0,
            loop_start_step: 0,
            loop_length_steps: 16,
            recorded: [None; MAX_RECORDED_NOTES],
            recorded_len: 0,
        };
        model.reset_scene(0);
        model.toast = "";
        model.toast_left = Fixed64::ZERO;
        model
    }

    pub(crate) fn theme(&self) -> Theme {
        THEMES[self.scene]
    }

    pub(crate) fn selected_pad(&self) -> Pad {
        self.pads[self.selected]
            .as_ref()
            .copied()
            .expect("a marble scene always owns at least one pad")
    }

    pub(crate) fn running(&self) -> bool {
        self.page == Page::Play && !self.paused
    }

    pub(crate) fn needs_animation(&self) -> bool {
        self.running()
            || self.toast_left.is_positive()
            || self.flash.is_positive()
            || self.recording
            || self.looping
            || self
                .pads
                .iter()
                .flatten()
                .any(|pad| pad.pulse.is_positive())
            || self.rings.iter().any(Option::is_some)
            || self.particles.iter().any(Option::is_some)
    }

    pub(super) fn notify(&mut self, message: &'static str) {
        self.toast = message;
        self.toast_left = Fixed64::from_ratio(17, 10);
    }

    fn pad_at(&self, point: Vec2) -> Option<usize> {
        (0..MAX_PADS).rev().find(|&slot| {
            self.pads[slot]
                .map(|pad| {
                    Vec2::fixed(pad.pos.x - point.x, pad.pos.y - point.y).length()
                        < pad.radius + Fixed64::from_int(5)
                })
                .unwrap_or(false)
        })
    }

    fn bounded(pad: &Pad, position: Vec2) -> Vec2 {
        Vec2::fixed(
            position.x.clamp(
                Fixed64::from_int(18) + pad.radius,
                Fixed64::from_int(462) - pad.radius,
            ),
            position.y.clamp(
                Fixed64::from_int(58) + pad.radius,
                Fixed64::from_int(245) - pad.radius,
            ),
        )
    }

    pub(crate) fn cancel_input(&mut self) {
        let _ = self.end_board_drag(true);
        self.keys = 0;
        self.clock.reset();
    }
}

#[crate::model]
impl MarbleModel {
    #[observe]
    pub(crate) fn pad_count(&self) -> usize {
        self.pads.iter().flatten().count()
    }

    #[observe]
    pub(crate) fn ball_count(&self) -> usize {
        self.balls.iter().flatten().count()
    }

    #[observe]
    pub(crate) fn selected_letter(&self) -> u8 {
        self.selected_pad().letter
    }

    #[observe]
    pub(crate) fn selected_radius(&self) -> Fixed64 {
        self.selected_pad().radius
    }

    #[observe]
    pub(crate) fn selected_bounce(&self) -> Fixed64 {
        self.selected_pad().bounce
    }

    #[observe]
    pub(crate) fn selected_pitch(&self) -> u8 {
        self.selected_pad().pitch
    }

    #[observe]
    pub(crate) fn selected_timbre(&self) -> PadTimbre {
        self.selected_pad().timbre
    }

    pub(crate) fn reset_scene(&mut self, index: usize) -> ChangeSet {
        presets::reset_scene(self, index)
    }

    pub(crate) fn set_page(&mut self, page: Page) -> ChangeSet {
        self.cancel_input();
        self.page = page;
        self.inspector = false;
        self.add_mode = false;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn toggle_pause(&mut self) -> ChangeSet {
        if self.page != Page::Play {
            let _ = self.set_page(Page::Play);
        }
        self.paused = !self.paused;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_recording(&mut self) -> ChangeSet {
        if self.recording {
            self.finish_recording(true);
        } else if self.looping {
            self.stop_recording();
            self.notify("LOOP STOPPED / FREE PLAY");
        } else {
            if self.page != Page::Play {
                let _ = self.set_page(Page::Play);
            }
            self.paused = false;
            self.recorded = [None; MAX_RECORDED_NOTES];
            self.recorded_len = 0;
            self.recording = true;
            self.looping = false;
            self.record_start_step = self.current_audio_step();
            self.notify("RECORD 4 BARS / PLAY THE PADS");
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn drop_ball(&mut self) -> ChangeSet {
        if self.inspector {
            return ChangeSet::NONE;
        }
        if self.page != Page::Play {
            let _ = self.set_page(Page::Play);
        }
        let vx = Fixed64::from_int(65) + Fixed64::from_ratio(i64::from(self.random() % 36), 1);
        let vy = Fixed64::from_int(-15) + Fixed64::from_ratio(i64::from(self.random() % 36), 1);
        if self.spawn_at(Vec2::new(53, 73), Vec2::fixed(vx, vy)) {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL
        }
    }

    #[model(local)]
    pub(crate) fn add_pad(&mut self, position: Vec2) -> ChangeSet {
        let Some(slot) = self.pads.iter().position(Option::is_none) else {
            self.notify("MAXIMUM 6 PADS");
            return ChangeSet::VISUAL;
        };
        let letter = (b'A'..=b'F')
            .find(|letter| !self.pads.iter().flatten().any(|pad| pad.letter == *letter))
            .unwrap_or(b'F');
        let mut pad = Pad {
            id: self.next_id,
            letter,
            pos: position,
            radius: Fixed64::from_int(16),
            color: PALETTE[5],
            bounce: Fixed64::from_ratio(11, 10),
            pulse: Fixed64::ZERO,
            pitch: 72,
            timbre: PadTimbre::Mallet,
            last_hit: Fixed64::from_int(-99),
        };
        pad.pos = Self::bounded(&pad, position);
        if self.pads.iter().flatten().any(|other| {
            Vec2::fixed(other.pos.x - pad.pos.x, other.pos.y - pad.pos.y).length()
                < other.radius + pad.radius + Fixed64::from_int(5)
        }) {
            self.notify("TOO CLOSE / CHOOSE EMPTY SPACE");
            return ChangeSet::VISUAL;
        }
        self.next_id = self.next_id.saturating_add(1);
        self.pads[slot] = Some(pad);
        self.selected = slot;
        self.add_mode = false;
        for ball in self.balls.iter_mut().flatten() {
            ball.cooldown[slot] = Fixed64::from_int(-99);
        }
        self.pulse(slot, false);
        self.notify("PAD ADDED");
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn remove_selected(&mut self) -> ChangeSet {
        if self.pad_count() <= 1 {
            self.notify("KEEP AT LEAST ONE PAD");
            return ChangeSet::VISUAL;
        }
        let slot = self.selected;
        self.pads[slot] = None;
        for ball in self.balls.iter_mut().flatten() {
            ball.cooldown[slot] = Fixed64::from_int(-99);
        }
        self.selected = self
            .pads
            .iter()
            .position(Option::is_some)
            .expect("removing a pad preserves at least one pad");
        self.inspector = false;
        self.notify("PAD REMOVED");
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_add_mode(&mut self) -> ChangeSet {
        if self.page != Page::Edit {
            let _ = self.set_page(Page::Edit);
        }
        if self.pad_count() >= MAX_PADS {
            self.notify("MAXIMUM 6 PADS");
            return ChangeSet::VISUAL;
        }
        self.add_mode = !self.add_mode;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_inspector(&mut self, open: bool) -> ChangeSet {
        if open && self.page != Page::Edit {
            return ChangeSet::NONE;
        }
        self.inspector = open;
        self.add_mode = false;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn set_gravity(&mut self, value: Fixed64) -> ChangeSet {
        let clamped = value.clamp(Fixed64::ZERO, Fixed64::from_ratio(16, 10));
        let step = Fixed64::from_ratio(5, 100);
        let steps = ((clamped + step / 2) / step).to_int();
        let next = Fixed64::from_ratio(steps * 5, 100);
        if self.gravity == next {
            return ChangeSet::NONE;
        }
        self.gravity = next;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_radius(&mut self, value: Fixed64) -> ChangeSet {
        if let Some(pad) = self.pads[self.selected].as_mut() {
            pad.radius = Fixed64::from_int(
                (value + Fixed64::from_ratio(1, 2))
                    .clamp(Fixed64::from_int(13), Fixed64::from_ratio(235, 10))
                    .to_int(),
            );
            pad.pos = Self::bounded(pad, pad.pos);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_bounce(&mut self, value: Fixed64) -> ChangeSet {
        let clamped = value.clamp(Fixed64::from_ratio(7, 10), Fixed64::from_ratio(14, 10));
        let steps = ((clamped - Fixed64::from_ratio(7, 10) + Fixed64::from_ratio(25, 1_000))
            / Fixed64::from_ratio(5, 100))
        .to_int();
        let next = Fixed64::from_ratio(70 + steps * 5, 100);
        let Some(pad) = self.pads[self.selected].as_mut() else {
            return ChangeSet::NONE;
        };
        if pad.bounce == next {
            return ChangeSet::NONE;
        }
        pad.bounce = next;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_color(&mut self, index: usize) -> ChangeSet {
        if index < 5 {
            if let Some(pad) = self.pads[self.selected].as_mut() {
                pad.color = PALETTE[index];
            }
            self.pulse(self.selected, false);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn adjust_pitch(&mut self, direction: i8) -> ChangeSet {
        let current = self.selected_pad().pitch;
        let nearest = PAD_PITCHES
            .iter()
            .enumerate()
            .min_by_key(|(_, pitch)| pitch.abs_diff(current))
            .map(|(index, _)| index)
            .unwrap_or(0);
        let next = if direction < 0 {
            nearest.saturating_sub(1)
        } else {
            (nearest + 1).min(PAD_PITCHES.len() - 1)
        };
        if let Some(pad) = self.pads[self.selected].as_mut() {
            pad.pitch = PAD_PITCHES[next];
        }
        self.pulse(self.selected, false);
        self.emit_pad_sound(self.selected, 190, true);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn cycle_timbre(&mut self) -> ChangeSet {
        if let Some(pad) = self.pads[self.selected].as_mut() {
            pad.timbre = pad.timbre.next();
        }
        self.pulse(self.selected, false);
        self.emit_pad_sound(self.selected, 190, true);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_bpm(&mut self, value: Fixed64) -> ChangeSet {
        let next = (value + Fixed64::from_ratio(1, 2)).to_int().clamp(55, 160) as u16;
        if self.bpm == next {
            return ChangeSet::NONE;
        }
        self.bpm = next;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_trails(&mut self) -> ChangeSet {
        self.trails = !self.trails;
        if !self.trails {
            for ball in self.balls.iter_mut().flatten() {
                ball.trail_len = 0;
            }
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_feedback(&mut self) -> ChangeSet {
        self.feedback = !self.feedback;
        if !self.feedback {
            self.rings = [None; MAX_RINGS];
            self.particles = [None; MAX_PARTICLES];
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn begin_board_drag(&mut self, point: Vec2) -> ChangeSet {
        if self.inspector
            || !matches!(self.page, Page::Play | Page::Edit)
            || point.y < Fixed64::from_int(52)
            || point.y > Fixed64::from_int(251)
        {
            return ChangeSet::NONE;
        }
        if self.add_mode {
            return self.add_pad(point);
        }
        let slot = self.pad_at(point);
        let original = slot.map(|i| self.pads[i].unwrap().pos).unwrap_or(point);
        self.drag = Some(Drag {
            slot,
            start: point,
            offset: Vec2::fixed(point.x - original.x, point.y - original.y),
            position: slot.map(|_| DragTransaction::begin(original)),
        });
        if let Some(slot) = slot {
            self.selected = slot;
            self.pulse(slot, false);
            self.emit_pad_sound(slot, 190, true);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn tap_board(&mut self, point: Vec2) -> ChangeSet {
        let changes = self.begin_board_drag(point);
        if changes == ChangeSet::NONE || self.drag.is_none() {
            return changes;
        }
        changes | self.end_board_drag(false)
    }

    pub(crate) fn move_board_drag(&mut self, point: Vec2) -> ChangeSet {
        let Some(mut drag) = self.drag else {
            return ChangeSet::NONE;
        };
        if self.page == Page::Edit {
            if let (Some(slot), Some(mut transaction)) = (drag.slot, drag.position) {
                if let Some(pad) = self.pads[slot].as_mut() {
                    let next = Self::bounded(
                        pad,
                        Vec2::fixed(point.x - drag.offset.x, point.y - drag.offset.y),
                    );
                    pad.pos = transaction.update(next);
                    drag.position = Some(transaction);
                }
            }
        } else if self.page == Page::Play && drag.slot.is_none() {
            self.target = Vec2::fixed(
                ((point.x - drag.start.x) / Fixed64::from_int(75))
                    .clamp(-Fixed64::ONE, Fixed64::ONE),
                ((point.y - drag.start.y) / Fixed64::from_int(65))
                    .clamp(-Fixed64::from_ratio(14, 10), Fixed64::ONE),
            );
        }
        self.drag = Some(drag);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn end_board_drag(&mut self, cancel: bool) -> ChangeSet {
        let Some(drag) = self.drag.take() else {
            if self.target == Vec2::default() {
                return ChangeSet::NONE;
            }
            self.target = Vec2::default();
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        };
        let mut changes = ChangeSet::MODEL | ChangeSet::VISUAL;
        if let (Some(slot), Some(transaction)) = (drag.slot, drag.position)
            && let Some(pad) = self.pads[slot].as_mut()
        {
            let changed = transaction.commit() != transaction.cancel();
            pad.pos = if cancel {
                transaction.cancel()
            } else {
                transaction.commit()
            };
            if changed && !cancel {
                changes = changes | ChangeSet::PERSISTENCE;
            }
        }
        self.target = Vec2::default();
        changes
    }

    #[effects]
    pub(crate) fn take_sounds(&mut self) -> [Option<MarbleSound>; MAX_SOUND_EVENTS] {
        let sounds = self.sounds;
        self.sounds = [None; MAX_SOUND_EVENTS];
        self.sound_len = 0;
        sounds
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        if elapsed_ms == 0 {
            return ChangeSet::NONE;
        }
        self.transport_steps +=
            Fixed64::from_ratio(i64::from(elapsed_ms) * i64::from(self.bpm), 15_000);
        let active = self.running();
        let steps = self.clock.steps(elapsed_ms, active);
        for _ in 0..steps {
            self.physics_step();
        }
        let dt = Fixed64::from_ratio(i64::from(elapsed_ms.min(60)), 1_000);
        let animated = self.needs_animation();
        for pad in self.pads.iter_mut().flatten() {
            pad.pulse = (pad.pulse - dt * Fixed64::from_ratio(34, 10)).max(Fixed64::ZERO);
        }
        for ring in &mut self.rings {
            if let Some(value) = ring {
                value.age += dt;
                value.radius += dt * Fixed64::from_int(23);
                if value.age >= Fixed64::from_ratio(45, 100) {
                    *ring = None;
                }
            }
        }
        for particle in &mut self.particles {
            if let Some(value) = particle {
                value.pos.x += value.velocity.x * dt;
                value.pos.y += value.velocity.y * dt;
                value.age += dt;
                if value.age >= Fixed64::from_ratio(43, 100) {
                    *particle = None;
                }
            }
        }
        self.flash = (self.flash - dt * Fixed64::from_int(2)).max(Fixed64::ZERO);
        self.toast_left = (self.toast_left - dt).max(Fixed64::ZERO);
        if self.toast_left.is_zero() {
            self.toast = "";
        }
        let audio_model_changed = self.advance_audio();
        if audio_model_changed {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else if animated {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }
}
