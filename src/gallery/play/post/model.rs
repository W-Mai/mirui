use super::data::{SHIFT_COUNT, SHIFTS, STATION_FLASH_TICKS, Shift};
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
    pub(super) shift: u8,
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
    /// Best finished score of each shift; zero until it is finished once.
    pub(super) best: [u16; SHIFT_COUNT],
    /// Shifts finished without a single wrong delivery.
    pub(super) starred: u8,
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
            shift: 0,
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
            best: [0; SHIFT_COUNT],
            starred: 0,
        }
    }
}

impl PostModel {
    pub(crate) fn current(&self) -> &'static Shift {
        &SHIFTS[usize::from(self.shift)]
    }

    pub(crate) fn active(&self, index: usize) -> Option<PostParcel> {
        (index < usize::from(self.parcel_len)).then_some(self.parcels[index])
    }

    pub(crate) fn queued(&self, index: usize) -> Option<u8> {
        self.current()
            .parcels
            .get(usize::from(self.cursor) + index)
            .copied()
    }

    pub(crate) const fn switch(&self, index: usize) -> u8 {
        self.switches[index]
    }

    #[cfg(test)]
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
    pub(crate) fn shift(&self) -> u8 {
        self.shift
    }

    #[observe]
    pub(crate) fn parcel_count(&self) -> u8 {
        self.current().parcels.len() as u8
    }

    /// Parcels the current shift allows on the rails at once.
    #[observe]
    pub(crate) fn in_flight(&self) -> u8 {
        self.current().in_flight
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
    pub(crate) fn best(&self) -> [u16; SHIFT_COUNT] {
        self.best
    }

    /// Best finished score of the current shift.
    #[observe]
    pub(crate) fn current_best(&self) -> u16 {
        self.best[usize::from(self.shift)]
    }

    #[observe]
    pub(crate) fn starred(&self) -> u8 {
        self.starred
    }

    #[observe]
    pub(crate) fn has_next(&self) -> bool {
        usize::from(self.shift) + 1 < SHIFT_COUNT
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
            && self.cursor < self.parcel_count()
            && self.parcel_len < self.in_flight()
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
            self.load_shift(self.shift);
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

    pub(crate) fn open_shifts(&mut self) -> ChangeSet {
        self.running = false;
        self.clock.reset();
        self.modal = PostModal::Shifts;
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

    /// Starts `shift` from its first parcel. Best scores, stars and the
    /// chosen speed carry over.
    pub(crate) fn load_shift(&mut self, shift: u8) -> ChangeSet {
        if usize::from(shift) >= SHIFT_COUNT {
            return ChangeSet::NONE;
        }
        *self = Self {
            shift,
            speed_index: self.speed_index,
            best: self.best,
            starred: self.starred,
            ..Self::default()
        };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.load_shift(self.shift)
    }

    /// Moves on to the shift after this one, or opens the shift list after
    /// the last.
    pub(crate) fn next_shift(&mut self) -> ChangeSet {
        if self.has_next() {
            self.load_shift(self.shift + 1)
        } else {
            self.open_shifts()
        }
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
            || self.cursor >= self.parcel_count()
            || self.parcel_len >= self.in_flight()
        {
            return false;
        }
        let target = self.current().parcels[usize::from(self.cursor)];
        self.parcels[usize::from(self.parcel_len)] = PostParcel::new(self.cursor, target);
        self.parcel_len += 1;
        self.cursor += 1;
        true
    }

    pub(super) fn step_fixed(&mut self) -> ChangeSet {
        let speed_x2 = self.speed_x2();
        let period = self.current().spawn_period;
        self.spawn_clock_half_ticks = self
            .spawn_clock_half_ticks
            .saturating_add(u16::from(speed_x2));
        let mut model_changed = false;
        if self.spawn_clock_half_ticks >= period {
            if self.spawn() {
                self.spawn_clock_half_ticks = 0;
                model_changed = true;
            } else {
                self.spawn_clock_half_ticks = period;
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
        if self.cursor == self.parcel_count() && self.parcel_len == 0 {
            self.finish();
            model_changed = true;
        }
        if model_changed {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL
        }
    }

    /// Closes the shift: keeps its best score, stars it when nothing went
    /// astray, and shows the summary.
    pub(super) fn finish(&mut self) {
        self.running = false;
        self.finished = true;
        self.modal = PostModal::Summary;
        self.clock.reset();
        let shift = usize::from(self.shift);
        self.best[shift] = self.best[shift].max(self.score);
        if self.missed == 0 {
            self.starred |= 1 << shift;
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
