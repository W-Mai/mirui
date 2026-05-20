use crate::ecs::Entity;
use crate::event::input::InputEvent;
use crate::types::Fixed;

use super::event::GestureEvent;
use super::system::GestureEvents;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GestureState {
    #[default]
    Idle,
    /// Single finger down, threshold not yet crossed.
    Pending,
    Dragging,
    LongPressed,
    /// Two fingers down, neither pinch nor rotate threshold crossed.
    MultiPending,
    /// At least one of pinch or rotate is currently emitting.
    MultiActive,
}

const DRAG_THRESHOLD: i32 = 10;
const LONG_PRESS_MS: u16 = 500;
/// 5% relative distance change wakes Pinch. Q24.8: 0.05 * 256 ≈ 13.
const PINCH_THRESHOLD: Fixed = Fixed::from_raw(13);
/// ≈0.1 rad (5.7°) wakes Rotate. Q24.8: 0.1 * 256 ≈ 26.
const ROTATE_THRESHOLD: Fixed = Fixed::from_raw(26);

pub(super) const MAX_FINGERS: usize = 4;

#[derive(Clone, Copy, Default)]
pub(super) struct Finger {
    pub id: u8,
    pub active: bool,
    pub start_x: Fixed,
    pub start_y: Fixed,
    pub current_x: Fixed,
    pub current_y: Fixed,
    pub prev_x: Fixed,
    pub prev_y: Fixed,
    pub prev_ms: u32,
    pub down_ms: u32,
}

#[derive(Default)]
pub struct GestureRecognizer {
    pub(super) state: GestureState,
    fingers: [Finger; MAX_FINGERS],
    /// First-finger hit-test target; persists for the whole interaction
    /// even after the finger lifts in multi-touch.
    target: Option<Entity>,

    // Multi-touch baseline captured when the second finger lands.
    initial_dist: Fixed,
    initial_angle: Fixed,
    last_emit_angle: Fixed,
    pinch_emitting: bool,
    rotate_emitting: bool,

    pub scroll_claimed: bool,
}

impl GestureRecognizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        event: &InputEvent,
        elapsed_ms: u32,
        hit_target: Option<Entity>,
        events_out: &mut GestureEvents,
    ) {
        if self.scroll_claimed {
            if matches!(event, InputEvent::PointerUp { .. }) {
                if let InputEvent::PointerUp { id, .. } = event {
                    self.release_finger(*id);
                }
                if self.active_count() == 0 {
                    self.reset();
                }
            }
            return;
        }

        match event {
            InputEvent::PointerDown { id, x, y } => {
                self.on_down(*id, *x, *y, elapsed_ms, hit_target, events_out)
            }
            InputEvent::PointerMove { id, x, y } => {
                self.on_move(*id, *x, *y, elapsed_ms, events_out)
            }
            InputEvent::PointerUp { id, x, y } => self.on_up(*id, *x, *y, elapsed_ms, events_out),
            _ => {}
        }
    }

    fn on_down(
        &mut self,
        id: u8,
        x: Fixed,
        y: Fixed,
        elapsed_ms: u32,
        hit_target: Option<Entity>,
        _events_out: &mut GestureEvents,
    ) {
        match self.state {
            GestureState::Idle => {
                let slot = 0;
                self.fingers[slot] = Finger {
                    id,
                    active: true,
                    start_x: x,
                    start_y: y,
                    current_x: x,
                    current_y: y,
                    prev_x: x,
                    prev_y: y,
                    prev_ms: elapsed_ms,
                    down_ms: elapsed_ms,
                };
                self.target = hit_target;
                self.state = GestureState::Pending;
            }
            GestureState::Pending => {
                if let Some(slot) = self.alloc_slot() {
                    self.fingers[slot] = Finger {
                        id,
                        active: true,
                        start_x: x,
                        start_y: y,
                        current_x: x,
                        current_y: y,
                        prev_x: x,
                        prev_y: y,
                        prev_ms: elapsed_ms,
                        down_ms: elapsed_ms,
                    };
                    self.capture_multi_baseline();
                    self.state = GestureState::MultiPending;
                }
            }
            // Drag in progress: ignore extra fingers (don't promote mid-drag,
            // user-facing experience matters more than feature coverage).
            GestureState::Dragging | GestureState::LongPressed => {}
            GestureState::MultiPending | GestureState::MultiActive => {
                // Ignore third / fourth finger while two are active.
            }
        }
    }

    fn on_move(
        &mut self,
        id: u8,
        x: Fixed,
        y: Fixed,
        elapsed_ms: u32,
        events_out: &mut GestureEvents,
    ) {
        let Some(slot) = self.find_slot(id) else {
            return;
        };
        let f = &mut self.fingers[slot];
        f.prev_x = f.current_x;
        f.prev_y = f.current_y;
        f.prev_ms = elapsed_ms;
        f.current_x = x;
        f.current_y = y;

        match self.state {
            GestureState::Pending => {
                let f0 = self.fingers[0];
                let dx = (f0.current_x - f0.start_x).to_int().abs();
                let dy = (f0.current_y - f0.start_y).to_int().abs();
                if dx + dy > DRAG_THRESHOLD {
                    self.state = GestureState::Dragging;
                    if let Some(target) = self.target {
                        events_out.push(GestureEvent::DragStart {
                            x: f0.start_x,
                            y: f0.start_y,
                            target,
                        });
                    }
                }
            }
            GestureState::Dragging => {
                let f0 = self.fingers[0];
                if let Some(target) = self.target {
                    events_out.push(GestureEvent::DragMove {
                        x: f0.current_x,
                        y: f0.current_y,
                        dx: f0.current_x - f0.start_x,
                        dy: f0.current_y - f0.start_y,
                        target,
                    });
                }
            }
            GestureState::MultiPending | GestureState::MultiActive => {
                self.emit_multi(events_out);
            }
            _ => {}
        }
    }

    fn on_up(
        &mut self,
        id: u8,
        x: Fixed,
        y: Fixed,
        elapsed_ms: u32,
        events_out: &mut GestureEvents,
    ) {
        let was = self.state;
        let active_before = self.active_count();
        let lifting = self.find_slot(id).map(|slot| self.fingers[slot]);
        self.release_finger(id);

        match was {
            GestureState::Pending => {
                if let Some(target) = self.target {
                    events_out.push(GestureEvent::Tap { x, y, target });
                }
                self.reset();
            }
            GestureState::Dragging => {
                if let (Some(target), Some(f)) = (self.target, lifting) {
                    let dt_ms = elapsed_ms.wrapping_sub(f.prev_ms).max(1);
                    let vx = (x - f.prev_x) * Fixed::from_int(1000) / Fixed::from_int(dt_ms as i32);
                    let vy = (y - f.prev_y) * Fixed::from_int(1000) / Fixed::from_int(dt_ms as i32);
                    events_out.push(GestureEvent::DragEnd {
                        x,
                        y,
                        vx,
                        vy,
                        target,
                    });
                }
                self.reset();
            }
            GestureState::LongPressed => {
                self.reset();
            }
            GestureState::MultiPending | GestureState::MultiActive => {
                // Either finger lifting ends multi-touch — simpler and matches
                // the physical "let go" experience. Don't fall back to single-
                // finger tracking on the survivor.
                if active_before <= 2 {
                    self.reset();
                }
            }
            GestureState::Idle => {}
        }
    }

    fn capture_multi_baseline(&mut self) {
        let (f0, f1) = (self.fingers[0], self.fingers[1]);
        let dx = f1.current_x - f0.current_x;
        let dy = f1.current_y - f0.current_y;
        self.initial_dist = dist(dx, dy).max(Fixed::from_raw(1));
        self.initial_angle = Fixed::atan2(dy, dx);
        self.last_emit_angle = self.initial_angle;
        self.pinch_emitting = false;
        self.rotate_emitting = false;
    }

    fn emit_multi(&mut self, events_out: &mut GestureEvents) {
        let f0 = self.fingers[0];
        let f1 = self.fingers[1];
        if !f0.active || !f1.active {
            return;
        }
        let dx = f1.current_x - f0.current_x;
        let dy = f1.current_y - f0.current_y;
        let cur_dist = dist(dx, dy);
        let cur_angle = Fixed::atan2(dy, dx);

        let scale = cur_dist / self.initial_dist;
        let angle_total = wrap_pi(cur_angle - self.initial_angle);
        let center_x = (f0.current_x + f1.current_x) / Fixed::from_int(2);
        let center_y = (f0.current_y + f1.current_y) / Fixed::from_int(2);

        let scale_off = (scale - Fixed::ONE).abs();
        if !self.pinch_emitting && scale_off > PINCH_THRESHOLD {
            self.pinch_emitting = true;
        }
        if !self.rotate_emitting && angle_total.abs() > ROTATE_THRESHOLD {
            self.rotate_emitting = true;
        }

        if self.pinch_emitting || self.rotate_emitting {
            self.state = GestureState::MultiActive;
        }

        if let Some(target) = self.target {
            if self.pinch_emitting {
                events_out.push(GestureEvent::Pinch {
                    x: center_x,
                    y: center_y,
                    scale,
                    target,
                });
            }
            if self.rotate_emitting {
                let incremental = wrap_pi(cur_angle - self.last_emit_angle);
                self.last_emit_angle = cur_angle;
                events_out.push(GestureEvent::Rotate {
                    x: center_x,
                    y: center_y,
                    angle: incremental,
                    target,
                });
            }
        }
    }

    fn alloc_slot(&self) -> Option<usize> {
        self.fingers.iter().position(|f| !f.active)
    }

    fn find_slot(&self, id: u8) -> Option<usize> {
        self.fingers.iter().position(|f| f.active && f.id == id)
    }

    fn active_count(&self) -> usize {
        self.fingers.iter().filter(|f| f.active).count()
    }

    fn release_finger(&mut self, id: u8) {
        if let Some(slot) = self.find_slot(id) {
            self.fingers[slot] = Finger::default();
        }
    }

    pub fn check_long_press(&mut self, elapsed_ms: u32, events_out: &mut GestureEvents) {
        if self.state == GestureState::Pending && !self.scroll_claimed {
            let held = elapsed_ms.wrapping_sub(self.fingers[0].down_ms);
            if held >= LONG_PRESS_MS as u32 {
                self.state = GestureState::LongPressed;
                if let Some(target) = self.target {
                    events_out.push(GestureEvent::LongPress {
                        x: self.fingers[0].current_x,
                        y: self.fingers[0].current_y,
                        target,
                    });
                }
            }
        }
    }

    fn reset(&mut self) {
        self.state = GestureState::Idle;
        self.target = None;
        self.scroll_claimed = false;
        for f in self.fingers.iter_mut() {
            *f = Finger::default();
        }
        self.pinch_emitting = false;
        self.rotate_emitting = false;
    }
}

fn dist(dx: Fixed, dy: Fixed) -> Fixed {
    (dx * dx + dy * dy).sqrt()
}

/// Keep a (signed) angle in (-π, π] so `cur - prev` doesn't jump 2π
/// at the wrap boundary.
fn wrap_pi(mut a: Fixed) -> Fixed {
    let pi = Fixed::PI;
    let two_pi = pi + pi;
    while a > pi {
        a -= two_pi;
    }
    while a <= -pi {
        a += two_pi;
    }
    a
}
