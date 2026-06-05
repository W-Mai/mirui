use alloc::collections::VecDeque;

use crate::event::input::InputEvent;
use crate::types::Fixed;

/// Driver-agnostic axis events fed into [`PointerState`]. Each backend
/// translates its native input protocol (Linux evdev, NuttX
/// `touch_sample_s`, etc.) into a stream of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputAxis {
    Slot(u8),
    TrackingId(i32),
    AbsX(i32),
    AbsY(i32),
    RelX(i32),
    RelY(i32),
    RelWheel(i32),
    RelHWheel(i32),
    Button(bool),
    Sync,
}

pub(crate) struct PointerState {
    width: u16,
    height: u16,
    abs_min_x: i32,
    abs_max_x: i32,
    abs_min_y: i32,
    abs_max_y: i32,
    slot: u8,
    last_xy: [(Fixed, Fixed); 16],
    last_down: [bool; 16],
    dirty: u16,
    wheel_dx: i32,
    wheel_dy: i32,
}

impl PointerState {
    pub(crate) fn new(
        width: u16,
        height: u16,
        abs_min_x: i32,
        abs_max_x: i32,
        abs_min_y: i32,
        abs_max_y: i32,
    ) -> Self {
        Self {
            width,
            height,
            abs_min_x,
            abs_max_x,
            abs_min_y,
            abs_max_y,
            slot: 0,
            last_xy: [(Fixed::ZERO, Fixed::ZERO); 16],
            last_down: [false; 16],
            dirty: 0,
            wheel_dx: 0,
            wheel_dy: 0,
        }
    }

    pub(crate) fn process(&mut self, axis: InputAxis, queue: &mut VecDeque<InputEvent>) {
        match axis {
            InputAxis::Slot(s) => {
                self.slot = s.min(15);
            }
            InputAxis::TrackingId(value) => {
                let i = self.slot as usize;
                if value < 0 {
                    if self.last_down[i] {
                        let (x, y) = self.last_xy[i];
                        queue.push_back(InputEvent::PointerUp {
                            id: self.slot,
                            x,
                            y,
                        });
                        self.last_down[i] = false;
                    }
                } else if !self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerDown {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = true;
                }
            }
            InputAxis::AbsX(value) => {
                let i = self.slot as usize;
                self.last_xy[i].0 =
                    map_axis(value, self.abs_min_x, self.abs_max_x, self.width as i32);
                self.dirty |= 1 << i;
            }
            InputAxis::AbsY(value) => {
                let i = self.slot as usize;
                self.last_xy[i].1 =
                    map_axis(value, self.abs_min_y, self.abs_max_y, self.height as i32);
                self.dirty |= 1 << i;
            }
            InputAxis::RelX(delta) => {
                let nx = (self.last_xy[0].0 + Fixed::from_int(delta))
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int(self.width as i32 - 1));
                self.last_xy[0].0 = nx;
                self.dirty |= 1;
            }
            InputAxis::RelY(delta) => {
                let ny = (self.last_xy[0].1 + Fixed::from_int(delta))
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int(self.height as i32 - 1));
                self.last_xy[0].1 = ny;
                self.dirty |= 1;
            }
            InputAxis::RelWheel(delta) => self.wheel_dy = self.wheel_dy.saturating_add(delta),
            InputAxis::RelHWheel(delta) => self.wheel_dx = self.wheel_dx.saturating_add(delta),
            InputAxis::Button(true) => {
                let i = self.slot as usize;
                if !self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerDown {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = true;
                }
            }
            InputAxis::Button(false) => {
                let i = self.slot as usize;
                if self.last_down[i] {
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerUp {
                        id: self.slot,
                        x,
                        y,
                    });
                    self.last_down[i] = false;
                }
            }
            InputAxis::Sync => {
                let dirty = core::mem::take(&mut self.dirty);
                for i in 0..16 {
                    if dirty & (1 << i) == 0 {
                        continue;
                    }
                    let (x, y) = self.last_xy[i];
                    queue.push_back(InputEvent::PointerMove { id: i as u8, x, y });
                }
                let dx = core::mem::take(&mut self.wheel_dx);
                let dy = core::mem::take(&mut self.wheel_dy);
                if dx != 0 || dy != 0 {
                    let (x, y) = self.last_xy[0];
                    queue.push_back(InputEvent::Wheel {
                        dx: Fixed::from_int(dx),
                        dy: Fixed::from_int(dy),
                        x,
                        y,
                    });
                }
            }
        }
    }
}

fn map_axis(value: i32, min: i32, max: i32, screen: i32) -> Fixed {
    let span = (max - min).max(1);
    let v = value.saturating_sub(min);
    let scaled = (v as i64 * screen as i64 / span as i64) as i32;
    Fixed::from_int(scaled.max(0).min(screen))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn drain(state: &mut PointerState, axes: &[InputAxis]) -> Vec<InputEvent> {
        let mut queue = VecDeque::new();
        for &a in axes {
            state.process(a, &mut queue);
        }
        queue.into_iter().collect()
    }

    fn st() -> PointerState {
        PointerState::new(800, 600, 0, 32767, 0, 32767)
    }

    #[test]
    fn hover_emits_pointer_move_without_button() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(16383),
                InputAxis::AbsY(16383),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 1, "hover sync must emit a single PointerMove");
        match &events[0] {
            InputEvent::PointerMove { id, x, y } => {
                assert_eq!(*id, 0);
                assert!((x.to_int() - 400).abs() <= 1);
                assert!((y.to_int() - 300).abs() <= 1);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }

    #[test]
    fn axis_without_sync_does_not_emit() {
        let mut s = st();
        let events = drain(&mut s, &[InputAxis::AbsX(100), InputAxis::AbsY(200)]);
        assert!(events.is_empty(), "no Sync = no move yet, got {events:?}");
    }

    #[test]
    fn button_press_release_emits_down_then_up() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(0),
                InputAxis::AbsY(0),
                InputAxis::Button(true),
                InputAxis::Sync,
                InputAxis::Button(false),
                InputAxis::Sync,
            ],
        );
        assert!(matches!(events[0], InputEvent::PointerDown { id: 0, .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { id: 0, .. }));
        assert!(matches!(events[2], InputEvent::PointerUp { id: 0, .. }));
    }

    #[test]
    fn drag_after_button_emits_move_per_sync() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::Button(true),
                InputAxis::AbsX(8192),
                InputAxis::Sync,
                InputAxis::AbsX(16384),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], InputEvent::PointerDown { .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { .. }));
        assert!(matches!(events[2], InputEvent::PointerMove { .. }));
    }

    #[test]
    fn multitouch_tracking_id_drives_down_up() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::Slot(1),
                InputAxis::TrackingId(42),
                InputAxis::AbsX(0),
                InputAxis::AbsY(0),
                InputAxis::Sync,
                InputAxis::TrackingId(-1),
                InputAxis::Sync,
            ],
        );
        assert!(matches!(events[0], InputEvent::PointerDown { id: 1, .. }));
        assert!(matches!(events[1], InputEvent::PointerMove { id: 1, .. }));
        assert!(matches!(events[2], InputEvent::PointerUp { id: 1, .. }));
    }

    #[test]
    fn rel_xy_accumulates_into_pointer_move() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelX(50),
                InputAxis::RelY(30),
                InputAxis::Sync,
                InputAxis::RelX(20),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 50);
                assert_eq!(y.to_int(), 30);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
        match &events[1] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 70);
                assert_eq!(y.to_int(), 30);
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }

    #[test]
    fn rel_xy_clamps_to_screen_bounds() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelX(-100),
                InputAxis::RelY(-100),
                InputAxis::Sync,
                InputAxis::RelX(10000),
                InputAxis::RelY(10000),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 0);
                assert_eq!(y.to_int(), 0);
            }
            _ => panic!(),
        }
        match &events[1] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 799);
                assert_eq!(y.to_int(), 599);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn wheel_coalesces_into_single_event_per_sync() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::RelWheel(1),
                InputAxis::RelWheel(2),
                InputAxis::RelHWheel(-1),
                InputAxis::Sync,
            ],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            InputEvent::Wheel { dx, dy, .. } => {
                assert_eq!(dx.to_int(), -1);
                assert_eq!(dy.to_int(), 3);
            }
            other => panic!("expected Wheel, got {other:?}"),
        }
    }

    #[test]
    fn axis_clamps_outside_calibration_range() {
        let mut s = st();
        let events = drain(
            &mut s,
            &[
                InputAxis::AbsX(-10),
                InputAxis::AbsY(99999),
                InputAxis::Sync,
            ],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 0);
                assert_eq!(y.to_int(), 600);
            }
            _ => panic!("expected PointerMove"),
        }
    }

    #[test]
    fn nonzero_abs_min_maps_physical_offset_to_view_origin() {
        // Overscan: driver reports physical 0..200; view occupies [10, 110).
        let mut s = PointerState::new(100, 100, 10, 110, 10, 110);
        let events = drain(
            &mut s,
            &[InputAxis::AbsX(10), InputAxis::AbsY(110), InputAxis::Sync],
        );
        match &events[0] {
            InputEvent::PointerMove { x, y, .. } => {
                assert_eq!(x.to_int(), 0, "physical off_x maps to view x=0");
                assert_eq!(
                    y.to_int(),
                    100,
                    "physical off_y+height maps to view y=height"
                );
            }
            other => panic!("expected PointerMove, got {other:?}"),
        }
    }
}
