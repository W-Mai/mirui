use super::missions::{MISSION_COUNT, MISSIONS, OrbitGoal, OrbitMission};
use super::physics::{CRASH_RADIUS, ESCAPE_RADIUS, MU, START_RADIUS, STEP, impulse, integrate};
use super::types::{
    LOG_LINES, MAX_EVENTS, MAX_NODES, MAX_PREVIEW, MAX_TELEMETRY, MAX_TRAIL, ManeuverNode,
    OrbitBody, OrbitCue, OrbitError, OrbitEvent, OrbitEventKind, OrbitModal, OrbitPage, OrbitPoint,
    OrbitSample, OrbitStatus, TrackPoint,
};
use crate::anim::{Tween, TweenConfig, ease};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::types::{Fixed, Fixed64};

const RECORD_INTERVAL: Fixed64 = Fixed64::from_ratio(4, 25);
/// The charts keep every second record, so they span the trail's 30 s.
const TELEMETRY_EVERY: u8 = 2;
/// Seconds between forecast points.
const FORECAST_STEP: Fixed64 = Fixed64::from_ratio(1, 10);
/// While coasting, the burn preview is recomputed this often.
const AIM_REFRESH: Fixed64 = Fixed64::from_ratio(7, 20);
/// The planned coast is recomputed once the craft has flown this far along it.
const COAST_REFRESH: Fixed64 = Fixed64::from_int(12);
const MAX_HEAT: i64 = 100;
const START_DV_TENTHS: u8 = 38;
const START_DELAY_SECONDS: u8 = 3;
/// No node fires inside the forecast.
const UNMARKED: u8 = u8::MAX;
/// A burn, arrival, sample or crash flash on the map.
const CUE: TweenConfig = TweenConfig::new(380, &ease::OUT_CUBIC);
/// How long a refused command keeps its reason on the map.
const REFUSAL_MS: u16 = 2_000;

/// A flash that has already played out.
fn still() -> Tween {
    Tween::preset(Fixed::ONE, Fixed::ONE, CUE)
}

/// What a forecast starts from.
#[derive(Clone, Copy)]
struct Flight {
    body: OrbitBody,
    fuel: Fixed64,
    heat: Fixed64,
    time: Fixed64,
}

impl Flight {
    /// Pays for a burn, or reports what stops it, leaving the flight unchanged.
    fn spend(&mut self, dv_tenths: u8) -> Result<Fixed64, OrbitError> {
        let dv = Fixed64::from_ratio(i64::from(dv_tenths), 10);
        let fuel_cost = dv * 2;
        let heat_cost = dv * 7;
        if self.fuel < fuel_cost {
            return Err(OrbitError::Fuel);
        }
        if self.heat + heat_cost > Fixed64::from_int(MAX_HEAT) {
            return Err(OrbitError::Heat);
        }
        self.fuel -= fuel_cost;
        self.heat += heat_cost;
        Ok(dv)
    }

    fn burn(&mut self, dv_tenths: u8, angle_degrees: i16) -> Result<(), OrbitError> {
        let dv = self.spend(dv_tenths)?;
        impulse(&mut self.body, dv, angle_degrees);
        Ok(())
    }

    fn coast(&mut self, dt: Fixed64) {
        integrate(&mut self.body, dt);
        self.heat = (self.heat - dt * 4).max(Fixed64::ZERO);
        self.time += dt;
    }
}

/// Flies `flight` ahead one tenth of a second per point: the planned nodes
/// fire at their time and `marks[k]` records the point where node `k` fired.
fn forecast(
    mut flight: Flight,
    queue: &[Option<ManeuverNode>],
    points: &mut [TrackPoint; MAX_PREVIEW],
    marks: &mut [u8; MAX_NODES],
) -> u8 {
    *marks = [UNMARKED; MAX_NODES];
    let mut next = 0;
    let mut len = 0;
    for (index, point) in points.iter_mut().enumerate() {
        let end = flight.time + FORECAST_STEP;
        while let Some(Some(node)) = queue.get(next)
            && node.at <= end
        {
            flight.coast((node.at - flight.time).max(Fixed64::ZERO));
            if flight.burn(node.dv_tenths, node.angle_degrees).is_ok() {
                marks[next] = index as u8;
            }
            next += 1;
        }
        flight.coast(end - flight.time);
        *point = TrackPoint::from_point(flight.body.position);
        len = index + 1;
        let radius = flight.body.radius();
        if radius < Fixed64::from_int(CRASH_RADIUS) || radius > Fixed64::from_int(ESCAPE_RADIUS) {
            break;
        }
    }
    len as u8
}

fn start_body() -> OrbitBody {
    let radius = Fixed64::from_int(START_RADIUS);
    OrbitBody {
        position: OrbitPoint {
            x: radius,
            y: Fixed64::ZERO,
        },
        velocity: OrbitPoint {
            x: Fixed64::ZERO,
            y: (MU / radius).sqrt(),
        },
    }
}

#[crate::model(
    change = ChangeSet,
    watch(visual = ChangeSet::VISUAL),
    persistence(key = "orbital_mission/progress")
)]
pub(crate) struct OrbitModel {
    pub(super) body: OrbitBody,
    trail: [TrackPoint; MAX_TRAIL],
    telemetry: [OrbitSample; MAX_TELEMETRY],
    events: [OrbitEvent; MAX_EVENTS],
    #[observe]
    pub(crate) queue: [Option<ManeuverNode>; MAX_NODES],
    clock: BoundedClock,
    #[observe]
    pub(crate) time: Fixed64,
    #[observe]
    pub(crate) fuel: Fixed64,
    #[observe]
    pub(crate) heat: Fixed64,
    next_record: Fixed64,
    records: u8,
    #[observe]
    pub(crate) page: OrbitPage,
    #[observe]
    pub(crate) modal: OrbitModal,
    #[observe]
    pub(crate) status: OrbitStatus,
    #[observe]
    pub(crate) mission: u8,
    /// Goals sampled in this flight.
    #[observe]
    pub(crate) completed: u8,
    /// One bit per mission won; the only state kept between sessions.
    #[persist]
    #[observe]
    pub(crate) cleared: u8,
    /// The craft was inside the current goal's zone at the last step.
    zone_latch: bool,
    #[observe]
    pub(crate) dv_tenths: u8,
    #[observe]
    pub(crate) angle_degrees: i16,
    #[observe]
    pub(crate) delay_seconds: u8,
    #[observe]
    pub(crate) warp: u8,
    next_node_id: u8,
    #[observe]
    pub(crate) queue_len: u8,
    trail_start: u8,
    trail_len: u8,
    telemetry_start: u8,
    #[observe]
    pub(crate) telemetry_len: u8,
    event_start: u8,
    event_len: u8,
    #[observe]
    pub(crate) preview: bool,
    #[observe]
    pub(crate) hint_shown: bool,
    aim: [TrackPoint; MAX_PREVIEW],
    aim_len: u8,
    aim_at: Fixed64,
    aim_body: OrbitBody,
    aim_dv_tenths: u8,
    aim_angle_degrees: i16,
    coast: [TrackPoint; MAX_PREVIEW],
    coast_len: u8,
    coast_at: Fixed64,
    coast_marks: [u8; MAX_NODES],
    coast_stale: bool,
    /// The last flash on the map, where it happened and how far it has played.
    cue: OrbitCue,
    cue_at: TrackPoint,
    cue_motion: Tween,
    /// Why the last command from a button or key was refused.
    #[observe]
    pub(crate) refusal: Option<OrbitError>,
    refusal_ms: u16,
}

impl Default for OrbitModel {
    fn default() -> Self {
        let mut model = Self {
            body: start_body(),
            trail: [TrackPoint::default(); MAX_TRAIL],
            telemetry: [OrbitSample::default(); MAX_TELEMETRY],
            events: [OrbitEvent::default(); MAX_EVENTS],
            queue: [None; MAX_NODES],
            clock: BoundedClock::new(60, 100, 6),
            time: Fixed64::ZERO,
            fuel: Fixed64::ZERO,
            heat: Fixed64::ZERO,
            next_record: Fixed64::ZERO,
            records: 0,
            page: OrbitPage::Map,
            modal: OrbitModal::None,
            status: OrbitStatus::Ready,
            mission: 0,
            completed: 0,
            cleared: 0,
            zone_latch: false,
            dv_tenths: START_DV_TENTHS,
            angle_degrees: 0,
            delay_seconds: START_DELAY_SECONDS,
            warp: 1,
            next_node_id: 1,
            queue_len: 0,
            trail_start: 0,
            trail_len: 0,
            telemetry_start: 0,
            telemetry_len: 0,
            event_start: 0,
            event_len: 0,
            preview: true,
            hint_shown: false,
            aim: [TrackPoint::default(); MAX_PREVIEW],
            aim_len: 0,
            aim_at: Fixed64::ZERO,
            aim_body: OrbitBody::default(),
            aim_dv_tenths: 0,
            aim_angle_degrees: 0,
            coast: [TrackPoint::default(); MAX_PREVIEW],
            coast_len: 0,
            coast_at: Fixed64::ZERO,
            coast_marks: [UNMARKED; MAX_NODES],
            coast_stale: true,
            cue: OrbitCue::Burn,
            cue_at: TrackPoint::default(),
            cue_motion: still(),
            refusal: None,
            refusal_ms: 0,
        };
        model.load_mission_raw(0);
        model
    }
}

impl OrbitModel {
    /// Restarts `mission`. The dial, warp and hint are kept when the mission
    /// is the one already loaded, so a retry starts from the last attempt.
    fn load_mission_raw(&mut self, mission: u8) -> ChangeSet {
        let mission = mission.min((MISSION_COUNT - 1) as u8);
        if mission != self.mission {
            self.dv_tenths = START_DV_TENTHS;
            self.angle_degrees = 0;
            self.delay_seconds = START_DELAY_SECONDS;
            self.warp = 1;
            self.hint_shown = false;
        }
        self.mission = mission;
        self.body = start_body();
        self.trail = [TrackPoint::default(); MAX_TRAIL];
        self.telemetry = [OrbitSample::default(); MAX_TELEMETRY];
        self.events = [OrbitEvent::default(); MAX_EVENTS];
        self.queue = [None; MAX_NODES];
        self.clock.reset();
        self.time = Fixed64::ZERO;
        self.fuel = Fixed64::from_int(i64::from(MISSIONS[usize::from(mission)].fuel));
        self.heat = Fixed64::ZERO;
        self.next_record = Fixed64::ZERO;
        self.records = 0;
        self.page = OrbitPage::Map;
        self.modal = OrbitModal::None;
        self.status = OrbitStatus::Ready;
        self.completed = 0;
        self.zone_latch = false;
        self.next_node_id = 1;
        self.queue_len = 0;
        self.trail_start = 0;
        self.trail_len = 0;
        self.telemetry_start = 0;
        self.telemetry_len = 0;
        self.event_start = 0;
        self.event_len = 0;
        self.cue_motion = still();
        self.refusal = None;
        self.refusal_ms = 0;
        self.push_event(OrbitEventKind::Loaded);
        self.refresh_forecasts(true);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// Loads another mission and opens its briefing.
    fn switch_raw(&mut self, mission: u8) -> ChangeSet {
        let changes = self.load_mission_raw(mission);
        self.modal = OrbitModal::Brief;
        changes
    }

    fn update_raw(&mut self, elapsed_ms: u16) -> ChangeSet {
        let feedback = self.play_feedback(elapsed_ms);
        let active = self.status == OrbitStatus::Running && self.modal == OrbitModal::None;
        let steps = self.clock.steps(elapsed_ms, active);
        if steps == 0 {
            return feedback;
        }
        let display_tenth = (self.time * 10).to_int();
        let fuel = self.fuel;
        let status = self.status;
        let queue_len = self.queue_len;
        let iterations = usize::from(steps) * usize::from(self.warp);
        for _ in 0..iterations {
            if !self.step() {
                break;
            }
            if self.arrived() {
                self.status = OrbitStatus::Paused;
                self.flash(OrbitCue::Arrive);
                break;
            }
        }
        self.refresh_forecasts(false);
        if (self.time * 10).to_int() != display_tenth
            || self.fuel != fuel
            || self.status != status
            || self.queue_len != queue_len
        {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL | feedback
        }
    }

    /// Plays the map flash and lets a refusal run out.
    fn play_feedback(&mut self, elapsed_ms: u16) -> ChangeSet {
        let mut changes = ChangeSet::NONE;
        if elapsed_ms > 0 && !self.cue_motion.is_finished() {
            self.cue_motion.tick(elapsed_ms);
            changes = ChangeSet::VISUAL;
        }
        if self.refusal_ms > 0 {
            self.refusal_ms = self.refusal_ms.saturating_sub(elapsed_ms);
            if self.refusal_ms == 0 {
                self.refusal = None;
                changes = changes | ChangeSet::MODEL;
            }
        }
        changes
    }

    /// Starts the flash for `cue` where the craft is now.
    fn flash(&mut self, cue: OrbitCue) {
        self.cue = cue;
        self.cue_at = TrackPoint::from_point(self.body.position);
        self.cue_motion = Tween::preset(Fixed::ZERO, Fixed::ONE, CUE);
    }

    /// Shows why a command from a button or key did nothing.
    fn refuse(&mut self, error: OrbitError) -> ChangeSet {
        self.refusal = Some(error);
        self.refusal_ms = REFUSAL_MS;
        ChangeSet::MODEL
    }

    /// Clears the reason of an earlier refusal once a command goes through.
    fn accept(&mut self, result: Result<ChangeSet, OrbitError>) -> ChangeSet {
        match result {
            Ok(changes) => {
                self.refusal = None;
                self.refusal_ms = 0;
                changes
            }
            Err(error) => self.refuse(error),
        }
    }

    /// True on the step the craft enters the current goal's zone.
    fn arrived(&mut self) -> bool {
        let inside = self.in_zone_raw();
        let entered = inside && !self.zone_latch;
        self.zone_latch = inside;
        entered
    }

    fn toggle_running_raw(&mut self) -> ChangeSet {
        if self.status.terminal() {
            return ChangeSet::NONE;
        }
        self.status = if self.status == OrbitStatus::Running {
            OrbitStatus::Paused
        } else {
            OrbitStatus::Running
        };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn flight(&self) -> Flight {
        Flight {
            body: self.body,
            fuel: self.fuel,
            heat: self.heat,
            time: self.time,
        }
    }

    fn apply_flight(&mut self, flight: Flight) {
        self.body = flight.body;
        self.fuel = flight.fuel;
        self.heat = flight.heat;
    }

    fn contact_raw(&self) -> bool {
        let seconds = self.mission().contact_seconds;
        seconds == 0 || self.time < Fixed64::from_int(i64::from(seconds))
    }

    /// Why a burn with the dial's values cannot fire now.
    fn burn_block_raw(&self) -> Option<OrbitError> {
        if self.status.terminal() {
            return Some(OrbitError::Terminal);
        }
        if !self.contact_raw() {
            return Some(OrbitError::NoContact);
        }
        self.flight().spend(self.dv_tenths).err()
    }

    fn burn_raw(&mut self) -> Result<ChangeSet, OrbitError> {
        if let Some(error) = self.burn_block_raw() {
            return Err(error);
        }
        let mut flight = self.flight();
        flight.burn(self.dv_tenths, self.angle_degrees)?;
        self.apply_flight(flight);
        self.flash(OrbitCue::Burn);
        self.push_event(OrbitEventKind::Burn(self.dv_tenths));
        self.refresh_forecasts(true);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    fn schedule_block_raw(&self) -> Option<OrbitError> {
        if self.status.terminal() {
            Some(OrbitError::Terminal)
        } else if !self.contact_raw() {
            Some(OrbitError::NoContact)
        } else if usize::from(self.queue_len) >= MAX_NODES {
            Some(OrbitError::QueueFull)
        } else if !(1..=30).contains(&self.delay_seconds) {
            Some(OrbitError::InvalidDelay)
        } else {
            None
        }
    }

    fn schedule_raw(&mut self) -> Result<ChangeSet, OrbitError> {
        if let Some(error) = self.schedule_block_raw() {
            return Err(error);
        }
        let node = ManeuverNode {
            id: self.next_node_id,
            at: self.time + Fixed64::from_int(i64::from(self.delay_seconds)),
            dv_tenths: self.dv_tenths,
            angle_degrees: self.angle_degrees,
        };
        self.next_node_id = self.next_node_id.wrapping_add(1).max(1);
        let mut index = usize::from(self.queue_len);
        while index > 0 {
            let previous = self.queue[index - 1].expect("queue prefix is dense");
            if previous.at < node.at || (previous.at == node.at && previous.id < node.id) {
                break;
            }
            self.queue[index] = Some(previous);
            index -= 1;
        }
        self.queue[index] = Some(node);
        self.queue_len += 1;
        self.refresh_forecasts(true);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    fn remove_node(&mut self, id: u8) -> Result<(), OrbitError> {
        let Some(index) = self.queue[..usize::from(self.queue_len)]
            .iter()
            .position(|node| node.is_some_and(|node| node.id == id))
        else {
            return Err(OrbitError::MissingNode);
        };
        for next in index + 1..usize::from(self.queue_len) {
            self.queue[next - 1] = self.queue[next];
        }
        self.queue_len -= 1;
        self.queue[usize::from(self.queue_len)] = None;
        self.coast_stale = true;
        Ok(())
    }

    pub(super) fn cancel_raw(&mut self, id: u8) -> Result<ChangeSet, OrbitError> {
        if !self.contact_raw() {
            return Err(OrbitError::NoContact);
        }
        self.remove_node(id)?;
        self.refresh_forecasts(true);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    fn scan_raw(&mut self) -> Result<ChangeSet, OrbitError> {
        if self.status.terminal() {
            return Err(OrbitError::Terminal);
        }
        if !self.eligible_raw() {
            return Err(OrbitError::OutsideWindow);
        }
        self.completed += 1;
        self.zone_latch = false;
        self.flash(OrbitCue::Sample);
        self.push_event(OrbitEventKind::Goal(self.completed));
        if self.goal().is_none() {
            self.status = OrbitStatus::Won;
            self.cleared |= 1 << self.mission;
            self.queue = [None; MAX_NODES];
            self.queue_len = 0;
            self.modal = OrbitModal::Result;
            self.push_event(OrbitEventKind::Won);
        }
        self.refresh_forecasts(true);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    /// Advances one simulation step; false once the flight has ended.
    pub(crate) fn step(&mut self) -> bool {
        if self.status.terminal() {
            return false;
        }
        self.time += STEP;
        while let Some(node) = self.queue[0]
            && node.at <= self.time
        {
            let _ = self.remove_node(node.id);
            let mut flight = self.flight();
            match flight.burn(node.dv_tenths, node.angle_degrees) {
                Ok(()) => {
                    self.apply_flight(flight);
                    self.flash(OrbitCue::Burn);
                    self.push_event(OrbitEventKind::NodeBurn(node.dv_tenths));
                }
                Err(error) => self.push_event(OrbitEventKind::NodeSkipped(error)),
            }
        }
        integrate(&mut self.body, STEP);
        self.heat = (self.heat - STEP * 4).max(Fixed64::ZERO);
        let radius = self.body.radius();
        if radius < Fixed64::from_int(CRASH_RADIUS) {
            self.end(OrbitStatus::Crashed, OrbitEventKind::Crashed);
        } else if radius > Fixed64::from_int(ESCAPE_RADIUS) {
            self.end(OrbitStatus::Escaped, OrbitEventKind::Escaped);
        }
        if self.time >= self.next_record {
            self.next_record = self.time + RECORD_INTERVAL;
            self.record(radius);
        }
        true
    }

    fn end(&mut self, status: OrbitStatus, kind: OrbitEventKind) {
        self.status = status;
        self.queue = [None; MAX_NODES];
        self.queue_len = 0;
        self.aim_len = 0;
        self.coast_len = 0;
        self.modal = OrbitModal::Result;
        if status == OrbitStatus::Crashed {
            self.flash(OrbitCue::Crash);
        }
        self.push_event(kind);
    }

    fn record(&mut self, radius: Fixed64) {
        ring_push(
            &mut self.trail,
            &mut self.trail_start,
            &mut self.trail_len,
            TrackPoint::from_point(self.body.position),
        );
        if self.records % TELEMETRY_EVERY == 0 {
            let tenths =
                |value: Fixed64| (value * 10).to_int().clamp(0, i64::from(u16::MAX)) as u16;
            ring_push(
                &mut self.telemetry,
                &mut self.telemetry_start,
                &mut self.telemetry_len,
                OrbitSample {
                    radius_tenths: tenths(radius),
                    speed_tenths: tenths(self.body.speed()),
                },
            );
        }
        self.records = self.records.wrapping_add(1);
    }

    /// Recomputes the burn preview and the planned coast when they are out
    /// of date; `force` recomputes both.
    fn refresh_forecasts(&mut self, force: bool) {
        let aim_due = force
            || self.aim_dv_tenths != self.dv_tenths
            || self.aim_angle_degrees != self.angle_degrees
            || (self.aim_body != self.body && self.time - self.aim_at >= AIM_REFRESH);
        if aim_due {
            let mut flight = self.flight();
            self.aim_len = if self.burn_block_raw().is_none()
                && flight.burn(self.dv_tenths, self.angle_degrees).is_ok()
            {
                let mut marks = [UNMARKED; MAX_NODES];
                forecast(
                    flight,
                    &self.queue[..usize::from(self.queue_len)],
                    &mut self.aim,
                    &mut marks,
                )
            } else {
                0
            };
            self.aim_at = self.time;
            self.aim_body = self.body;
            self.aim_dv_tenths = self.dv_tenths;
            self.aim_angle_degrees = self.angle_degrees;
        }
        if force || self.coast_stale || self.time - self.coast_at >= COAST_REFRESH {
            self.coast_len = if self.status.terminal() {
                0
            } else {
                forecast(
                    self.flight(),
                    &self.queue[..usize::from(self.queue_len)],
                    &mut self.coast,
                    &mut self.coast_marks,
                )
            };
            self.coast_at = self.time;
            self.coast_stale = false;
        }
    }

    fn goal(&self) -> Option<OrbitGoal> {
        self.mission().goal(usize::from(self.completed))
    }

    /// The craft is where the current goal is, at any speed.
    fn in_zone_raw(&self) -> bool {
        match self.goal() {
            Some(OrbitGoal::Band { min, max }) => {
                let radius = self.body.radius();
                radius >= Fixed64::from_int(i64::from(min))
                    && radius <= Fixed64::from_int(i64::from(max))
            }
            Some(OrbitGoal::Beacon { x, y, range, .. }) => {
                let offset = OrbitPoint {
                    x: self.body.position.x - Fixed64::from_int(i64::from(x)),
                    y: self.body.position.y - Fixed64::from_int(i64::from(y)),
                };
                offset.radius() <= Fixed64::from_int(i64::from(range))
            }
            None => false,
        }
    }

    fn eligible_raw(&self) -> bool {
        self.in_zone_raw()
            && match self.goal() {
                Some(OrbitGoal::Beacon { max_speed, .. }) => {
                    self.body.speed() <= Fixed64::from_int(i64::from(max_speed))
                }
                _ => true,
            }
    }

    /// The flight has been started and would be lost by loading a mission.
    fn in_progress(&self) -> bool {
        !self.status.terminal()
            && (self.time > Fixed64::ZERO
                || self.queue_len > 0
                || self.fuel < Fixed64::from_int(i64::from(self.mission().fuel)))
    }

    pub(super) fn push_event(&mut self, kind: OrbitEventKind) {
        ring_push(
            &mut self.events,
            &mut self.event_start,
            &mut self.event_len,
            OrbitEvent {
                time: self.time,
                kind,
            },
        );
    }

    fn set_page_raw(&mut self, page: OrbitPage) -> ChangeSet {
        if self.page == page {
            return ChangeSet::NONE;
        }
        self.page = page;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn set_modal_raw(&mut self, modal: OrbitModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn adjust_dv_raw(&mut self, delta_tenths: i8) -> ChangeSet {
        let dv_tenths = (i16::from(self.dv_tenths) + i16::from(delta_tenths)).clamp(2, 80) as u8;
        if self.dv_tenths == dv_tenths {
            return ChangeSet::NONE;
        }
        self.dv_tenths = dv_tenths;
        self.refresh_forecasts(false);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// Turns the burn direction, wrapping past ±180°.
    fn set_angle_raw(&mut self, angle_degrees: i16) -> ChangeSet {
        let wrapped = (angle_degrees + 180).rem_euclid(360) - 180;
        let angle_degrees = if wrapped == -180 { 180 } else { wrapped };
        if self.angle_degrees == angle_degrees {
            return ChangeSet::NONE;
        }
        self.angle_degrees = angle_degrees;
        self.refresh_forecasts(false);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn adjust_delay_raw(&mut self, delta: i8) -> ChangeSet {
        let delay_seconds = (i16::from(self.delay_seconds) + i16::from(delta)).clamp(1, 30) as u8;
        if self.delay_seconds == delay_seconds {
            return ChangeSet::NONE;
        }
        self.delay_seconds = delay_seconds;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    #[cfg(test)]
    pub(crate) fn load_mission(&mut self, mission: u8) -> ChangeSet {
        self.load_mission_raw(mission)
    }

    pub(crate) const fn page(&self) -> OrbitPage {
        self.page
    }
    pub(crate) const fn modal(&self) -> OrbitModal {
        self.modal
    }
    pub(crate) const fn status(&self) -> OrbitStatus {
        self.status
    }
    #[cfg(test)]
    pub(crate) const fn mission_index(&self) -> u8 {
        self.mission
    }
    pub(crate) const fn mission(&self) -> OrbitMission {
        MISSIONS[self.mission as usize]
    }
    pub(crate) const fn completed(&self) -> usize {
        self.completed as usize
    }
    pub(crate) const fn body(&self) -> OrbitBody {
        self.body
    }
    #[cfg(test)]
    pub(crate) const fn time(&self) -> Fixed64 {
        self.time
    }
    pub(crate) const fn fuel(&self) -> Fixed64 {
        self.fuel
    }
    pub(crate) const fn heat(&self) -> Fixed64 {
        self.heat
    }
    pub(crate) const fn dv_tenths(&self) -> u8 {
        self.dv_tenths
    }
    pub(crate) const fn angle_degrees(&self) -> i16 {
        self.angle_degrees
    }
    pub(crate) const fn preview(&self) -> bool {
        self.preview
    }
    #[cfg(test)]
    pub(crate) const fn queue_len(&self) -> usize {
        self.queue_len as usize
    }
    #[cfg(test)]
    pub(crate) const fn cleared_mask(&self) -> u8 {
        self.cleared
    }
    pub(crate) const fn trail_len(&self) -> usize {
        self.trail_len as usize
    }
    pub(crate) const fn telemetry_len(&self) -> usize {
        self.telemetry_len as usize
    }
    pub(crate) const fn event_len(&self) -> usize {
        self.event_len as usize
    }

    pub(crate) fn queue(&self, index: usize) -> Option<ManeuverNode> {
        self.queue.get(index).copied().flatten()
    }

    pub(crate) fn trail(&self, index: usize) -> Option<TrackPoint> {
        ring_get(&self.trail, self.trail_start, self.trail_len, index)
    }

    pub(crate) fn telemetry(&self, index: usize) -> Option<OrbitSample> {
        ring_get(
            &self.telemetry,
            self.telemetry_start,
            self.telemetry_len,
            index,
        )
    }

    pub(crate) fn event(&self, index: usize) -> Option<OrbitEvent> {
        ring_get(&self.events, self.event_start, self.event_len, index)
    }

    /// Where the craft goes if the dial's burn fires now, then the plan.
    pub(crate) fn aim(&self) -> &[TrackPoint] {
        &self.aim[..usize::from(self.aim_len)]
    }

    /// The forecast points the craft has already passed.
    fn coast_passed(&self) -> usize {
        ((self.time - self.coast_at) / FORECAST_STEP)
            .to_int()
            .clamp(0, i64::from(self.coast_len)) as usize
    }

    /// Where the craft goes from here with the plan as it stands.
    pub(crate) fn coast(&self) -> &[TrackPoint] {
        &self.coast[self.coast_passed()..usize::from(self.coast_len)]
    }

    /// Where each planned node fires on the coast, if it fires within it.
    pub(crate) fn node_mark(&self, index: usize) -> Option<TrackPoint> {
        let mark = usize::from(*self.coast_marks.get(index)?);
        (mark >= self.coast_passed() && mark < usize::from(self.coast_len))
            .then(|| self.coast[mark])
    }

    /// The flash playing on the map: what, where, and how far it has played
    /// from 0 to 1.
    pub(crate) fn cue_flash(&self) -> Option<(OrbitCue, TrackPoint, Fixed)> {
        (!self.cue_motion.is_finished()).then(|| (self.cue, self.cue_at, self.cue_motion.value()))
    }

    fn confirm_raw(&mut self) -> ChangeSet {
        match self.modal {
            OrbitModal::Confirm(mission) if mission == self.mission => {
                self.load_mission_raw(mission)
            }
            OrbitModal::Confirm(mission) => self.switch_raw(mission),
            _ => ChangeSet::NONE,
        }
    }

    fn has_next_raw(&self) -> bool {
        usize::from(self.mission) + 1 < MISSION_COUNT
    }

    fn next_mission_raw(&mut self) -> ChangeSet {
        if self.status == OrbitStatus::Won && self.has_next_raw() {
            self.switch_raw(self.mission + 1)
        } else {
            ChangeSet::NONE
        }
    }

    fn toggle_hint_raw(&mut self) -> ChangeSet {
        self.hint_shown = !self.hint_shown;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }
}

#[crate::model]
impl OrbitModel {
    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        self.update_raw(elapsed_ms)
    }

    #[cfg(test)]
    pub(crate) fn toggle_running(&mut self) -> ChangeSet {
        self.toggle_running_raw()
    }

    #[cfg(test)]
    pub(crate) fn burn(&mut self) -> Result<ChangeSet, OrbitError> {
        self.burn_raw()
    }

    #[cfg(test)]
    pub(crate) fn schedule(&mut self) -> Result<ChangeSet, OrbitError> {
        self.schedule_raw()
    }

    pub(crate) fn cancel_at(&mut self, index: usize) -> Result<ChangeSet, OrbitError> {
        let Some(node) = self.queue(index) else {
            return Err(OrbitError::MissingNode);
        };
        self.cancel_raw(node.id)
    }

    #[cfg(test)]
    pub(crate) fn scan(&mut self) -> Result<ChangeSet, OrbitError> {
        self.scan_raw()
    }

    /// Burns with the dial now, or shows why it cannot.
    pub(crate) fn press_burn(&mut self) -> ChangeSet {
        let result = self.burn_raw();
        self.accept(result)
    }

    /// Samples the current goal, or shows why it cannot.
    pub(crate) fn press_scan(&mut self) -> ChangeSet {
        let result = self.scan_raw();
        self.accept(result)
    }

    /// Plans a node from the dial and the delay, or shows why it cannot.
    pub(crate) fn press_schedule(&mut self) -> ChangeSet {
        let result = self.schedule_raw();
        self.accept(result)
    }

    /// The first footer button: coast or pause, or the next mission once won.
    pub(crate) fn press_run(&mut self) -> ChangeSet {
        if self.status == OrbitStatus::Won {
            self.next_mission_raw()
        } else {
            self.toggle_running_raw()
        }
    }

    /// The main button of the open dialog.
    pub(crate) fn modal_primary(&mut self) -> ChangeSet {
        match self.modal {
            OrbitModal::Confirm(_) => self.confirm_raw(),
            OrbitModal::Result if self.status == OrbitStatus::Won && self.has_next_raw() => {
                self.next_mission_raw()
            }
            OrbitModal::Result if self.status == OrbitStatus::Won => {
                self.set_modal_raw(OrbitModal::Missions)
            }
            OrbitModal::Result => self.load_mission_raw(self.mission),
            _ => self.set_modal_raw(OrbitModal::None),
        }
    }

    /// The second button of the open dialog.
    pub(crate) fn modal_secondary(&mut self) -> ChangeSet {
        match self.modal {
            OrbitModal::Brief => self.toggle_hint_raw(),
            OrbitModal::Result if self.status == OrbitStatus::Won => {
                self.load_mission_raw(self.mission)
            }
            OrbitModal::Result => self.set_modal_raw(OrbitModal::Missions),
            _ => self.set_modal_raw(OrbitModal::None),
        }
    }

    pub(crate) fn set_page(&mut self, page: OrbitPage) -> ChangeSet {
        self.set_page_raw(page)
    }

    pub(crate) fn set_modal(&mut self, modal: OrbitModal) -> ChangeSet {
        self.set_modal_raw(modal)
    }

    /// Picks a mission from the list; a flight in progress asks first.
    pub(crate) fn choose_mission(&mut self, mission: u8) -> ChangeSet {
        if usize::from(mission) >= MISSION_COUNT {
            ChangeSet::NONE
        } else if self.in_progress() {
            self.set_modal_raw(OrbitModal::Confirm(mission))
        } else {
            self.switch_raw(mission)
        }
    }

    /// Accepts the pending confirmation.
    #[cfg(test)]
    pub(crate) fn confirm(&mut self) -> ChangeSet {
        self.confirm_raw()
    }

    /// Starts the mission over; a flight in progress asks first.
    pub(crate) fn restart(&mut self) -> ChangeSet {
        if self.in_progress() {
            self.set_modal_raw(OrbitModal::Confirm(self.mission))
        } else if self.status.terminal() {
            self.load_mission_raw(self.mission)
        } else {
            ChangeSet::NONE
        }
    }

    #[cfg(test)]
    pub(crate) fn next_mission(&mut self) -> ChangeSet {
        self.next_mission_raw()
    }

    #[cfg(test)]
    pub(crate) fn toggle_hint(&mut self) -> ChangeSet {
        self.toggle_hint_raw()
    }

    pub(crate) fn adjust_dv(&mut self, delta_tenths: i8) -> ChangeSet {
        self.adjust_dv_raw(delta_tenths)
    }

    pub(crate) fn adjust_angle(&mut self, delta: i16) -> ChangeSet {
        self.set_angle_raw(self.angle_degrees + delta)
    }

    /// Swaps between burning with and against the motion.
    pub(crate) fn flip_angle(&mut self) -> ChangeSet {
        self.set_angle_raw(self.angle_degrees + 180)
    }

    pub(crate) fn adjust_delay(&mut self, delta: i8) -> ChangeSet {
        self.adjust_delay_raw(delta)
    }

    pub(crate) fn toggle_preview(&mut self) -> ChangeSet {
        self.preview = !self.preview;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn cycle_warp(&mut self) -> ChangeSet {
        self.warp = match self.warp {
            1 => 2,
            2 => 4,
            _ => 1,
        };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    #[observe]
    pub(crate) fn radius(&self) -> Fixed64 {
        self.body.radius()
    }

    #[observe]
    pub(crate) fn speed(&self) -> Fixed64 {
        self.body.speed()
    }

    #[observe]
    pub(crate) fn eligible(&self) -> bool {
        self.eligible_raw()
    }

    #[observe]
    pub(crate) fn in_zone(&self) -> bool {
        self.in_zone_raw()
    }

    #[observe]
    pub(crate) fn current_goal(&self) -> Option<OrbitGoal> {
        self.goal()
    }

    #[observe]
    pub(crate) fn burn_block(&self) -> Option<OrbitError> {
        self.burn_block_raw()
    }

    #[observe]
    pub(crate) fn schedule_block(&self) -> Option<OrbitError> {
        self.schedule_block_raw()
    }

    #[observe]
    pub(crate) fn can_restart(&self) -> bool {
        self.in_progress() || self.status.terminal()
    }

    /// The fuel the planned nodes will burn, in tenths.
    #[observe]
    pub(crate) fn planned_fuel(&self) -> u16 {
        self.queue
            .iter()
            .flatten()
            .map(|node| u16::from(node.dv_tenths) * 2)
            .sum()
    }

    #[observe]
    pub(crate) fn has_next(&self) -> bool {
        self.has_next_raw()
    }

    /// Tenths of a second of contact left, when the mission has a limit.
    #[observe]
    pub(crate) fn contact_left(&self) -> Option<u16> {
        let seconds = self.mission().contact_seconds;
        (seconds != 0).then(|| {
            let left = Fixed64::from_int(i64::from(seconds)) - self.time;
            (left * 10).to_int().max(0) as u16
        })
    }

    /// The latest events, oldest first.
    #[observe]
    pub(crate) fn log(&self) -> [Option<OrbitEvent>; LOG_LINES] {
        let skip = self.event_len().saturating_sub(LOG_LINES);
        core::array::from_fn(|line| self.event(skip + line))
    }
}

fn ring_push<T: Copy, const N: usize>(values: &mut [T; N], start: &mut u8, len: &mut u8, value: T) {
    if usize::from(*len) < N {
        let index = (usize::from(*start) + usize::from(*len)) % N;
        values[index] = value;
        *len += 1;
    } else {
        values[usize::from(*start)] = value;
        *start = ((*start as usize + 1) % N) as u8;
    }
}

fn ring_get<T: Copy, const N: usize>(
    values: &[T; N],
    start: u8,
    len: u8,
    index: usize,
) -> Option<T> {
    if index >= usize::from(len) {
        return None;
    }
    Some(values[(usize::from(start) + index) % N])
}
