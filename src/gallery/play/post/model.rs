use super::data::{MANIFEST_COUNT, MANIFESTS, SPAWN_PERIOD_HALF_TICKS, STATION_FLASH_TICKS};
use super::route::PostRoute;
use super::types::{DeliveryEvent, MAX_EVENTS, MAX_PARCELS, PostModal, PostParcel};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::types::Fixed;

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PostModel {
    pub(super) parcels: [PostParcel; MAX_PARCELS],
    pub(super) events: [DeliveryEvent; MAX_EVENTS],
    pub(super) arrivals: [u8; 3],
    pub(super) flashes: [u8; 3],
    pub(super) switches: [u8; 2],
    pub(super) clock: BoundedClock,
    pub(super) manifest_id: u8,
    pub(super) cursor: u8,
    pub(super) parcel_len: u8,
    pub(super) event_len: u8,
    pub(super) delivered: u8,
    pub(super) missed: u8,
    pub(super) streak: u8,
    pub(super) speed_index: u8,
    pub(super) running: bool,
    pub(super) started: bool,
    pub(super) finished: bool,
    pub(super) modal: PostModal,
    pub(super) spawn_clock_half_ticks: u16,
    pub(super) score: u16,
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
    pub(crate) fn active(&self, index: usize) -> Option<PostParcel> {
        (index < usize::from(self.parcel_len)).then_some(self.parcels[index])
    }

    pub(crate) fn queued(&self, index: usize) -> Option<u8> {
        MANIFESTS[usize::from(self.manifest_id)]
            .get(usize::from(self.cursor) + index)
            .copied()
    }

    pub(crate) const fn switch(&self, index: usize) -> u8 {
        self.switches[index]
    }

    pub(crate) const fn cursor(&self) -> u8 {
        self.cursor
    }

    pub(crate) const fn station_flashing(&self, station: usize) -> bool {
        self.flashes[station] != 0
    }

    #[cfg(test)]
    pub(crate) const fn event_len(&self) -> u8 {
        self.event_len
    }

    #[cfg(test)]
    pub(crate) fn event(&self, index: usize) -> Option<DeliveryEvent> {
        (index < usize::from(self.event_len)).then_some(self.events[index])
    }
}

#[crate::model]
impl PostModel {
    #[observe]
    pub(crate) fn manifest_id(&self) -> u8 {
        self.manifest_id
    }

    #[observe]
    pub(crate) fn manifest_len(&self) -> u8 {
        MANIFESTS[usize::from(self.manifest_id)].len() as u8
    }

    #[observe]
    pub(crate) fn active_len(&self) -> u8 {
        self.parcel_len
    }

    #[observe]
    pub(crate) fn delivered(&self) -> u8 {
        self.delivered
    }

    #[observe]
    pub(crate) fn missed(&self) -> u8 {
        self.missed
    }

    #[observe]
    pub(crate) fn streak(&self) -> u8 {
        self.streak
    }

    #[observe]
    pub(crate) fn score(&self) -> u16 {
        self.score
    }

    #[observe]
    pub(crate) fn station_a_count(&self) -> u8 {
        self.arrivals[0]
    }

    #[observe]
    pub(crate) fn station_b_count(&self) -> u8 {
        self.arrivals[1]
    }

    #[observe]
    pub(crate) fn station_c_count(&self) -> u8 {
        self.arrivals[2]
    }

    #[observe]
    pub(crate) fn running(&self) -> bool {
        self.running
    }

    #[observe]
    pub(crate) fn started(&self) -> bool {
        self.started
    }

    #[observe]
    pub(crate) fn finished(&self) -> bool {
        self.finished
    }

    #[observe]
    pub(crate) fn modal(&self) -> PostModal {
        self.modal
    }

    #[observe]
    pub(crate) fn speed_x2(&self) -> u8 {
        [1, 2, 3][self.speed_index as usize]
    }

    #[observe]
    pub(crate) fn can_send(&self) -> bool {
        !self.finished
            && self.modal == PostModal::None
            && self.cursor < self.manifest_len()
            && usize::from(self.parcel_len) < MAX_PARCELS
    }

    #[observe]
    pub(crate) fn queued_slots(&self) -> [Option<u8>; 5] {
        core::array::from_fn(|index| self.queued(index))
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
}

impl PostModel {
    pub(super) fn spawn(&mut self) -> bool {
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

    pub(super) fn step_fixed(&mut self) -> ChangeSet {
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

    pub(super) fn record_delivery(&mut self, parcel: PostParcel, route: PostRoute) {
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

    pub(super) fn remove_parcel(&mut self, index: usize) {
        let len = usize::from(self.parcel_len);
        for source in index + 1..len {
            self.parcels[source - 1] = self.parcels[source];
        }
        self.parcel_len -= 1;
        self.parcels[usize::from(self.parcel_len)] = PostParcel::EMPTY;
    }
}
