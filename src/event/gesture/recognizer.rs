use crate::ecs::Entity;
use crate::event::input::InputEvent;
use crate::types::Fixed;

use super::event::GestureEvent;
use super::system::GestureEvents;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GestureState {
    #[default]
    Idle,
    Pending,
    Dragging,
    LongPressed,
}

const DRAG_THRESHOLD: i32 = 10;
const LONG_PRESS_MS: u16 = 500;

#[derive(Default)]
pub struct GestureRecognizer {
    pub(super) state: GestureState,
    pointer_id: u8,
    start_x: Fixed,
    start_y: Fixed,
    current_x: Fixed,
    current_y: Fixed,
    down_elapsed_ms: u32,
    target: Option<Entity>,
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
                self.reset();
            }
            return;
        }

        match event {
            InputEvent::PointerDown { id, x, y } => {
                if self.state != GestureState::Idle {
                    return;
                }
                self.state = GestureState::Pending;
                self.pointer_id = *id;
                self.start_x = *x;
                self.start_y = *y;
                self.current_x = *x;
                self.current_y = *y;
                self.down_elapsed_ms = elapsed_ms;
                self.target = hit_target;
            }
            InputEvent::PointerMove { id, x, y } => {
                if *id != self.pointer_id {
                    return;
                }
                self.current_x = *x;
                self.current_y = *y;

                match self.state {
                    GestureState::Pending => {
                        let dx = (*x - self.start_x).to_int().abs();
                        let dy = (*y - self.start_y).to_int().abs();
                        if dx + dy > DRAG_THRESHOLD {
                            self.state = GestureState::Dragging;
                            if let Some(target) = self.target {
                                events_out.push(GestureEvent::DragStart {
                                    x: self.start_x,
                                    y: self.start_y,
                                    target,
                                });
                            }
                        }
                    }
                    GestureState::Dragging => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::DragMove {
                                x: *x,
                                y: *y,
                                dx: *x - self.start_x,
                                dy: *y - self.start_y,
                                target,
                            });
                        }
                    }
                    _ => {}
                }
            }
            InputEvent::PointerUp { id, x, y } => {
                if *id != self.pointer_id {
                    return;
                }
                match self.state {
                    GestureState::Pending => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::Tap {
                                x: *x,
                                y: *y,
                                target,
                            });
                        }
                    }
                    GestureState::Dragging => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::DragEnd {
                                x: *x,
                                y: *y,
                                target,
                            });
                        }
                    }
                    GestureState::LongPressed => {}
                    GestureState::Idle => {}
                }
                self.reset();
            }
            _ => {}
        }
    }

    pub fn check_long_press(&mut self, elapsed_ms: u32, events_out: &mut GestureEvents) {
        if self.state == GestureState::Pending && !self.scroll_claimed {
            let held = elapsed_ms.wrapping_sub(self.down_elapsed_ms);
            if held >= LONG_PRESS_MS as u32 {
                self.state = GestureState::LongPressed;
                if let Some(target) = self.target {
                    events_out.push(GestureEvent::LongPress {
                        x: self.current_x,
                        y: self.current_y,
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
    }
}
