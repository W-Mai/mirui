use super::data::{Page, Phase, Snapshot as View};
use super::{copy, style::*};
use crate::anim::{Tween, TweenConfig, ease};
use crate::prelude::*;
use crate::types::Transform;
use alloc::{string::String, vec::Vec};

pub(crate) mod timing {
    use super::*;

    pub const TRAVEL: TweenConfig = TweenConfig::new(240, &ease::OUT_CUBIC);
    pub const SETTLED_MS: u16 = 800;
    pub const HOP_MS: u16 = 300;
    pub const RECOIL_MS: u16 = 240;
    pub const FOCUS_PULSE_MS: u16 = 240;
    pub const PRESS_MS: u16 = 120;
    pub const PRESS_FADE_MS: u16 = 180;
    pub const FLAG_RISE_MS: u16 = 240;
    pub const WALK_FRAME_MS: u16 = 90;
    pub const WAVE_MS: u16 = 1200;
    pub const WAVE_STEP_MS: u16 = 32;
    pub const WAVE_COUNT: u16 = 3;
    pub const CHOICE_WAVE_MS: u16 = 600;
    pub const DUST_MS: u16 = 260;
    pub const STARS_MS: u16 = 420;
    pub const HOST_HOP_DELAY_MS: u16 = 70;
    pub const SIGNAL_STAGGER_MS: u16 = 55;
    pub const ACTIVITY_MS: u16 = 450;
    pub const ACTIVITY_STAGGER_MS: u16 = 105;

    pub fn progress(age: u16, duration: u16) -> f32 {
        f32::from(age.min(duration)) / f32::from(duration)
    }
}

pub(super) mod anchors {
    use super::Fixed;

    pub const CHOICE_LEFT: Fixed = Fixed::from_ratio(1, 4);
    pub const CHOICE_RIGHT: Fixed = Fixed::from_ratio(3, 4);
    pub const HOST_HOME: Fixed = Fixed::from_ratio(1, 5);
    pub const HOST_EXIT: Fixed = Fixed::from_ratio(1, 10);
    pub const HOST_START: Fixed = Fixed::from_ratio(73, 100);
    pub const GUEST_HOME: Fixed = Fixed::from_ratio(4, 5);
    pub const GUEST_GATE: Fixed = Fixed::from_ratio(98, 100);
    pub const GUEST_APPROACH: Fixed = Fixed::from_ratio(94, 100);
    pub const GUEST_CHECK: Fixed = Fixed::from_ratio(88, 100);
    pub const GUEST_EXIT: Fixed = Fixed::from_ratio(3, 10);
    pub const GATE: Fixed = Fixed::from_ratio(9, 10);
}

use timing::SETTLED_MS;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_timeline_samples_reuse_the_room_identity_storage() {
        let mut timeline = Timeline::default();
        let mut view = View {
            page: Page::Host,
            room_name: "Room".into(),
            ..View::default()
        };
        timeline.sample(&view, Some(0), 0, 0, copy::loading(&view.presentation()));
        let pointer = timeline.key.as_ref().unwrap().room.as_ptr();
        for at in 1..1000 {
            timeline.sample(&view, Some(0), at, 0, copy::loading(&view.presentation()));
            assert_eq!(timeline.key.as_ref().unwrap().room.as_ptr(), pointer);
        }
        view.phase = Phase::HostReady;
        timeline.sample(&view, Some(0), 1001, 0, copy::loading(&view.presentation()));
        assert_eq!(timeline.key.as_ref().unwrap().room.as_ptr(), pointer);
    }

    #[test]
    fn card_projection_preserves_pixels_and_excludes_unrelated_activity() {
        for page in [Page::Games, Page::Choice, Page::Host, Page::Joined] {
            for age in 0..=SETTLED_MS {
                let frame = Frame {
                    page,
                    selected: Some(0),
                    selection: age,
                    phase_age: age / 2,
                    press: age / 3,
                    wave: Some(age),
                    walking: true,
                    ..Frame::default()
                };
                let cards = frame.cards();
                for index in [0, 1] {
                    assert_eq!(cards.card_transform(index), frame.card_transform(index));
                    assert_eq!(cards.card_color(index), frame.card_color(index));
                }
            }
        }
        let settled = Frame {
            selected: Some(0),
            ..Frame::default()
        };
        let loading = Frame {
            wave: Some(32),
            walking: true,
            ..settled
        };
        assert_eq!(settled.cards(), loading.cards());
        assert!(loading.wave.is_some());
    }

    #[test]
    fn travel_retargets_continuously_and_finishes_inside_normalized_bounds() {
        let mut travel = Travel::default();
        travel.target(anchors::CHOICE_LEFT, 0, true);
        travel.target(anchors::CHOICE_RIGHT, 0, false);
        for now in 0..u64::from(timing::TRAVEL.duration_ms) {
            let current = travel.value(now);
            assert!((Fixed::ZERO..=Fixed::ONE).contains(&current));
            let next = if now % 2 == 0 {
                anchors::CHOICE_LEFT
            } else {
                anchors::CHOICE_RIGHT
            };
            travel.target(next, now, false);
            assert_eq!(travel.value(now), current);
        }
        let start = u64::from(timing::TRAVEL.duration_ms);
        travel.target(anchors::GUEST_HOME, start, false);
        // Repeated observations of the same target must not extend the movement.
        for now in start..=start + u64::from(timing::TRAVEL.duration_ms) {
            travel.target(anchors::GUEST_HOME, now, false);
        }
        assert_eq!(travel.value(start * 2), anchors::GUEST_HOME);
        assert!(!travel.moving(start * 2));
    }

    #[test]
    fn entry_and_catalog_navigation_animate_without_poll_replays() {
        let mut timeline = Timeline::default();
        let mut view = View::default();
        for page in [Page::Mode, Page::Choice, Page::Mode, Page::Games] {
            view.page = page;
            assert_eq!(
                timeline
                    .sample(&view, Some(0), 1000, 0, copy::loading(&view.presentation()))
                    .selection,
                0
            );
            view.status = "Status update".into();
            assert_eq!(
                timeline
                    .sample(&view, Some(0), 1120, 0, copy::loading(&view.presentation()))
                    .selection,
                120
            );
            assert_eq!(
                timeline
                    .sample(&view, Some(0), 2000, 0, copy::loading(&view.presentation()))
                    .selection,
                SETTLED_MS
            );
        }
        view.core = "NES / FC".into();
        view.entries.push(super::super::data::GameRow {
            name: "Game".into(),
            ..Default::default()
        });
        assert_eq!(
            timeline
                .sample(&view, Some(0), 2100, 0, copy::loading(&view.presentation()))
                .selection,
            0
        );
        let moving = timeline.sample(&view, Some(0), 2160, 0, copy::loading(&view.presentation()));
        assert_ne!(moving.card_transform(0), Transform::IDENTITY);
        assert_eq!(moving.card_transform(1), Transform::IDENTITY);
        assert_eq!(
            timeline
                .sample(&view, Some(0), 3000, 0, copy::loading(&view.presentation()))
                .selection,
            SETTLED_MS
        );
    }

    #[test]
    fn snapshots_do_not_replay_motion_and_bursts_cannot_accumulate_scale() {
        let mut timeline = Timeline::default();
        let view = View::default();
        timeline.sample(&view, Some(0), 0, 0, copy::loading(&view.presentation()));
        timeline.sample(&view, Some(1), 10, 0, copy::loading(&view.presentation()));
        assert_eq!(
            timeline
                .sample(&view, Some(1), 120, 0, copy::loading(&view.presentation()))
                .expansion(),
            (
                Fixed::from_f32(FOCUS_OUTSET_X_PERCENT),
                Fixed::from_f32(FOCUS_OUTSET_Y_PERCENT)
            )
        );
        for i in 0..100 {
            let frame = timeline.sample(
                &view,
                Some(i % 2),
                140 + i as u64 * 17,
                0,
                copy::loading(&view.presentation()),
            );
            let (x, y) = frame.expansion();
            assert!(
                (Fixed::ZERO..=Fixed::from_f32(FOCUS_OUTSET_X_PERCENT)).contains(&x)
                    && (Fixed::ZERO..=Fixed::from_f32(FOCUS_OUTSET_Y_PERCENT)).contains(&y)
            );
        }
        let mut next = view.clone();
        next.status = "Updated status".into();
        let a = timeline.sample(&next, Some(1), 5000, 0, copy::loading(&next.presentation()));
        let b = timeline.sample(&next, Some(1), 6000, 0, copy::loading(&next.presentation()));
        assert_eq!(a, b);
        assert_eq!(b.expansion(), (Fixed::ZERO, Fixed::ZERO));
    }

    #[test]
    fn readiness_and_reopening_follow_backend_state() {
        let mut timeline = Timeline::default();
        let mut view = View {
            page: Page::Host,
            phase: Phase::WaitingPeer,
            ..View::default()
        };
        timeline.sample(&view, None, 0, 0, copy::loading(&view.presentation()));
        view.phase = Phase::PeerPreparing;
        assert!(
            !timeline
                .sample(&view, None, 10, 0, copy::loading(&view.presentation()))
                .ready
        );
        view.phase = Phase::HostReady;
        view.ready = true;
        assert_eq!(
            timeline
                .sample(&view, Some(0), 20, 0, copy::loading(&view.presentation()))
                .phase_age,
            0
        );
        assert_eq!(
            Timeline::default()
                .sample(&view, Some(0), 20, 0, copy::loading(&view.presentation()))
                .phase_age,
            SETTLED_MS
        );
    }

    #[test]
    fn session_routes_continue_from_current_positions_and_preserve_membership() {
        let mut timeline = Timeline::default();
        let mut view = View {
            page: Page::Host,
            phase: Phase::HostReady,
            ready: true,
            room_name: "Room".into(),
            ..View::default()
        };
        let ready = timeline.sample(&view, Some(0), 0, 0, copy::loading(&view.presentation()));
        view.phase = Phase::Starting;
        let start = timeline.sample(&view, Some(0), 10, 0, copy::loading(&view.presentation()));
        assert_eq!((ready.host, ready.guest), (start.host, start.guest));
        let moving = timeline.sample(&view, Some(0), 130, 0, copy::loading(&view.presentation()));
        assert!(moving.host > start.host && moving.guest > start.guest);
        view.page = Page::Leaving;
        view.phase = Phase::Leaving;
        view.room_name.clear();
        view.ready = false;
        let leaving = timeline.sample(&view, None, 130, 0, copy::loading(&view.presentation()));
        assert_eq!((moving.host, moving.guest), (leaving.host, leaving.guest));
        assert!(leaving.visitor);
        let settled = timeline.sample(&view, None, 1000, 0, copy::loading(&view.presentation()));
        assert!(settled.host < leaving.host && settled.guest < leaving.guest);
        assert!(!settled.host_walking && !settled.guest_walking);

        let mut alone = Timeline::default();
        view.page = Page::Host;
        view.phase = Phase::WaitingPeer;
        assert!(
            !alone
                .sample(&view, None, 0, 0, copy::loading(&view.presentation()))
                .visitor
        );
        view.page = Page::Leaving;
        view.phase = Phase::Leaving;
        assert!(
            !alone
                .sample(&view, None, 10, 0, copy::loading(&view.presentation()))
                .visitor
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Frame {
    pub external_actor: bool,
    pub page: Page,
    pub phase: Phase,
    pub selected: Option<usize>,
    pub ready: bool,
    pub connected: bool,
    pub network_only: bool,
    pub selection: u16,
    pub pulse: u16,
    pub phase_age: u16,
    pub press: u16,
    pub wave: Option<u16>,
    pub walker: Fixed,
    pub deck: Fixed,
    pub host: Fixed,
    pub guest: Fixed,
    pub walking: bool,
    pub host_walking: bool,
    pub guest_walking: bool,
    pub visitor: bool,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            external_actor: false,
            page: Page::Choice,
            phase: Phase::Idle,
            selected: None,
            ready: false,
            connected: false,
            network_only: false,
            selection: SETTLED_MS,
            pulse: SETTLED_MS,
            phase_age: SETTLED_MS,
            press: SETTLED_MS,
            wave: None,
            walker: anchors::CHOICE_LEFT,
            deck: Fixed::ZERO,
            host: anchors::HOST_HOME,
            guest: anchors::GUEST_HOME,
            walking: false,
            host_walking: false,
            guest_walking: false,
            visitor: false,
        }
    }
}

#[derive(PartialEq, Eq)]
struct SceneKey<R = String> {
    page: Page,
    phase: Phase,
    ready: bool,
    connected: bool,
    network_only: bool,
    room: R,
}

impl<R: AsRef<str>> SceneKey<R> {
    fn same_page<S: AsRef<str>>(&self, other: &SceneKey<S>) -> bool {
        self.page == other.page && self.room.as_ref() == other.room.as_ref()
    }
    fn same_state<S: AsRef<str>>(&self, other: &SceneKey<S>) -> bool {
        self.same_page(other)
            && self.phase == other.phase
            && self.ready == other.ready
            && self.connected == other.connected
            && self.network_only == other.network_only
    }

    fn is_session(&self) -> bool {
        matches!(self.page, Page::Host | Page::Joined | Page::Leaving)
    }
}

#[derive(Default)]
pub(super) struct Timeline {
    key: Option<SceneKey>,
    selected: Option<usize>,
    catalog: Option<(String, Vec<super::data::GameRow>)>,
    selection_at: Option<u64>,
    pulse_at: Option<u64>,
    phase_at: Option<u64>,
    press_at: Option<u64>,
    press_sequence: u64,
    walker: Travel,
    deck: Travel,
    host: Travel,
    guest: Travel,
    visitor: bool,
}

struct Travel {
    tween: Tween,
    at: u64,
}

impl Default for Travel {
    fn default() -> Self {
        Self {
            tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, timing::TRAVEL),
            at: 0,
        }
    }
}

impl Travel {
    fn sample(&self, now: u64) -> Tween {
        let mut tween = self.tween;
        tween.tick(
            now.saturating_sub(self.at)
                .min(u64::from(tween.duration_ms)) as u16,
        );
        tween
    }
    fn moving(&self, now: u64) -> bool {
        !self.sample(now).is_finished()
    }
    fn value(&self, now: u64) -> Fixed {
        self.sample(now).value()
    }
    fn target(&mut self, target: Fixed, now: u64, snap: bool) {
        if snap {
            self.tween = Tween::preset(target, target, timing::TRAVEL);
            self.at = now;
        } else if self.tween.to != target {
            self.tween = self.sample(now);
            self.tween.retarget(target, timing::TRAVEL);
            self.at = now;
        }
    }
}

fn age(now: u64, start: Option<u64>) -> u16 {
    start.map_or(SETTLED_MS, |start| {
        now.saturating_sub(start).min(u64::from(SETTLED_MS)) as u16
    })
}

impl Timeline {
    pub(super) fn sample(
        &mut self,
        view: &View,
        selected: Option<usize>,
        now: u64,
        press: u64,
        loading: bool,
    ) -> Frame {
        let key = SceneKey {
            page: view.scene_page(),
            phase: copy::phase(&view.presentation()),
            ready: view.ready,
            connected: view.connected,
            network_only: view.network_only,
            room: view.room_name.as_str(),
        };
        let page_changed = self.key.as_ref().is_none_or(|old| !old.same_page(&key));
        let catalog_changed = if matches!(view.page, Page::Games | Page::Candidates) {
            self.catalog
                .as_ref()
                .is_none_or(|(path, entries)| path != &view.core || entries != &view.entries)
        } else {
            self.catalog.is_some()
        };
        let deck_target = if view.lobby && view.page == Page::Mode {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        let pane_changed = self.deck.tween.to != deck_target;
        self.deck.target(deck_target, now, page_changed);
        let entered = pane_changed
            || page_changed
            || catalog_changed
            || self
                .key
                .as_ref()
                .is_some_and(|old| old.network_only != key.network_only);
        if catalog_changed {
            self.catalog = (matches!(view.page, Page::Games | Page::Candidates))
                .then(|| (view.core.clone(), view.entries.clone()));
        }
        let continues_session = self.key.as_ref().is_some_and(|old| {
            old.is_session()
                && key.is_session()
                && (old.room == key.room || key.page == Page::Leaving)
        });
        if !matches!(
            key.phase,
            Phase::Leaving | Phase::Failed | Phase::RestoreFailed | Phase::NetworkReconnecting
        ) || !continues_session
        {
            self.visitor = view.ready
                || view.connected
                || view.page == Page::Joined
                || matches!(key.phase, Phase::ApprovalPending | Phase::PeerPreparing);
        }
        let entering = self.key.is_some()
            && !continues_session
            && matches!(key.phase, Phase::Creating | Phase::Connecting);
        if self.key.as_ref().is_none_or(|old| !old.same_state(&key)) {
            // Opening an existing state does not replay its readiness celebration.
            self.phase_at = self
                .key
                .as_ref()
                .filter(|old| {
                    old.same_page(&key)
                        || matches!(
                            key.phase,
                            Phase::Creating | Phase::Connecting | Phase::Leaving
                        )
                })
                .map(|_| now);
            if let Some(old) = &mut self.key {
                old.page = key.page;
                old.phase = key.phase;
                old.ready = key.ready;
                old.connected = key.connected;
                old.network_only = key.network_only;
                old.room.clone_from(&view.room_name);
            } else {
                self.key = Some(SceneKey {
                    page: key.page,
                    phase: key.phase,
                    ready: key.ready,
                    connected: key.connected,
                    network_only: key.network_only,
                    room: view.room_name.clone(),
                });
            }
        }
        if entered || self.selected != selected {
            self.selection_at = Some(now);
            if entered || age(now, self.pulse_at) >= timing::FOCUS_PULSE_MS {
                self.pulse_at = Some(now);
            }
            self.selected = selected;
        }
        if press != self.press_sequence {
            self.press_sequence = press;
            self.press_at = Some(now);
        }
        self.walker.target(
            if selected == Some(1) {
                anchors::CHOICE_RIGHT
            } else {
                anchors::CHOICE_LEFT
            },
            now,
            page_changed,
        );
        let guest = match copy::phase(&view.presentation()) {
            Phase::ApprovalPending | Phase::ApprovalPendingClient => anchors::GUEST_GATE,
            Phase::Connecting
            | Phase::PeerPreparing
            | Phase::NetworkConfirming
            | Phase::NetworkReconnecting => anchors::GUEST_APPROACH,
            Phase::Checking | Phase::Notifying => anchors::GUEST_CHECK,
            Phase::Starting => anchors::GUEST_APPROACH,
            Phase::Leaving | Phase::Closed => anchors::GUEST_EXIT,
            Phase::Failed | Phase::RestoreFailed if continues_session => self.guest.value(now),
            _ => anchors::GUEST_HOME,
        };
        let host = match copy::phase(&view.presentation()) {
            Phase::Starting => anchors::HOST_START,
            Phase::Leaving | Phase::Closed => anchors::HOST_EXIT,
            Phase::Failed | Phase::RestoreFailed if continues_session => self.host.value(now),
            _ => anchors::HOST_HOME,
        };
        if entering {
            self.host.target(anchors::HOST_EXIT, now, true);
            self.guest.target(anchors::GUEST_GATE, now, true);
        }
        self.host.target(host, now, !continues_session && !entering);
        self.guest
            .target(guest, now, !continues_session && !entering);
        Frame {
            external_actor: false,
            page: view.page,
            phase: copy::phase(&view.presentation()),
            selected,
            ready: view.ready,
            connected: view.connected,
            network_only: view.network_only,
            selection: age(now, self.selection_at),
            pulse: age(now, self.pulse_at),
            phase_age: age(now, self.phase_at),
            press: age(now, self.press_at),
            wave: loading.then_some(
                (now % u64::from(timing::WAVE_MS) / u64::from(timing::WAVE_STEP_MS)
                    * u64::from(timing::WAVE_STEP_MS)) as u16,
            ),
            walker: self.walker.value(now),
            deck: self.deck.value(now),
            host: self.host.value(now),
            guest: self.guest.value(now),
            walking: self.walker.moving(now),
            host_walking: self.host.moving(now),
            guest_walking: self.guest.moving(now),
            visitor: self.visitor,
        }
    }
}

pub(super) fn hop(age: u16, height: f32) -> i32 {
    let p = timing::progress(age, timing::HOP_MS);
    -libm::roundf(libm::sinf(p * core::f32::consts::PI) * height) as i32
}

pub(super) fn recoil(age: u16) -> i32 {
    let p = timing::progress(age, timing::RECOIL_MS);
    libm::roundf(libm::sinf(p * core::f32::consts::PI * 5.0) * 3.0 * (1.0 - p) * (1.0 - p)) as i32
}

impl Frame {
    pub(super) fn expansion(self) -> (Fixed, Fixed) {
        let p = timing::progress(self.pulse, timing::FOCUS_PULSE_MS);
        let amount = 16.0 * (p * p) * ((1.0 - p) * (1.0 - p));
        (
            Fixed::from_f32(amount * FOCUS_OUTSET_X_PERCENT),
            Fixed::from_f32(amount * FOCUS_OUTSET_Y_PERCENT),
        )
    }

    pub(super) fn card_transform(self, index: usize) -> Transform {
        if self.selected != Some(index) {
            return Transform::IDENTITY;
        }
        let pressed = if self.press < timing::PRESS_MS { 2 } else { 0 };
        let age = if matches!(self.page, Page::Host | Page::Joined) {
            self.phase_age.min(self.selection)
        } else {
            self.selection
        };
        Transform::translate(recoil(age).into(), (hop(age, 3.0) + pressed).into())
    }

    pub(super) fn card_color(self, index: usize) -> ColorToken {
        if self.selected == Some(index) && self.selection < timing::HOP_MS {
            ACCENT_DIM
        } else {
            PANEL
        }
    }
}

impl Frame {
    pub(super) fn cards(self) -> Self {
        Self {
            page: self.page,
            selected: self.selected,
            selection: self.selection.min(timing::HOP_MS),
            phase_age: self.phase_age.min(timing::HOP_MS),
            press: self.press.min(timing::PRESS_MS),
            ..Self::default()
        }
    }

    pub(super) fn choice(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            ..Self::default()
        }
    }
    pub(super) fn walker(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            walker: self.walker,
            walking: self.walking,
            ..Self::default()
        }
    }
    pub(super) fn session(self) -> Self {
        Self {
            phase: self.phase,
            phase_age: self.phase_age,
            ready: self.ready,
            connected: self.connected,
            network_only: self.network_only,
            host: self.host,
            guest: self.guest,
            host_walking: self.host_walking,
            guest_walking: self.guest_walking,
            visitor: self.visitor,
            wave: self.wave,
            ..Self::default()
        }
    }
    pub(super) fn signal(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            wave: self.wave,
            ..Self::default()
        }
    }
    pub(super) fn search(self) -> Self {
        Self {
            wave: self.wave,
            ..Self::default()
        }
    }
}
