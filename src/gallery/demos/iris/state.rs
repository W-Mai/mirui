use crate::prelude::Fixed;

/// One-third stops shown by the readouts, from f/1.4 to f/16.
const THIRD_STOPS: [&str; 22] = [
    "1.4", "1.6", "1.8", "2", "2.2", "2.5", "2.8", "3.2", "3.5", "4", "4.5", "5", "5.6", "6.3",
    "7.1", "8", "9", "10", "11", "13", "14", "16",
];
/// Readout label for a third stop, without the leading `F`.
pub(super) fn third_label(third: u8) -> &'static str {
    THIRD_STOPS[usize::from(third).min(THIRD_STOPS.len() - 1)]
}

/// f-numbers of the full stops, `sqrt(2)^(n + 1)`, in 1/1000 units.
const F_MILLI: [i32; 8] = [1414, 2000, 2828, 4000, 5657, 8000, 11314, 16000];
pub(super) const MAX_STOP: i32 = 7;
/// Edge distance of the blades at f/1.4, as a fraction of the lens radius.
pub(super) const OPEN_EDGE: Fixed = Fixed::from_ratio(70, 100);
/// The stop that meters as a correct exposure for the simulated scene.
const METERED_STOP: i32 = 3;

pub(super) use super::LOOP_MS;
const CAPTURE_AT_MS: u32 = 3_400;
const SHUTTER_CLOSE_MS: u32 = 110;
const SHUTTER_HOLD_MS: u32 = 60;
const SHUTTER_OPEN_MS: u32 = 240;
const TOAST_IN_MS: u32 = 420;
const TOAST_HOLD_MS: u32 = 2_700;
const TOAST_OUT_MS: u32 = 380;
const FIRST_SHOT: u16 = 41;

/// Showcase aperture path: stop down from f/2 to f/8, capture, open again.
const STOP_KEYS: [(u32, i32); 6] = [
    (0, 1),
    (700, 1),
    (2_700, 5),
    (4_100, 5),
    (6_700, 1),
    (LOOP_MS, 1),
];
/// Focus pull in centimetres, paired with the aperture path.
const FOCUS_KEYS: [(u32, i32); 6] = [
    (0, 70),
    (2_200, 70),
    (3_150, 300),
    (5_400, 300),
    (6_900, 70),
    (LOOP_MS, 70),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrisLayout {
    /// 128 x 160 class panels: lens plus one line of exposure.
    Micro,
    /// Round or square watch faces: centred lens, readouts on the bezel.
    Dial,
    /// Phones held upright: lens over a camera control stack.
    Portrait,
    /// Landscape panels and desktop windows: lens stage plus an inspector.
    Landscape,
}

impl IrisLayout {
    pub fn classify(width: Fixed, height: Fixed) -> Self {
        let short = width.min(height);
        let long = width.max(height);
        if long * 10 <= short * 12 && long <= Fixed::from_int(480) {
            Self::Dial
        } else if short < Fixed::from_int(200) {
            Self::Micro
        } else if height * 10 > width * 13 {
            Self::Portrait
        } else {
            Self::Landscape
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrisRun {
    /// Gallery use: the showcase path runs until the user takes over.
    Live,
    /// README capture: the path loops exactly and the shot counter repeats.
    Capture,
}

fn smoothstep(t: Fixed) -> Fixed {
    let t = t.max(Fixed::ZERO).min(Fixed::ONE);
    t * t * (Fixed::from_int(3) - t * 2)
}

fn keyed(keys: &[(u32, i32)], ms: u32, unit: i32) -> Fixed {
    let ms = ms % LOOP_MS;
    for pair in keys.windows(2) {
        let (t0, v0) = pair[0];
        let (t1, v1) = pair[1];
        if ms >= t0 && ms <= t1 {
            let t = Fixed::from_ratio((ms - t0) as i32, (t1 - t0).max(1) as i32);
            let from = Fixed::from_ratio(v0, unit);
            let to = Fixed::from_ratio(v1, unit);
            return from + (to - from) * smoothstep(t);
        }
    }
    Fixed::from_ratio(keys[0].1, unit)
}

/// f-number for a continuous stop position, interpolated between full stops.
pub(super) fn f_number(stop: Fixed) -> Fixed {
    let stop = stop.max(Fixed::ZERO).min(Fixed::from_int(MAX_STOP));
    let index = stop.floor().to_int().clamp(0, MAX_STOP - 1) as usize;
    let t = stop - Fixed::from_int(index as i32);
    let from = Fixed::from_ratio(F_MILLI[index], 1000);
    let to = Fixed::from_ratio(F_MILLI[index + 1], 1000);
    from + (to - from) * t
}

/// Blade edge distance from the optical axis for a stop, as a fraction of the
/// lens radius. The open diameter is inversely proportional to the f-number.
pub(super) fn opening(stop: Fixed) -> Fixed {
    OPEN_EDGE * Fixed::from_ratio(F_MILLI[0], 1000) / f_number(stop)
}

fn shutter_closure(elapsed: u32) -> Fixed {
    if elapsed < SHUTTER_CLOSE_MS {
        let t = Fixed::from_ratio(elapsed as i32, SHUTTER_CLOSE_MS as i32);
        t * t
    } else if elapsed < SHUTTER_CLOSE_MS + SHUTTER_HOLD_MS {
        Fixed::ONE
    } else if elapsed < SHUTTER_CLOSE_MS + SHUTTER_HOLD_MS + SHUTTER_OPEN_MS {
        let t = Fixed::from_ratio(
            (elapsed - SHUTTER_CLOSE_MS - SHUTTER_HOLD_MS) as i32,
            SHUTTER_OPEN_MS as i32,
        );
        Fixed::ONE - smoothstep(t)
    } else {
        Fixed::ZERO
    }
}

fn toast_alpha(elapsed: u32) -> u8 {
    let start = SHUTTER_CLOSE_MS + SHUTTER_HOLD_MS + 120;
    let level = if elapsed < start {
        Fixed::ZERO
    } else if elapsed < start + TOAST_IN_MS {
        smoothstep(Fixed::from_ratio(
            (elapsed - start) as i32,
            TOAST_IN_MS as i32,
        ))
    } else if elapsed < start + TOAST_IN_MS + TOAST_HOLD_MS {
        Fixed::ONE
    } else if elapsed < start + TOAST_IN_MS + TOAST_HOLD_MS + TOAST_OUT_MS {
        Fixed::ONE
            - smoothstep(Fixed::from_ratio(
                (elapsed - start - TOAST_IN_MS - TOAST_HOLD_MS) as i32,
                TOAST_OUT_MS as i32,
            ))
    } else {
        Fixed::ZERO
    };
    // Keep one level of coverage so captured frames retain a stable draw list.
    (level * 254).to_int().clamp(0, 254) as u8 + 1
}

#[crate::model]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IrisState {
    stop: Fixed,
    target: Fixed,
    focus: Fixed,
    focus_target: Fixed,
    closure: Fixed,
    since_capture: u32,
    shots: u16,
    clock: u32,
    scripted: bool,
    run: IrisRun,
    layout: IrisLayout,
}

impl IrisState {
    pub fn new(run: IrisRun, width: u16, height: u16) -> Self {
        Self {
            stop: Fixed::ONE,
            target: Fixed::ONE,
            focus: Fixed::from_ratio(70, 100),
            focus_target: Fixed::from_ratio(70, 100),
            closure: Fixed::ZERO,
            since_capture: u32::MAX,
            shots: FIRST_SHOT,
            clock: 0,
            scripted: true,
            run,
            layout: IrisLayout::classify(
                Fixed::from_int(i32::from(width)),
                Fixed::from_int(i32::from(height)),
            ),
        }
    }
}

#[crate::model]
impl IrisState {
    /// Continuous aperture position in stops from f/1.4.
    #[observe]
    pub fn stop(&self) -> Fixed {
        self.stop
    }

    /// Blade edge distance with the shutter closure applied.
    #[observe]
    pub fn edge(&self) -> Fixed {
        opening(self.stop) * (Fixed::ONE - self.closure)
    }

    #[observe]
    pub fn closure(&self) -> Fixed {
        self.closure
    }

    /// Nearest third stop, counted from f/1.4.
    #[observe]
    pub fn aperture_third(&self) -> u8 {
        (self.stop * 3).round().to_int().clamp(0, MAX_STOP * 3) as u8
    }

    /// Exposure against the metered scene, in stops; positive is brighter.
    #[observe]
    pub fn exposure(&self) -> Fixed {
        (Fixed::from_int(METERED_STOP) - self.stop)
            .max(Fixed::from_int(-3))
            .min(Fixed::from_int(3))
    }

    /// Focus distance in metres.
    #[observe]
    pub fn focus(&self) -> Fixed {
        self.focus
    }

    #[observe]
    pub fn shots(&self) -> u16 {
        self.shots
    }

    #[observe]
    pub fn toast(&self) -> u8 {
        toast_alpha(self.since_capture)
    }

    #[observe]
    pub fn scripted(&self) -> bool {
        self.scripted
    }

    #[observe]
    pub fn layout(&self) -> IrisLayout {
        self.layout
    }

    /// Ambient phase in degrees; it completes one turn per showcase loop.
    #[observe]
    pub fn phase(&self) -> Fixed {
        Fixed::from_ratio((self.clock % LOOP_MS) as i32 * 9, (LOOP_MS / 40) as i32)
    }

    pub fn set_viewport(&mut self, width: Fixed, height: Fixed) {
        let layout = IrisLayout::classify(width, height);
        if self.layout != layout {
            self.layout = layout;
        }
    }

    pub fn set_stop(&mut self, value: Fixed) {
        self.scripted = false;
        self.target = value.max(Fixed::ZERO).min(Fixed::from_int(MAX_STOP));
    }

    pub fn toggle_auto(&mut self) {
        self.scripted = !self.scripted;
    }

    pub fn capture(&mut self) {
        let busy = SHUTTER_CLOSE_MS + SHUTTER_HOLD_MS + SHUTTER_OPEN_MS;
        if self.since_capture < busy {
            return;
        }
        self.since_capture = 0;
    }

    pub fn advance(&mut self, delta_ms: u16) {
        if delta_ms == 0 {
            return;
        }
        let dt = u32::from(delta_ms.min(100));
        let before = self.clock % LOOP_MS;
        self.clock = self.clock.wrapping_add(dt);
        let now = self.clock % LOOP_MS;

        if self.scripted {
            self.target = keyed(&STOP_KEYS, now, 1);
            self.focus_target = keyed(&FOCUS_KEYS, now, 100);
            let crossed = if now >= before {
                before < CAPTURE_AT_MS && now >= CAPTURE_AT_MS
            } else {
                before < CAPTURE_AT_MS || now >= CAPTURE_AT_MS
            };
            if crossed {
                self.since_capture = 0;
            }
            if self.run == IrisRun::Capture && now < before {
                self.shots = FIRST_SHOT;
            }
        }

        let ease = Fixed::from_ratio(dt as i32, (dt + 70) as i32);
        self.stop += (self.target - self.stop) * ease;
        self.focus += (self.focus_target - self.focus) * ease;

        let previous = self.since_capture;
        self.since_capture = self.since_capture.saturating_add(dt);
        let swap = SHUTTER_CLOSE_MS + SHUTTER_HOLD_MS / 2;
        if previous < swap && self.since_capture >= swap {
            self.shots = self.shots.wrapping_add(1);
        }
        self.closure = if self.since_capture == u32::MAX {
            Fixed::ZERO
        } else {
            shutter_closure(self.since_capture)
        };
    }
}
