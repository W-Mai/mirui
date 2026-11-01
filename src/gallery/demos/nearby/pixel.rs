use super::style::*;
#[component]
pub(super) struct ActorSprite {
    pub state: model!(DisplayState),
    pub actions: model!(super::launcher::Launcher),
}
#[view(component = ActorSprite, priority = 60)]
pub(super) fn actor_sprite(
    state: model!(component.state, watch(actor_motion)),
    actions: model!(
        component.actions,
        watch(active, menu, menu_selection, selection_age, card_motion)
    ),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let frame = if actions.active {
        let frame = actions.card_motion();
        cursor_frame(
            frame,
            Some(if actions.menu() {
                actions.menu_selection
            } else {
                frame.selected.unwrap_or(0)
            }),
            actions.selection_age,
        )
    } else {
        cursor_frame(state.actor_motion(), None, 800)
    };
    let session = matches!(frame.page, Page::Host | Page::Joined | Page::Leaving);
    let celebrate = session
        && (frame.ready || frame.connected)
        && matches!(
            frame.phase,
            Phase::HostReady | Phase::WaitingStart | Phase::NetworkConnected
        );
    let hop = if celebrate {
        motion::hop(
            frame.phase_age.saturating_sub(timing::HOST_HOP_DELAY_MS),
            7.0,
        )
    } else if session {
        0
    } else {
        motion::hop(frame.selection, 4.0)
    };
    let mut painter = Painter {
        theme: &theme,
        emit: |command: DrawCommand<'_>| draw_in_view(paint, command),
        width: rect.w.to_int(),
        height: rect.h.to_int(),
        unit: 1,
    };
    painter.person(
        rect.w.to_int() / 2,
        rect.h.to_int() - 38 + hop,
        false,
        if session && frame.host_walking {
            frame.phase_age
        } else if frame.walking {
            frame.selection
        } else {
            0
        },
        if session {
            matches!(frame.phase, Phase::Leaving | Phase::Closed)
        } else {
            frame.selected == Some(0)
        },
    );
}

pub(super) fn cursor_frame(mut frame: Frame, selected: Option<usize>, age: u16) -> Frame {
    if let Some(selected) = selected {
        frame.selected = Some(selected);
        frame.selection = age;
        frame.walking = frame.selection < timing::TRAVEL.duration_ms;
    }
    frame
}
#[component]
pub(super) struct ActorAnchor(pub Stage);
#[derive(Clone, Copy)]
pub(super) enum Stage {
    Walker,
    Search,
    Session,
}

#[compose]
pub(super) fn art(state: model!(DisplayState), kind: Stage, height: i32) -> Entity {
    let scenery = if matches!(kind, Stage::Session | Stage::Search) {
        Scenery::Landscape
    } else {
        Scenery::Map
    };
    ui! {
        View (
            height: height,
            min_height: 0,
            shrink: 1.0,
            width: Dimension::percent(100),
            clip_children: true
        ) [
            scenery,
            ActorAnchor(kind),
        ] {
            match kind {
                Stage :: Walker => {
                    View (grow: 1.0, width: Dimension::percent(100)) [
                        WalkerSprite {
                            state: state.clone(),
                        },
                    ]
                }
                Stage :: Search => {
                    View (grow: 1.0, width: Dimension::percent(100)) [
                        SearchSprite {
                            state: state.clone(),
                        },
                    ]
                }
                Stage :: Session => {
                    View (grow: 1.0, width: Dimension::percent(100)) [
                        SessionSprite {
                            state: state.clone(),
                        },
                    ]
                }
            }
        }
    }
}
#[cfg(not(feature = "std"))]
trait FloatOps {
    fn round(self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn powi(self, exponent: i32) -> Self;
    fn mul_add(self, multiplier: Self, addend: Self) -> Self;
}
#[cfg(not(feature = "std"))]
impl FloatOps for f32 {
    fn round(self) -> Self {
        libm::roundf(self)
    }
    fn sin(self) -> Self {
        libm::sinf(self)
    }
    fn cos(self) -> Self {
        libm::cosf(self)
    }
    fn powi(self, exponent: i32) -> Self {
        libm::powf(self, exponent as f32)
    }
    fn mul_add(self, multiplier: Self, addend: Self) -> Self {
        libm::fmaf(self, multiplier, addend)
    }
}
use super::data::{Page, Phase};
use super::{
    model::DisplayState,
    motion::{self, Frame, anchors, timing},
    palette::actor,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Choice {
        index: usize,
        mode: bool,
    },
    Walker,
    Session,
    Signal {
        index: usize,
        strength: u8,
        available: bool,
    },
    Search,
}

#[component]
pub(super) struct ChoiceSprite {
    pub state: model!(DisplayState),
    pub index: usize,
    pub mode: bool,
}

#[component]
pub(super) struct WalkerSprite {
    pub state: model!(DisplayState),
}

#[component]
pub(super) struct SessionSprite {
    pub state: model!(DisplayState),
}

#[component]
pub(super) struct SignalSprite {
    pub state: model!(DisplayState),
    pub index: usize,
    pub strength: u8,
    pub available: bool,
}

#[component]
pub(super) struct SearchSprite {
    pub state: model!(DisplayState),
}

const ACTOR_SIDE_CLEARANCE: i32 = 48;
fn stage_position(width: i32, position: Fixed) -> i32 {
    ACTOR_SIDE_CLEARANCE + (Fixed::from_int(width - ACTOR_SIDE_CLEARANCE * 2) * position).to_int()
}

impl<F: FnMut(DrawCommand<'_>)> Painter<'_, F> {
    fn person(&mut self, x: i32, y: i32, guest: bool, step: u16, facing_left: bool) {
        // Rectangles below are local pixel-art data; stage placement is layout-derived.
        let color = if guest { actor::GUEST } else { actor::HOST };
        let light = if guest {
            actor::GUEST_LIGHT
        } else {
            actor::HOST_LIGHT
        };
        let boots = actor::BOOTS;
        let f = step / timing::WALK_FRAME_MS % 4;
        self.fill(x - 13, y + 34, 28, 3, DIVIDER);
        for (dx, dy, w, h) in [
            (-8, 0, 16, 6),
            (-12, 6, 24, 4),
            (-8, 19, 16, 10),
            (-12, 20 + i32::from(f % 2) * 2, 4, 8),
            (8, 20 - i32::from(f % 2) * 2, 4, 8),
        ] {
            self.fill(x + dx, y + dy, w, h, color);
        }
        self.fill(x - 7, y, 14, 2, light);
        self.fill(x - 8, y + 19, 3, 10, light);
        self.fill(x - 8, y + 10, 16, 9, actor::SKIN);
        self.fill(x + if facing_left { -6 } else { 3 }, y + 12, 3, 3, boots);
        self.fill(x - 8 + if f == 1 { 3 } else { 0 }, y + 29, 6, 5, boots);
        self.fill(x + 2 - if f == 3 { 3 } else { 0 }, y + 29, 6, 5, boots);
        if guest {
            self.fill(
                x + if facing_left { 8 } else { -12 },
                y + 4,
                4,
                8,
                actor::PACK,
            );
            self.fill(
                x + if facing_left { 12 } else { -16 },
                y + 6,
                4,
                4,
                actor::PACK,
            );
        }
    }
    fn flag(&mut self, x: i32, y: i32, age: u16) {
        let h = (56.0 * timing::progress(age, timing::FLAG_RISE_MS)).round() as i32;
        self.fill(x, y - h, 3, h, FOREGROUND);
        if h > 22 {
            self.fill(x + 3, y - h, 29, 17, ACCENT);
            self.fill(x + 27, y - h + 13, 8, 4, PANEL);
        }
        self.fill(x - 8, y, 19, 4, DIVIDER);
    }
    fn beacon(&mut self, x: i32, y: i32, wave: Option<u16>) {
        self.fill(x - 10, y - 12, 20, 12, FOREGROUND);
        self.fill(x - 4, y - 26, 8, 14, ACCENT);
        self.fill(x - 2, y - 30, 4, 4, WARNING);
        if let Some(wave) = wave {
            let reach = (y - 28).min(x - 2).min(self.width - x - 2).clamp(0, 48);
            for index in 0..timing::WAVE_COUNT {
                let p = timing::progress(
                    (wave + index * (timing::WAVE_MS / timing::WAVE_COUNT)) % timing::WAVE_MS,
                    timing::WAVE_MS,
                );
                let r = 12 + (p * (reach - 12).max(0) as f32).round() as i32;
                if reach < 12 {
                    continue;
                }
                let color = self.alpha(ACCENT, ((1.0 - p) * 150.0) as u8);
                for (dx, dy, w, h) in [
                    (-r, -22 - r, 4, 12),
                    (r - 4, -22 - r, 4, 12),
                    (-r + 4, -26 - r, 12, 4),
                    (r - 16, -26 - r, 12, 4),
                ] {
                    self.fill(x + dx, y + dy, w, h, color);
                }
            }
        }
    }
    fn dust(&mut self, x: i32, y: i32, age: u16) {
        if !(timing::HOP_MS..timing::HOP_MS + timing::DUST_MS).contains(&age) {
            return;
        }
        let p = timing::progress(age - timing::HOP_MS, timing::DUST_MS);
        for i in 0..6 {
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let dx = (side * (8.0 + (i + 1) as f32 * 6.0 * p)).round() as i32;
            let dy = -(p * core::f32::consts::PI)
                .sin()
                .mul_add((3 + i * 2) as f32, 0.0)
                .round() as i32;
            self.fill(
                x + dx,
                y + dy,
                3,
                3,
                self.alpha(SECONDARY, ((1.0 - p) * 180.0) as u8),
            );
        }
    }
    fn stars(&mut self, x: i32, y: i32, age: u16) {
        if age >= timing::STARS_MS {
            return;
        }
        let p = timing::progress(age, timing::STARS_MS);
        for i in 0..8 {
            let a = i as f32 * core::f32::consts::FRAC_PI_4;
            let reach = (x - 4)
                .min(self.width - x - 6)
                .min(y - 2)
                .min(self.height - y - 6)
                .clamp(0, 26) as f32;
            let r = reach * (0.35 + 0.65 * (1.0 - (1.0 - p).powi(3)));
            self.fill(
                x + (a.cos() * r).round() as i32,
                y + (a.sin() * r).round() as i32,
                4,
                4,
                self.alpha(ACCENT, ((1.0 - p) * 255.0) as u8),
            );
        }
    }
    fn session(&mut self, w: i32, h: i32, frame: Frame) {
        let floor = h - 12;
        let position = |value: Fixed| stage_position(w, value);
        let host = position(frame.host);
        let guest = position(frame.guest);
        let ready = if frame.network_only {
            frame.connected
        } else {
            frame.ready
        };
        let celebrate = ready
            && frame.phase_age < timing::SETTLED_MS
            && matches!(
                frame.phase,
                Phase::HostReady | Phase::WaitingStart | Phase::NetworkConnected
            );
        self.fill(16, floor, w - 32, 2, DIVIDER);
        for x in (24..w - 20).step_by(22) {
            self.fill(x, floor + 4, 8, 2, DIVIDER);
        }
        if w > 250 && h >= 84 {
            self.flag(
                w / 7,
                floor,
                if frame.phase == Phase::Creating {
                    frame.phase_age
                } else {
                    timing::SETTLED_MS
                },
            );
        }
        let shake = if matches!(frame.phase, Phase::Failed | Phase::RestoreFailed) {
            motion::recoil(frame.phase_age)
        } else {
            0
        };
        if h >= 116 {
            self.beacon(w / 2 + shake, floor - 50, frame.wave);
        }
        if !frame.external_actor {
            self.person(
                host,
                floor - 34
                    + if celebrate {
                        motion::hop(
                            frame.phase_age.saturating_sub(timing::HOST_HOP_DELAY_MS),
                            7.0,
                        )
                    } else {
                        0
                    },
                false,
                if frame.host_walking {
                    frame.phase_age
                } else {
                    0
                },
                matches!(frame.phase, Phase::Leaving | Phase::Closed),
            );
        }
        if frame.visitor {
            self.person(
                guest,
                floor - 34
                    + if celebrate {
                        motion::hop(frame.phase_age, 11.0)
                    } else {
                        0
                    },
                true,
                if frame.guest_walking {
                    frame.phase_age
                } else {
                    0
                },
                frame.phase != Phase::Starting,
            );
        }
        self.fill(
            position(anchors::HOST_HOME) - 20,
            floor + 7,
            40,
            4,
            ACCENT_DIM,
        );
        self.fill(
            position(anchors::GUEST_HOME) - 20,
            floor + 7,
            40,
            4,
            if ready { ACCENT } else { DIVIDER },
        );
        if w >= 240
            && matches!(
                frame.phase,
                Phase::ApprovalPending | Phase::ApprovalPendingClient
            )
        {
            let gate = position(anchors::GATE) - 16;
            self.fill(gate, floor - 42, 4, 40, ACCENT);
            self.fill(gate - 10, floor - 42, 24, 4, ACCENT);
        }
        if w >= 240 && matches!(frame.phase, Phase::Checking | Phase::Notifying) {
            self.fill(w / 2 - 12, floor - 40, 24, 2, ACCENT);
            self.fill(w / 2 - 12, floor - 18, 24, 2, ACCENT);
            self.fill(w / 2 - 12, floor - 40, 2, 24, ACCENT);
            self.fill(w / 2 + 10, floor - 40, 2, 24, ACCENT);
            let bars = if frame.phase == Phase::Notifying {
                3
            } else {
                1
            };
            for i in 0..bars {
                self.fill(w / 2 - 6 + i * 5, floor - 32, 3, 8, ACCENT);
            }
        }
        if celebrate {
            self.stars(guest, floor - 30, frame.phase_age);
            self.dust(guest, floor, frame.phase_age);
        }
    }
}
#[view(component = ChoiceSprite, priority = 60)]
pub(super) fn choice_sprite(
    component: component!(),
    state: model!(component.state, watch(choice_motion)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    draw(
        Kind::Choice {
            index: component.index,
            mode: component.mode,
        },
        rect.w.to_int(),
        rect.h.to_int(),
        state.choice_motion(),
        &theme,
        |command| {
            draw_in_view(paint, command);
        },
    );
}

#[view(component = WalkerSprite, priority = 60)]
pub(super) fn walker_sprite(
    state: model!(component.state, watch(walker_motion)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let mut frame = state.walker_motion();
    frame.external_actor = true;
    let rect = paint.rect();
    draw(
        Kind::Walker,
        rect.w.to_int(),
        rect.h.to_int(),
        frame,
        &theme,
        |command| {
            draw_in_view(paint, command);
        },
    );
}

#[view(component = SessionSprite, priority = 60)]
pub(super) fn session_sprite(
    state: model!(component.state, watch(session_motion)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let frame = state.session_motion();
    draw(
        Kind::Session,
        rect.w.to_int(),
        rect.h.to_int(),
        frame,
        &theme,
        |command| {
            draw_in_view(paint, command);
        },
    );
}

#[view(component = SignalSprite, priority = 60)]
pub(super) fn signal_sprite(
    component: component!(),
    state: model!(component.state, watch(signal_motion)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    draw(
        Kind::Signal {
            index: component.index,
            strength: component.strength,
            available: component.available,
        },
        rect.w.to_int(),
        rect.h.to_int(),
        state.signal_motion(),
        &theme,
        |command| {
            draw_in_view(paint, command);
        },
    );
}

#[view(component = SearchSprite, priority = 60)]
pub(super) fn search_sprite(
    state: model!(component.state, watch(search_motion)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    draw(
        Kind::Search,
        rect.w.to_int(),
        rect.h.to_int(),
        state.search_motion(),
        &theme,
        |command| {
            draw_in_view(paint, command);
        },
    );
}

fn draw(
    kind: Kind,
    width: i32,
    height: i32,
    frame: Frame,
    theme: &Theme,
    emit: impl FnMut(DrawCommand<'_>),
) {
    if width <= 0 || height <= 0 {
        return;
    }
    // Each stage includes the full jump, shadow and particle envelope.
    let (min_width, min_height) = match kind {
        Kind::Choice { .. } => (112, 80),
        Kind::Walker => (112, 38),
        Kind::Session => (160, 60),
        Kind::Search => (112, 42),
        Kind::Signal { .. } => (30, 26),
    };
    let unit = ((min_width + width - 1) / width)
        .max((min_height + height - 1) / height)
        .max(1);
    let (w, h) = (width * unit, height * unit);
    let mut p = Painter {
        theme,
        emit,
        width: w,
        height: h,
        unit,
    };
    match kind {
        Kind::Choice { index, mode } => {
            let age = if frame.selected == Some(index) {
                frame.selection
            } else {
                timing::SETTLED_MS
            };
            let jump = motion::hop(age, 10.0);
            let y = h - 10;
            p.fill(8, y, w - 16, 2, DIVIDER);
            if mode && index == 1 {
                p.fill(w / 2 - 26, y - 45 + jump, 52, 30, FOREGROUND);
                p.fill(w / 2 - 20, y - 39 + jump, 40, 18, BACKGROUND);
                p.fill(w / 2 - 16, y - 14 + jump, 4, 10, ACCENT);
                p.fill(w / 2 - 19, y - 11 + jump, 10, 4, ACCENT);
                p.fill(w / 2 + 12, y - 11 + jump, 5, 5, ACCENT);
            } else if index == 0 && !mode {
                p.tent(w / 2 - 12, y + jump, 2);
                p.flag(w / 2 + 20, y + jump, timing::SETTLED_MS)
            } else {
                p.beacon(
                    w / 2,
                    y - 12 + jump,
                    (age < timing::CHOICE_WAVE_MS).then_some(age),
                );
            }
            p.dust(w / 2, y, age);
        }
        Kind::Walker => {
            let x = stage_position(w, frame.walker);
            if !frame.external_actor {
                p.person(
                    x,
                    h - 38
                        + if frame.selection >= timing::TRAVEL.duration_ms {
                            motion::hop(
                                frame.selection - timing::TRAVEL.duration_ms,
                                (h - 38).min(4) as f32,
                            )
                        } else {
                            0
                        },
                    false,
                    if frame.walking { frame.selection } else { 0 },
                    frame.selected == Some(0),
                );
            }
            p.dust(
                x,
                h - 4,
                frame.selection.saturating_sub(timing::TRAVEL.duration_ms),
            );
        }
        Kind::Session => {
            p.session(w, h, frame);
        }
        Kind::Search => p.beacon(w / 2, h - 8, frame.wave),
        Kind::Signal {
            index,
            strength,
            available,
        } => {
            for i in 0..4 {
                let age = if let Some(wave) = frame.wave {
                    wave.saturating_sub(i * timing::SIGNAL_STAGGER_MS)
                } else if frame.selected == Some(index) {
                    frame
                        .selection
                        .saturating_sub(i * timing::SIGNAL_STAGGER_MS)
                } else {
                    timing::SETTLED_MS
                };
                let hbar = 6 + i32::from(i) * 4;
                p.fill(
                    i32::from(i) * 7 + 2,
                    h - hbar - 2 + motion::hop(age, 3.0),
                    4,
                    hbar,
                    if available && u16::from(strength) > i * 25 {
                        ACCENT
                    } else {
                        DIVIDER
                    },
                );
            }
        }
    }
}
use crate::prelude::*;
use crate::render::{command::DrawCommand, raster::FillRule, scene::Paint};
use crate::types::Transform;
use crate::ui::theme::{Theme, ThemedColor};
use crate::ui::view::PaintCtx;

static TREE_CANOPY: Path = path!(
    M 4 0 H 6 V 2 H 7 V 4 H 8 V 6 H 9 V 8 H 10 V 10
    H 0 V 8 H 1 V 6 H 2 V 4 H 3 V 2 H 4 Z
);
static TENT_SHELL: Path = path!(
    M 7 0 H 9 V 1 H 10 V 2 H 11 V 3 H 12 V 4
    H 13 V 5 H 14 V 6 H 15 V 7 H 16 V 8 H 0
    V 7 H 1 V 6 H 2 V 5 H 3 V 4 H 4
    V 3 H 5 V 2 H 6 V 1 H 7 Z
);
static TENT_LIGHT: Path = path!(
    M 5 0 H 6 V 6 H 0 V 5 H 1 V 4 H 2 V 3 H 3 V 2 H 4 V 1 H 5 Z
);

#[derive(Clone, Copy, Debug)]
#[component]
pub(super) enum Scenery {
    Logo,
    Header,
    Map,
    Margins,
    Landscape,
    Ridge,
}

struct Painter<'a, F> {
    theme: &'a Theme,
    emit: F,
    width: i32,
    height: i32,
    unit: i32,
}

impl<F: FnMut(DrawCommand<'_>)> Painter<'_, F> {
    fn contour(
        &mut self,
        path: &Path,
        x: i32,
        y: i32,
        scale: i32,
        color: impl Into<ThemedColor>,
    ) -> bool {
        let color = color.into().resolve(self.theme);
        // Preserve Fill's whole-pixel coverage and translucent blending on reduced grids.
        if scale % self.unit != 0 || color.a != 255 {
            return false;
        }
        let scale = Fixed::from_int(scale / self.unit);
        (self.emit)(DrawCommand::FillPath {
            path,
            transform: Transform::translate(
                Fixed::from_int(x / self.unit),
                Fixed::from_int(y / self.unit),
            )
            .compose(&Transform::scale(scale, scale)),
            paint: &Paint::Color(color.into()),
            opa: 255,
            fill_rule: FillRule::NonZero,
        });
        true
    }

    fn tree(&mut self, x: i32, y: i32, scale: i32, color: impl Into<ThemedColor> + Copy) {
        self.fill(x - scale, y - 3 * scale, 2 * scale, 3 * scale, DEEP);
        if !self.contour(&TREE_CANOPY, x - 5 * scale, y - 12 * scale, scale, color) {
            for row in 0..5 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (12 - row * 2) * scale,
                    (row * 2 + 2) * scale,
                    2 * scale,
                    color,
                );
            }
        }
    }
    fn tent(&mut self, x: i32, y: i32, scale: i32) {
        if !self.contour(&TENT_SHELL, x - 8 * scale, y - 8 * scale, scale, CLAY) {
            for row in 0..8 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (8 - row) * scale,
                    (row * 2 + 2) * scale,
                    scale,
                    CLAY,
                );
            }
        }
        if !self.contour(&TENT_LIGHT, x - 6 * scale, y - 6 * scale, scale, WARNING) {
            for row in 0..6 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (6 - row) * scale,
                    (row + 1) * scale,
                    scale,
                    WARNING,
                );
            }
        }
        self.fill(x, y - 4 * scale, 3 * scale, 4 * scale, DEEP);
        self.fill(x - 9 * scale, y, 18 * scale, scale, DEEP);
    }
    fn mountain(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: impl Into<ThemedColor> + Copy,
    ) {
        for row in (0..height).step_by(4) {
            let span = (width * (row + 4).min(height) / height).min(self.width);
            self.fill(
                (x - span / 2).clamp(0, self.width - span),
                y - height + row,
                span,
                4.min(height - row),
                color,
            );
        }
    }
    fn scenery(&mut self, kind: Scenery) {
        let (w, h) = (self.width, self.height);
        match kind {
            Scenery::Logo => {
                let scale = (w.min(h) / 24).max(1);
                self.fill(0, 0, w, h, BACKGROUND);
                self.fill(4, 4, w - 8, h - 8, SKY);
                self.tree(w - 5 * scale, h - 4 * scale, scale, ACCENT);
                self.tent(w / 2 - 2 * scale, h - 5 * scale, scale);
                self.fill(4, h - 5, w - 8, 2, ACCENT);
            }
            Scenery::Header => {
                self.mountain(w - w / 4, h, w / 3, (h - 4).min(48), self.alpha(HILL, 45));
                self.mountain(w - w / 8, h, w / 4, (h - 4).min(64), self.alpha(SKY, 30));
                let scale = (h / 24).clamp(1, 3).min((w / 24).max(1));
                self.tree(w - 8 * scale, h, scale, self.alpha(HILL, 55));
                self.tree(w * 2 / 3, h, 1, self.alpha(HILL, 55));
            }
            Scenery::Map | Scenery::Margins => {
                if matches!(kind, Scenery::Margins) {
                    let (edge, length, stride) = if w < 500 { (4, 4, 16) } else { (10, 8, 22) };
                    for y in (h / 4..h - 36).step_by(stride) {
                        self.fill(edge, y, length, 2, self.alpha(DIVIDER, 125));
                        self.fill(
                            w - edge - length,
                            y + 8,
                            length,
                            2,
                            self.alpha(DIVIDER, 125),
                        );
                    }
                }
                for y in (12..h - 4).step_by(38) {
                    for x in (8..w - 8).step_by(94) {
                        self.fill(x, y, 2, 2, self.alpha(DIVIDER, 70));
                        self.fill(x + 4, y, 2, 2, self.alpha(DIVIDER, 70));
                    }
                }
            }
            Scenery::Ridge => {
                let floor = h - 6;
                self.mountain(w * 3 / 5, floor, w / 3, (floor - 4).min(42), PANEL);
                self.mountain(w * 4 / 5, floor, w / 3, (floor - 4).min(30), HILL);
                self.tent(w * 2 / 3, floor, 1);
                self.tree(w - 16, floor, 1, ACCENT);
                self.tree(w - 32, floor, 1, HILL);
                self.fill(w / 2, floor, w - w / 2, 3, DIVIDER);
            }
            Scenery::Landscape => {
                let floor = h - 10;
                self.fill(0, 0, w, floor, SKY);
                self.mountain(w / 3, floor, w * 2 / 3, (floor - 4).min(60), PANEL);
                self.mountain(w * 3 / 4, floor, w / 2, (floor - 4).min(44), HILL);
                self.tree(20, floor, 2, ACCENT);
                self.tree(w - 20, floor, 2, ACCENT);
                self.tree(w - 38, floor, 1, HILL);
                self.fill(0, floor, w, 4, HILL);
                for x in (10..w - 6).step_by(32) {
                    self.fill(x, floor + 6, 6, 2, DIVIDER);
                }
                self.fill(20, 14, 32, 3, BACKGROUND);
                self.fill(28, 11, 16, 3, BACKGROUND);
            }
        }
    }
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: impl Into<ThemedColor>) {
        if w <= 0 || h <= 0 {
            return;
        }
        #[cfg(test)]
        assert!(
            x >= 0 && y >= 0 && x + w <= self.width && y + h <= self.height,
            "Sprite primitive ({x}, {y}, {w}, {h}) exceeds {}×{}",
            self.width,
            self.height
        );
        (self.emit)(DrawCommand::Fill {
            area: Rect::new(
                x / self.unit,
                y / self.unit,
                (w / self.unit).max(1),
                (h / self.unit).max(1),
            ),
            color: color.into().resolve(self.theme),
            transform: Transform::IDENTITY,
            quad: None,
            radius: Fixed::ZERO,
            opa: 255,
        });
    }
    fn alpha(&self, token: ColorToken, alpha: u8) -> Color {
        let color = self.theme.resolve(token);
        Color::rgba(color.r, color.g, color.b, alpha)
    }
}
#[view(component = Scenery, priority = 50)]
pub(super) fn scenery(component: component!(), paint: paint!(), theme: res!(Theme)) {
    let rect = paint.rect();
    draw_scenery(
        *component,
        rect.w.to_int(),
        rect.h.to_int(),
        &theme,
        |command| draw_in_view(paint, command),
    );
}

fn draw_in_view(paint: &mut PaintCtx<'_>, mut command: DrawCommand<'_>) {
    let rect = paint.rect();
    let parent = paint
        .transform()
        .compose(&Transform::translate(rect.x, rect.y));
    match &mut command {
        DrawCommand::Fill {
            area,
            transform,
            quad,
            ..
        } => {
            *transform = parent.compose(transform);
            if !matches!(
                transform.classify(),
                crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
            ) {
                *quad = Some(transform.apply_rect(*area));
            }
        }
        DrawCommand::FillPath { transform, .. } => {
            *transform = parent.compose(transform);
        }
        _ => unreachable!("Pixel artwork emits fills and contours"),
    }
    paint.draw_clipped(&command, paint.transform().apply_rect_bbox(rect));
}

fn draw_scenery(
    kind: Scenery,
    width: i32,
    height: i32,
    theme: &Theme,
    emit: impl FnMut(DrawCommand<'_>),
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let (mw, mh) = match kind {
        Scenery::Logo => (48, 48),
        Scenery::Landscape | Scenery::Ridge => (112, 40),
        _ => (32, 16),
    };
    let unit = ((mw + width - 1) / width)
        .max((mh + height - 1) / height)
        .max(1);
    Painter {
        theme,
        emit,
        width: width * unit,
        height: height * unit,
        unit,
    }
    .scenery(kind);
}

#[compose]
pub(super) fn backdrop(kind: Scenery) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            kind,
            crate::ui::IgnoreHitTest,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scenery_scales_preserve_local_geometry_and_palette_roles() {
        for index in 0..3 {
            let theme = super::super::palette::theme(index);
            for kind in [
                Scenery::Logo,
                Scenery::Header,
                Scenery::Map,
                Scenery::Margins,
                Scenery::Landscape,
                Scenery::Ridge,
            ] {
                for (width, height) in [(16, 16), (48, 48), (320, 240), (640, 480)] {
                    draw_scenery(kind, width, height, &theme, |command| match command {
                        DrawCommand::Fill { area, color, .. } => {
                            assert!(area.w > Fixed::ZERO && area.h > Fixed::ZERO);
                            assert_ne!(color, Color::rgb(255, 0, 255));
                        }
                        DrawCommand::FillPath { .. } => {}
                        _ => panic!("Scenery must emit fills and static contours"),
                    });
                }
            }
        }
    }
}
