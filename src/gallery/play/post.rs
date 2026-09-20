use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::types::{Fixed, Point};

pub(crate) const MAX_PARCELS: usize = 3;
pub(crate) const MAX_EVENTS: usize = 12;
pub(crate) const MANIFEST_COUNT: usize = 3;

const MANIFEST_0: [u8; 9] = [0, 1, 2, 0, 2, 1, 0, 1, 2];
const MANIFEST_1: [u8; 12] = [2, 0, 1, 2, 1, 0, 1, 2, 0, 2, 1, 0];
const MANIFEST_2: [u8; 6] = [0, 0, 1, 1, 2, 2];
const MANIFESTS: [&[u8]; MANIFEST_COUNT] = [&MANIFEST_0, &MANIFEST_1, &MANIFEST_2];
const SPAWN_PERIOD_HALF_TICKS: u16 = 276;
const STATION_FLASH_TICKS: u8 = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostRoute {
    Entry,
    ToSecondSwitch,
    StationA,
    StationB,
    StationC,
}

impl PostRoute {
    const fn terminal_station(self) -> Option<u8> {
        match self {
            Self::StationA => Some(0),
            Self::StationB => Some(1),
            Self::StationC => Some(2),
            Self::Entry | Self::ToSecondSwitch => None,
        }
    }

    fn length(self) -> Fixed {
        match self {
            Self::Entry => Fixed::from_int(119),
            Self::ToSecondSwitch => Fixed::from_int(114),
            Self::StationA => Fixed::from_ratio(285_994, 1_000),
            Self::StationB => Fixed::from_int(135),
            Self::StationC => Fixed::from_ratio(173_404, 1_000),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostParcel {
    id: u8,
    target: u8,
    route: PostRoute,
    distance: Fixed,
    decisions: [u8; 2],
    decision_count: u8,
}

impl PostParcel {
    const EMPTY: Self = Self {
        id: 0,
        target: 0,
        route: PostRoute::Entry,
        distance: Fixed::ZERO,
        decisions: [0; 2],
        decision_count: 0,
    };

    const fn new(id: u8, target: u8) -> Self {
        Self {
            id,
            target,
            ..Self::EMPTY
        }
    }

    pub(crate) const fn target(self) -> u8 {
        self.target
    }

    #[cfg(test)]
    pub(crate) const fn route(self) -> PostRoute {
        self.route
    }

    #[cfg(test)]
    pub(crate) const fn distance(self) -> Fixed {
        self.distance
    }

    #[cfg(test)]
    pub(crate) const fn decision_count(self) -> u8 {
        self.decision_count
    }

    pub(crate) fn position(self) -> Point {
        position_on_route(self.route, self.distance)
    }

    fn lock_decision(&mut self, decision: u8) {
        if usize::from(self.decision_count) < self.decisions.len() {
            self.decisions[usize::from(self.decision_count)] = decision;
            self.decision_count += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeliveryEvent {
    pub(crate) parcel_id: u8,
    pub(crate) target: u8,
    pub(crate) station: u8,
    pub(crate) correct: bool,
}

impl DeliveryEvent {
    const EMPTY: Self = Self {
        parcel_id: 0,
        target: 0,
        station: 0,
        correct: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostModal {
    None,
    Manifests,
    Summary,
    Reset,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PostModel {
    parcels: [PostParcel; MAX_PARCELS],
    events: [DeliveryEvent; MAX_EVENTS],
    arrivals: [u8; 3],
    flashes: [u8; 3],
    switches: [u8; 2],
    clock: BoundedClock,
    manifest_id: u8,
    cursor: u8,
    parcel_len: u8,
    event_len: u8,
    delivered: u8,
    missed: u8,
    streak: u8,
    speed_index: u8,
    running: bool,
    started: bool,
    finished: bool,
    modal: PostModal,
    spawn_clock_half_ticks: u16,
    score: u16,
}

impl Default for PostModel {
    fn default() -> Self {
        Self {
            parcels: [PostParcel::EMPTY; MAX_PARCELS],
            events: [DeliveryEvent::EMPTY; MAX_EVENTS],
            arrivals: [0; 3],
            flashes: [0; 3],
            switches: [0; 2],
            clock: BoundedClock::new(60, 120, 7),
            manifest_id: 0,
            cursor: 0,
            parcel_len: 0,
            event_len: 0,
            delivered: 0,
            missed: 0,
            streak: 0,
            speed_index: 1,
            running: false,
            started: false,
            finished: false,
            modal: PostModal::None,
            spawn_clock_half_ticks: 0,
            score: 0,
        }
    }
}

impl PostModel {
    pub(crate) const fn manifest_id(&self) -> u8 {
        self.manifest_id
    }

    pub(crate) fn manifest_len(&self) -> u8 {
        MANIFESTS[usize::from(self.manifest_id)].len() as u8
    }

    pub(crate) const fn cursor(&self) -> u8 {
        self.cursor
    }

    pub(crate) const fn active_len(&self) -> u8 {
        self.parcel_len
    }

    pub(crate) fn active(&self, index: usize) -> Option<PostParcel> {
        (index < usize::from(self.parcel_len)).then_some(self.parcels[index])
    }

    pub(crate) fn queued(&self, index: usize) -> Option<u8> {
        MANIFESTS[usize::from(self.manifest_id)]
            .get(usize::from(self.cursor) + index)
            .copied()
    }

    pub(crate) const fn delivered(&self) -> u8 {
        self.delivered
    }

    pub(crate) const fn missed(&self) -> u8 {
        self.missed
    }

    pub(crate) const fn streak(&self) -> u8 {
        self.streak
    }

    pub(crate) const fn score(&self) -> u16 {
        self.score
    }

    pub(crate) const fn switch(&self, index: usize) -> u8 {
        self.switches[index]
    }

    pub(crate) const fn arrivals(&self, station: usize) -> u8 {
        self.arrivals[station]
    }

    pub(crate) const fn station_flashing(&self, station: usize) -> bool {
        self.flashes[station] != 0
    }

    pub(crate) const fn running(&self) -> bool {
        self.running
    }

    pub(crate) const fn started(&self) -> bool {
        self.started
    }

    pub(crate) const fn finished(&self) -> bool {
        self.finished
    }

    pub(crate) const fn modal(&self) -> PostModal {
        self.modal
    }

    #[cfg(test)]
    pub(crate) const fn event_len(&self) -> u8 {
        self.event_len
    }

    #[cfg(test)]
    pub(crate) fn event(&self, index: usize) -> Option<DeliveryEvent> {
        (index < usize::from(self.event_len)).then_some(self.events[index])
    }

    pub(crate) const fn speed_x2(&self) -> u8 {
        [1, 2, 3][self.speed_index as usize]
    }

    pub(crate) fn toggle_switch(&mut self, index: usize) -> ChangeSet {
        if self.modal != PostModal::None || index >= self.switches.len() {
            return ChangeSet::NONE;
        }
        self.switches[index] ^= 1;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_running(&mut self) -> ChangeSet {
        if self.modal != PostModal::None {
            return ChangeSet::NONE;
        }
        if self.finished {
            self.load_manifest(self.manifest_id);
        }
        self.running = !self.running;
        self.clock.reset();
        if self.running && !self.started {
            self.started = true;
            let _ = self.spawn();
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn spawn_manual(&mut self) -> ChangeSet {
        if self.modal != PostModal::None || !self.spawn() {
            return ChangeSet::NONE;
        }
        self.started = true;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn cycle_speed(&mut self) -> ChangeSet {
        if self.modal != PostModal::None {
            return ChangeSet::NONE;
        }
        self.speed_index = (self.speed_index + 1) % 3;
        self.clock.reset();
        ChangeSet::MODEL
    }

    pub(crate) fn open_manifests(&mut self) -> ChangeSet {
        self.running = false;
        self.clock.reset();
        self.modal = PostModal::Manifests;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn open_reset(&mut self) -> ChangeSet {
        self.running = false;
        self.clock.reset();
        self.modal = PostModal::Reset;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        if self.modal == PostModal::None {
            return ChangeSet::NONE;
        }
        self.modal = PostModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn load_manifest(&mut self, manifest_id: u8) -> ChangeSet {
        if usize::from(manifest_id) >= MANIFEST_COUNT {
            return ChangeSet::NONE;
        }
        *self = Self {
            manifest_id,
            ..Self::default()
        };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.load_manifest(self.manifest_id)
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let steps = self.clock.steps(
            elapsed_ms,
            self.running && self.modal == PostModal::None && !self.finished,
        );
        let mut changes = ChangeSet::NONE;
        for _ in 0..steps {
            changes = changes | self.step_fixed();
        }
        changes
    }

    fn spawn(&mut self) -> bool {
        if self.finished
            || self.cursor >= self.manifest_len()
            || usize::from(self.parcel_len) >= MAX_PARCELS
        {
            return false;
        }
        let target = MANIFESTS[usize::from(self.manifest_id)][usize::from(self.cursor)];
        self.parcels[usize::from(self.parcel_len)] = PostParcel::new(self.cursor, target);
        self.parcel_len += 1;
        self.cursor += 1;
        true
    }

    fn step_fixed(&mut self) -> ChangeSet {
        let speed_x2 = self.speed_x2();
        self.spawn_clock_half_ticks = self
            .spawn_clock_half_ticks
            .saturating_add(u16::from(speed_x2));
        let mut model_changed = false;
        if self.spawn_clock_half_ticks >= SPAWN_PERIOD_HALF_TICKS {
            if self.spawn() {
                self.spawn_clock_half_ticks = 0;
                model_changed = true;
            } else {
                self.spawn_clock_half_ticks = SPAWN_PERIOD_HALF_TICKS;
            }
        }
        for flash in &mut self.flashes {
            *flash = flash.saturating_sub(1);
        }
        let movement = Fixed::from_ratio(i32::from(speed_x2), 2);
        let mut index = 0;
        while index < usize::from(self.parcel_len) {
            self.parcels[index].distance += movement;
            let mut delivered = false;
            for _ in 0..4 {
                let length = self.parcels[index].route.length();
                if self.parcels[index].distance < length {
                    break;
                }
                self.parcels[index].distance -= length;
                match self.parcels[index].route {
                    PostRoute::Entry => {
                        let decision = self.switches[0];
                        self.parcels[index].lock_decision(decision);
                        self.parcels[index].route = if decision == 0 {
                            PostRoute::StationA
                        } else {
                            PostRoute::ToSecondSwitch
                        };
                    }
                    PostRoute::ToSecondSwitch => {
                        let decision = self.switches[1];
                        self.parcels[index].lock_decision(decision);
                        self.parcels[index].route = if decision == 0 {
                            PostRoute::StationB
                        } else {
                            PostRoute::StationC
                        };
                    }
                    route => {
                        self.record_delivery(self.parcels[index], route);
                        self.remove_parcel(index);
                        delivered = true;
                        model_changed = true;
                        break;
                    }
                }
            }
            if !delivered {
                index += 1;
            }
        }
        if self.cursor == self.manifest_len() && self.parcel_len == 0 {
            self.running = false;
            self.finished = true;
            self.modal = PostModal::Summary;
            self.clock.reset();
            model_changed = true;
        }
        if model_changed {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL
        }
    }

    fn record_delivery(&mut self, parcel: PostParcel, route: PostRoute) {
        let station = route
            .terminal_station()
            .expect("delivery route must terminate at a station");
        let correct = parcel.target == station;
        self.arrivals[usize::from(station)] = self.arrivals[usize::from(station)].saturating_add(1);
        self.flashes[usize::from(station)] = STATION_FLASH_TICKS;
        if correct {
            self.delivered = self.delivered.saturating_add(1);
            self.streak = self.streak.saturating_add(1);
            let bonus = u16::from(self.streak.saturating_sub(1).min(5)) * 20;
            self.score = self.score.saturating_add(100 + bonus);
        } else {
            self.missed = self.missed.saturating_add(1);
            self.streak = 0;
        }
        if usize::from(self.event_len) < MAX_EVENTS {
            self.events[usize::from(self.event_len)] = DeliveryEvent {
                parcel_id: parcel.id,
                target: parcel.target,
                station,
                correct,
            };
            self.event_len += 1;
        }
    }

    fn remove_parcel(&mut self, index: usize) {
        let len = usize::from(self.parcel_len);
        for source in index + 1..len {
            self.parcels[source - 1] = self.parcels[source];
        }
        self.parcel_len -= 1;
        self.parcels[usize::from(self.parcel_len)] = PostParcel::EMPTY;
    }
}

pub(crate) fn position_on_route(route: PostRoute, distance: Fixed) -> Point {
    match route {
        PostRoute::Entry => Point::new(Fixed::from_int(32) + distance, Fixed::from_int(157)),
        PostRoute::ToSecondSwitch => {
            Point::new(Fixed::from_int(151) + distance, Fixed::from_int(157))
        }
        PostRoute::StationA => polyline_position(
            Point::new(Fixed::from_int(151), Fixed::from_int(157)),
            Point::new(Fixed::from_int(195), Fixed::from_int(89)),
            Point::new(Fixed::from_int(400), Fixed::from_int(89)),
            Fixed::from_ratio(80_994, 1_000),
            distance,
        ),
        PostRoute::StationB => Point::new(Fixed::from_int(265) + distance, Fixed::from_int(157)),
        PostRoute::StationC => polyline_position(
            Point::new(Fixed::from_int(265), Fixed::from_int(157)),
            Point::new(Fixed::from_int(306), Fixed::from_int(225)),
            Point::new(Fixed::from_int(400), Fixed::from_int(225)),
            Fixed::from_ratio(79_404, 1_000),
            distance,
        ),
    }
}

fn polyline_position(
    start: Point,
    corner: Point,
    end: Point,
    first_length: Fixed,
    distance: Fixed,
) -> Point {
    if distance <= first_length {
        let t = distance / first_length;
        return Point {
            x: start.x + (corner.x - start.x) * t,
            y: start.y + (corner.y - start.y) * t,
        };
    }
    let remaining = distance - first_length;
    let second_length = (end.x - corner.x).abs() + (end.y - corner.y).abs();
    let t = (remaining / second_length).min(Fixed::ONE);
    Point {
        x: corner.x + (end.x - corner.x) * t,
        y: corner.y + (end.y - corner.y) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_steps(model: &mut PostModel, count: usize) {
        for _ in 0..count {
            let _ = model.step_fixed();
        }
    }

    #[test]
    fn initial_start_dispatches_once_and_pause_preserves_position() {
        let mut model = PostModel::default();
        model.toggle_running();
        assert!(model.running());
        assert_eq!(model.cursor(), 1);
        assert_eq!(model.active_len(), 1);
        run_steps(&mut model, 17);
        let before = model.active(0).unwrap().distance();
        model.toggle_running();
        let _ = model.advance_ms(500);
        assert_eq!(model.active(0).unwrap().distance(), before);
    }

    #[test]
    fn switch_choice_locks_when_the_parcel_crosses_the_junction() {
        let mut model = PostModel::default();
        model.toggle_running();
        run_steps(&mut model, 118);
        assert_eq!(model.active(0).unwrap().route(), PostRoute::Entry);
        model.toggle_switch(0);
        run_steps(&mut model, 1);
        assert_eq!(model.active(0).unwrap().route(), PostRoute::ToSecondSwitch);
        model.toggle_switch(0);
        assert_eq!(model.active(0).unwrap().route(), PostRoute::ToSecondSwitch);
        assert_eq!(model.active(0).unwrap().decision_count(), 1);
    }

    #[test]
    fn residual_distance_survives_a_junction_step() {
        let mut model = PostModel::default();
        model.toggle_running();
        model.speed_index = 2;
        run_steps(&mut model, 80);
        let parcel = model.active(0).unwrap();
        assert_eq!(parcel.route(), PostRoute::StationA);
        assert_eq!(parcel.distance(), Fixed::ONE);
    }

    #[test]
    fn capacity_is_bounded_without_a_hidden_queue() {
        let mut model = PostModel::default();
        assert!(model.spawn());
        assert!(model.spawn());
        assert!(model.spawn());
        assert!(!model.spawn());
        model.spawn_clock_half_ticks = SPAWN_PERIOD_HALF_TICKS;
        let cursor = model.cursor();
        let _ = model.step_fixed();
        assert_eq!(model.active_len(), MAX_PARCELS as u8);
        assert_eq!(model.cursor(), cursor);
        assert_eq!(model.spawn_clock_half_ticks, SPAWN_PERIOD_HALF_TICKS);
    }

    #[test]
    fn terminal_delivery_is_recorded_exactly_once() {
        let mut model = PostModel::default();
        model.toggle_running();
        run_steps(&mut model, 406);
        assert_eq!(model.event_len(), 1);
        assert_eq!(model.delivered(), 1);
        let event = model.event(0).unwrap();
        assert_eq!(event.parcel_id, 0);
        assert!(event.correct);
        run_steps(&mut model, 1);
        assert_eq!(model.event_len(), 1);
    }

    #[test]
    fn wrong_station_resets_the_streak_and_scores_nothing() {
        let mut model = PostModel::default();
        model.parcels[0] = PostParcel {
            route: PostRoute::StationB,
            distance: PostRoute::StationB.length() - Fixed::HALF,
            ..PostParcel::new(0, 0)
        };
        model.parcel_len = 1;
        model.running = true;
        model.started = true;
        model.streak = 3;
        let _ = model.step_fixed();
        assert_eq!(model.missed(), 1);
        assert_eq!(model.streak(), 0);
        assert_eq!(model.score(), 0);
    }

    #[test]
    fn correct_streak_bonus_caps_at_one_hundred() {
        let mut model = PostModel::default();
        for id in 0..8 {
            model.record_delivery(PostParcel::new(id, 0), PostRoute::StationA);
        }
        assert_eq!(model.score(), 1_300);
        assert_eq!(model.delivered(), 8);
    }

    #[test]
    fn speed_changes_dispatch_and_motion_rate_without_rerouting() {
        let mut slow = PostModel::default();
        slow.toggle_running();
        slow.speed_index = 0;
        let mut fast = PostModel::default();
        fast.toggle_running();
        fast.speed_index = 2;
        run_steps(&mut slow, 60);
        run_steps(&mut fast, 60);
        assert_eq!(slow.active(0).unwrap().distance(), Fixed::from_int(30));
        assert_eq!(fast.active(0).unwrap().distance(), Fixed::from_int(90));
        assert_eq!(slow.cursor(), 1);
        assert_eq!(fast.cursor(), 1);
    }

    #[test]
    fn opening_and_closing_a_panel_never_resumes_the_shift() {
        let mut model = PostModel::default();
        model.toggle_running();
        model.open_manifests();
        assert!(!model.running());
        assert_eq!(model.modal(), PostModal::Manifests);
        model.close_modal();
        assert!(!model.running());
        assert_eq!(model.modal(), PostModal::None);
    }

    #[test]
    fn each_manifest_finishes_with_a_complete_terminal_partition() {
        for manifest_id in 0..MANIFEST_COUNT as u8 {
            let mut model = PostModel::default();
            model.load_manifest(manifest_id);
            model.toggle_running();
            for _ in 0..20_000 {
                let _ = model.step_fixed();
                if model.finished() {
                    break;
                }
            }
            assert!(model.finished());
            assert_eq!(model.event_len(), model.manifest_len());
            assert_eq!(model.delivered() + model.missed(), model.manifest_len());
            assert_eq!(model.active_len(), 0);
        }
    }

    #[test]
    fn model_storage_stays_bounded() {
        assert!(core::mem::size_of::<PostModel>() <= 512);
    }
}
