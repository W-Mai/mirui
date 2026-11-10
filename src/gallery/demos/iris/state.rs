use crate::anim::{
    BOUNCY, INTERACTIVE, MAX_FRAME_DT_MS, Motion, PlayMode, SMOOTH, Spring, Transition, Tween,
    TweenConfig, ease,
};
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
const FIRST_SHOT: u16 = 41;
/// Windows shorter than this drop the level meter and the focus section.
const ROOMY_HEIGHT: i32 = 600;
/// The blades snap shut, then reopen more slowly.
const SHUTTER_CLOSE: TweenConfig = TweenConfig::new(110, &ease::IN_QUAD);
const SHUTTER_OPEN: TweenConfig = TweenConfig::new(240, &ease::IN_OUT_CUBIC);
/// How long the saved notice stays down before it slides away.
const NOTICE_HOLD_MS: u16 = 2_700;
/// Offset that parks the saved notice above its stage, which clips it away.
pub(super) const NOTICE_PARKED: Fixed = Fixed::from_int(-48);

/// One step of the program mode, applied when the loop clock passes it.
#[derive(Clone, Copy)]
enum Cue {
    /// Move the aperture to a full stop.
    Stop(i32, TweenConfig),
    /// Pull focus to a distance in centimetres.
    Focus(i32, TweenConfig),
    /// Release the shutter.
    Shoot,
}

/// Stop down from f/2 to f/8 while focus pulls back, shoot, then open again.
const PROGRAMME: [(u16, Cue); 5] = [
    (
        700,
        Cue::Stop(5, TweenConfig::new(2_000, &ease::IN_OUT_CUBIC)),
    ),
    (
        2_200,
        Cue::Focus(300, TweenConfig::new(950, &ease::IN_OUT_CUBIC)),
    ),
    (3_400, Cue::Shoot),
    (
        4_100,
        Cue::Stop(1, TweenConfig::new(2_600, &ease::IN_OUT_CUBIC)),
    ),
    (
        5_400,
        Cue::Focus(70, TweenConfig::new(1_500, &ease::IN_OUT_CUBIC)),
    ),
];

/// Whether the loop clock passed `at` while moving from `before` to `now`.
fn passed(before: u16, now: u16, at: u16) -> bool {
    if now >= before {
        before < at && at <= now
    } else {
        before < at || at <= now
    }
}

/// The last cue of one kind at or before `now`, wrapping to the end of the loop.
fn latest(now: u16, kind: fn(&Cue) -> bool) -> Option<Cue> {
    let mut cues = PROGRAMME.iter().rev().filter(|(_, cue)| kind(cue));
    cues.clone()
        .find(|(at, _)| *at <= now)
        .or_else(|| cues.next())
        .map(|(_, cue)| *cue)
}

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
    /// Gallery use: program mode runs until the user takes over.
    Live,
    /// README capture: the programme loops exactly and the shot counter repeats.
    Capture,
}

/// The P / A / S / M dial. Program mode runs the aperture programme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExposureMode {
    Program,
    Aperture,
    Shutter,
    Manual,
}

impl ExposureMode {
    pub(super) const ALL: [Self; 4] = [Self::Program, Self::Aperture, Self::Shutter, Self::Manual];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Program => "P",
            Self::Aperture => "A",
            Self::Shutter => "S",
            Self::Manual => "M",
        }
    }
}

/// Single autofocus lets the programme pull focus; manual focus holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusMode {
    Single,
    Manual,
}

impl FocusMode {
    pub(super) const ALL: [Self; 2] = [Self::Single, Self::Manual];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Single => "AF-S",
            Self::Manual => "MF",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureMode {
    Photo,
    Video,
}

impl CaptureMode {
    pub(super) const ALL: [Self; 2] = [Self::Photo, Self::Video];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Photo => "Photo",
            Self::Video => "Video",
        }
    }
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

/// A motion already resting at `value`.
fn resting(value: Fixed) -> Motion {
    Spring::preset(value, value, SMOOTH).into()
}

#[crate::model]
pub(crate) struct IrisState {
    /// Continuous aperture position in stops from f/1.4.
    #[observe]
    pub(super) stop: Fixed,
    /// Focus distance in metres.
    #[observe]
    pub(super) focus: Fixed,
    /// How far the shutter pushes the blades shut, from 0 to 1.
    #[observe]
    pub(super) closure: Fixed,
    /// Vertical offset of the saved notice; [`NOTICE_PARKED`] hides it.
    #[observe]
    pub(super) notice: Fixed,
    #[observe]
    pub(super) shots: u16,
    #[observe]
    pub(super) layout: IrisLayout,
    /// The window is tall enough for the level meter and the focus section.
    #[observe]
    pub(super) roomy: bool,
    /// Ambient phase in degrees; it completes one turn per programme loop.
    #[observe]
    pub(super) phase: Fixed,
    #[observe]
    pub(super) exposure_mode: ExposureMode,
    #[observe]
    pub(super) focus_mode: FocusMode,
    #[observe]
    pub(super) capture_mode: CaptureMode,
    /// Loop clock of the programme; its value is the ambient phase.
    programme: Tween,
    aperture: Motion,
    focus_pull: Motion,
    shutter: Motion,
    closing: bool,
    notice_motion: Motion,
    notice_hold: u16,
    run: IrisRun,
}

impl IrisState {
    pub fn new(run: IrisRun, width: u16, height: u16) -> Self {
        let width = Fixed::from_int(i32::from(width));
        let height = Fixed::from_int(i32::from(height));
        Self {
            stop: Fixed::ONE,
            focus: Fixed::from_ratio(70, 100),
            closure: Fixed::ZERO,
            notice: NOTICE_PARKED,
            shots: FIRST_SHOT,
            layout: IrisLayout::classify(width, height),
            roomy: height >= Fixed::from_int(ROOMY_HEIGHT),
            phase: Fixed::ZERO,
            exposure_mode: ExposureMode::Program,
            focus_mode: FocusMode::Single,
            capture_mode: CaptureMode::Photo,
            programme: Tween::new(
                Fixed::ZERO,
                Fixed::from_int(360),
                LOOP_MS as u16,
                &ease::LINEAR,
                PlayMode::Loop,
            ),
            aperture: resting(Fixed::ONE),
            focus_pull: resting(Fixed::from_ratio(70, 100)),
            shutter: resting(Fixed::ZERO),
            closing: false,
            notice_motion: resting(NOTICE_PARKED),
            notice_hold: 0,
            run,
        }
    }

    fn apply(&mut self, cue: Cue, transition: Option<Transition>) {
        match cue {
            Cue::Stop(stop, config) => transition
                .unwrap_or(config.into())
                .retarget(&mut self.aperture, Fixed::from_int(stop)),
            Cue::Focus(centimetres, config) if self.focus_mode == FocusMode::Single => transition
                .unwrap_or(config.into())
                .retarget(&mut self.focus_pull, Fixed::from_ratio(centimetres, 100)),
            Cue::Focus(..) => {}
            Cue::Shoot => self.release(),
        }
    }

    fn release(&mut self) {
        if self.closing || !self.shutter.is_done() {
            return;
        }
        self.shutter = Tween::preset(self.closure, Fixed::ONE, SHUTTER_CLOSE).into();
        self.closing = true;
    }

    /// Hand the aperture and focus back to the programme at its current place.
    fn resume(&mut self) {
        let now = self.programme.elapsed_ms;
        let kinds: [fn(&Cue) -> bool; 2] = [
            |cue| matches!(cue, Cue::Stop(..)),
            |cue| matches!(cue, Cue::Focus(..)),
        ];
        for kind in kinds {
            if let Some(cue) = latest(now, kind) {
                self.apply(cue, Some(SMOOTH.into()));
            }
        }
    }
}

#[crate::model]
impl IrisState {
    /// Blade edge distance with the shutter closure applied.
    #[observe]
    pub fn edge(&self) -> Fixed {
        opening(self.stop) * (Fixed::ONE - self.closure)
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

    /// Program mode drives the aperture; every other mode leaves it to the user.
    #[observe]
    pub fn scripted(&self) -> bool {
        self.exposure_mode == ExposureMode::Program
    }

    pub fn set_viewport(&mut self, width: Fixed, height: Fixed) {
        self.layout = IrisLayout::classify(width, height);
        self.roomy = height >= Fixed::from_int(ROOMY_HEIGHT);
    }

    /// Follow the aperture slider. Turning it in program mode switches to
    /// aperture priority, as on a camera dial.
    pub fn set_stop(&mut self, value: Fixed) {
        if self.exposure_mode == ExposureMode::Program {
            self.exposure_mode = ExposureMode::Aperture;
        }
        let target = value.max(Fixed::ZERO).min(Fixed::from_int(MAX_STOP));
        Transition::from(INTERACTIVE).retarget(&mut self.aperture, target);
    }

    pub fn select_exposure(&mut self, mode: ExposureMode) {
        let resumed = mode == ExposureMode::Program && self.exposure_mode != mode;
        self.exposure_mode = mode;
        if resumed {
            self.resume();
        }
    }

    pub fn select_focus(&mut self, mode: FocusMode) {
        self.focus_mode = mode;
    }

    pub fn select_capture(&mut self, mode: CaptureMode) {
        self.capture_mode = mode;
    }

    pub fn capture(&mut self) {
        self.release();
    }

    pub fn advance(&mut self, delta_ms: u16) {
        if delta_ms == 0 {
            return;
        }
        let dt = delta_ms.min(MAX_FRAME_DT_MS);
        let before = self.programme.elapsed_ms;
        self.programme.tick(dt);
        let now = self.programme.elapsed_ms;
        self.phase = self.programme.value();
        if self.exposure_mode == ExposureMode::Program {
            for (at, cue) in PROGRAMME {
                if passed(before, now, at) {
                    self.apply(cue, None);
                }
            }
            if self.run == IrisRun::Capture && now < before {
                self.shots = FIRST_SHOT;
            }
        }

        self.aperture.tick(dt);
        self.stop = self.aperture.value();
        self.focus_pull.tick(dt);
        self.focus = self.focus_pull.value();

        self.shutter.tick(dt);
        self.closure = self.shutter.value();
        if self.closing && self.shutter.is_done() {
            // Blades fully shut: the frame is exposed and the notice drops in.
            self.closing = false;
            self.shots = self.shots.wrapping_add(1);
            self.shutter = Tween::preset(Fixed::ONE, Fixed::ZERO, SHUTTER_OPEN).into();
            Transition::from(BOUNCY).retarget(&mut self.notice_motion, Fixed::ZERO);
            self.notice_hold = NOTICE_HOLD_MS;
        }
        if self.notice_hold > 0 {
            self.notice_hold = self.notice_hold.saturating_sub(dt);
            if self.notice_hold == 0 {
                Transition::from(SMOOTH).retarget(&mut self.notice_motion, NOTICE_PARKED);
            }
        }
        self.notice_motion.tick(dt);
        self.notice = self.notice_motion.value();
    }
}
