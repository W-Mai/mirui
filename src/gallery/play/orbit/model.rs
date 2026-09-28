use super::missions::{MISSION_COUNT, MISSIONS, OrbitGoal, OrbitMission};
use super::physics::{MU, STEP, impulse, integrate};
use super::types::{
    MAX_EVENTS, MAX_NODES, MAX_PREVIEW, MAX_TELEMETRY, MAX_TRAIL, ManeuverNode, OrbitBody,
    OrbitError, OrbitEvent, OrbitEventKind, OrbitModal, OrbitPage, OrbitPoint, OrbitStatus,
    OrbitTelemetry,
};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::types::Fixed64;

const RECORD_INTERVAL: Fixed64 = Fixed64::from_ratio(4, 25);

#[derive(Clone, Copy, Debug)]
pub(crate) struct OrbitModel {
    pub(super) body: OrbitBody,
    pub(super) trail: [OrbitPoint; MAX_TRAIL],
    pub(super) telemetry: [OrbitTelemetry; MAX_TELEMETRY],
    pub(super) events: [OrbitEvent; MAX_EVENTS],
    pub(super) queue: [Option<ManeuverNode>; MAX_NODES],
    pub(super) clock: BoundedClock,
    pub(super) time: Fixed64,
    pub(super) fuel: Fixed64,
    pub(super) heat: Fixed64,
    pub(super) next_record: Fixed64,
    pub(super) page: OrbitPage,
    pub(super) modal: OrbitModal,
    pub(super) status: OrbitStatus,
    pub(super) mission: u8,
    pub(super) completed: u8,
    pub(super) dv_tenths: u8,
    pub(super) angle_degrees: i16,
    pub(super) delay_seconds: u8,
    pub(super) warp: u8,
    pub(super) next_node_id: u8,
    pub(super) queue_len: u8,
    pub(super) trail_start: u8,
    pub(super) trail_len: u8,
    pub(super) telemetry_start: u8,
    pub(super) telemetry_len: u8,
    pub(super) event_start: u8,
    pub(super) event_len: u8,
    pub(super) preview: bool,
}

impl Default for OrbitModel {
    fn default() -> Self {
        let mut model = Self {
            body: OrbitBody::default(),
            trail: [OrbitPoint::default(); MAX_TRAIL],
            telemetry: [OrbitTelemetry::default(); MAX_TELEMETRY],
            events: [OrbitEvent::default(); MAX_EVENTS],
            queue: [None; MAX_NODES],
            clock: BoundedClock::new(60, 100, 6),
            time: Fixed64::ZERO,
            fuel: Fixed64::ZERO,
            heat: Fixed64::ZERO,
            next_record: Fixed64::ZERO,
            page: OrbitPage::Map,
            modal: OrbitModal::None,
            status: OrbitStatus::Ready,
            mission: 0,
            completed: 0,
            dv_tenths: 38,
            angle_degrees: 0,
            delay_seconds: 3,
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
        };
        model.load_mission(0);
        model
    }
}

impl OrbitModel {
    pub(crate) fn load_mission(&mut self, mission: u8) -> ChangeSet {
        let mission = mission.min((MISSION_COUNT - 1) as u8);
        self.body = OrbitBody {
            position: OrbitPoint {
                x: Fixed64::from_int(78),
                y: Fixed64::ZERO,
            },
            velocity: OrbitPoint {
                x: Fixed64::ZERO,
                y: (MU / Fixed64::from_int(78)).sqrt(),
            },
        };
        self.trail = [OrbitPoint::default(); MAX_TRAIL];
        self.telemetry = [OrbitTelemetry::default(); MAX_TELEMETRY];
        self.events = [OrbitEvent::default(); MAX_EVENTS];
        self.queue = [None; MAX_NODES];
        self.clock.reset();
        self.time = Fixed64::ZERO;
        self.fuel = Fixed64::from_int(48);
        self.heat = Fixed64::ZERO;
        self.next_record = Fixed64::ZERO;
        self.page = OrbitPage::Map;
        self.modal = OrbitModal::None;
        self.status = OrbitStatus::Ready;
        self.mission = mission;
        self.completed = 0;
        self.dv_tenths = MISSIONS[usize::from(mission)].default_dv_tenths;
        self.angle_degrees = 0;
        self.delay_seconds = 3;
        self.warp = 1;
        self.next_node_id = 1;
        self.queue_len = 0;
        self.trail_start = 0;
        self.trail_len = 0;
        self.telemetry_start = 0;
        self.telemetry_len = 0;
        self.event_start = 0;
        self.event_len = 0;
        self.preview = true;
        self.push_event(OrbitEventKind::Loaded);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn update(&mut self, elapsed_ms: u16) -> ChangeSet {
        let active = self.status == OrbitStatus::Running && self.modal == OrbitModal::None;
        let steps = self.clock.steps(elapsed_ms, active);
        if steps == 0 {
            return ChangeSet::NONE;
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
        }
        if (self.time * 10).to_int() != display_tenth
            || self.fuel != fuel
            || self.status != status
            || self.queue_len != queue_len
        {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL
        }
    }

    pub(crate) fn toggle_running(&mut self) -> ChangeSet {
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

    pub(crate) fn burn(&mut self) -> Result<ChangeSet, OrbitError> {
        self.burn_values(self.dv_tenths, self.angle_degrees)
    }

    fn burn_values(&mut self, dv_tenths: u8, angle_degrees: i16) -> Result<ChangeSet, OrbitError> {
        if !(2..=80).contains(&dv_tenths) || !(-180..=180).contains(&angle_degrees) {
            return Err(OrbitError::InvalidBurn);
        }
        if self.status.terminal() {
            return Err(OrbitError::Terminal);
        }
        let dv = Fixed64::from_ratio(i64::from(dv_tenths), 10);
        let fuel_cost = dv * 2;
        let heat_cost = dv * 7;
        if self.fuel < fuel_cost {
            return Err(OrbitError::Fuel);
        }
        if self.heat + heat_cost > Fixed64::from_int(100) {
            return Err(OrbitError::Heat);
        }
        impulse(&mut self.body, dv, angle_degrees);
        self.fuel -= fuel_cost;
        self.heat += heat_cost;
        self.push_event(OrbitEventKind::Burn);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn schedule(&mut self) -> Result<ChangeSet, OrbitError> {
        if self.status.terminal() {
            return Err(OrbitError::Terminal);
        }
        if !(1..=30).contains(&self.delay_seconds) {
            return Err(OrbitError::InvalidDelay);
        }
        if usize::from(self.queue_len) >= MAX_NODES {
            return Err(OrbitError::QueueFull);
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
        self.page = OrbitPage::Plan;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn cancel(&mut self, id: u8) -> Result<ChangeSet, OrbitError> {
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
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn scan(&mut self) -> Result<ChangeSet, OrbitError> {
        if !self.eligible() {
            return Err(OrbitError::OutsideWindow);
        }
        self.push_event(OrbitEventKind::Goal);
        self.completed += 1;
        if self.goal().is_none() {
            self.status = OrbitStatus::Won;
            self.queue = [None; MAX_NODES];
            self.queue_len = 0;
            self.push_event(OrbitEventKind::Won);
        }
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn step(&mut self) -> bool {
        if self.status.terminal() {
            return false;
        }
        self.time += STEP;
        while self.queue_len > 0 && self.queue[0].is_some_and(|node| node.at <= self.time) {
            let node = self.queue[0].expect("queue head exists");
            let _ = self.cancel(node.id);
            if self
                .burn_values(node.dv_tenths, node.angle_degrees)
                .is_err()
            {
                self.push_event(OrbitEventKind::NodeSkipped);
            }
        }
        integrate(&mut self.body, STEP);
        self.heat = (self.heat - STEP * 4).max(Fixed64::ZERO);
        let radius = self.body.radius();
        if radius < Fixed64::from_int(27) {
            self.status = OrbitStatus::Crashed;
            self.push_event(OrbitEventKind::Crashed);
        } else if radius > Fixed64::from_int(260) {
            self.status = OrbitStatus::Escaped;
            self.push_event(OrbitEventKind::Escaped);
        }
        if self.time >= self.next_record {
            self.next_record = self.time + RECORD_INTERVAL;
            self.push_trail(self.body.position);
            self.push_telemetry(OrbitTelemetry {
                time: self.time,
                radius,
                speed: self.body.speed(),
                fuel: self.fuel,
            });
        }
        true
    }

    pub(crate) fn predict(&self, output: &mut [OrbitPoint; MAX_PREVIEW]) -> usize {
        let mut body = self.body;
        impulse(
            &mut body,
            Fixed64::from_ratio(i64::from(self.dv_tenths), 10),
            self.angle_degrees,
        );
        let mut len = 0;
        for point in output.iter_mut() {
            for _ in 0..6 {
                integrate(&mut body, STEP);
            }
            *point = body.position;
            len += 1;
            let radius = body.radius();
            if radius < Fixed64::from_int(27) || radius > Fixed64::from_int(260) {
                break;
            }
        }
        len
    }

    pub(crate) fn eligible(&self) -> bool {
        let Some(goal) = self.goal() else {
            return false;
        };
        match goal {
            OrbitGoal::Band { min, max, .. } => {
                let radius = self.body.radius();
                radius >= Fixed64::from_int(i64::from(min))
                    && radius <= Fixed64::from_int(i64::from(max))
            }
            OrbitGoal::Beacon {
                x,
                y,
                range,
                max_speed,
                ..
            } => {
                let offset = OrbitPoint {
                    x: self.body.position.x - Fixed64::from_int(i64::from(x)),
                    y: self.body.position.y - Fixed64::from_int(i64::from(y)),
                };
                offset.radius() <= Fixed64::from_int(i64::from(range))
                    && self.body.speed() <= Fixed64::from_int(i64::from(max_speed))
            }
        }
    }

    fn goal(&self) -> Option<OrbitGoal> {
        MISSIONS[usize::from(self.mission)]
            .goals
            .get(usize::from(self.completed))
            .copied()
            .flatten()
    }

    fn push_trail(&mut self, value: OrbitPoint) {
        ring_push(
            &mut self.trail,
            &mut self.trail_start,
            &mut self.trail_len,
            value,
        );
    }

    fn push_telemetry(&mut self, value: OrbitTelemetry) {
        ring_push(
            &mut self.telemetry,
            &mut self.telemetry_start,
            &mut self.telemetry_len,
            value,
        );
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

    pub(crate) fn set_page(&mut self, page: OrbitPage) -> ChangeSet {
        self.page = page;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_modal(&mut self, modal: OrbitModal) -> ChangeSet {
        self.modal = modal;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn adjust_dv(&mut self, delta_tenths: i8) -> ChangeSet {
        self.dv_tenths = (i16::from(self.dv_tenths) + i16::from(delta_tenths)).clamp(2, 80) as u8;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn reset_dv(&mut self) -> ChangeSet {
        self.dv_tenths = self.mission().default_dv_tenths;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn adjust_angle(&mut self, delta: i16) -> ChangeSet {
        self.angle_degrees = (self.angle_degrees + delta).clamp(-180, 180);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn adjust_delay(&mut self, delta: i8) -> ChangeSet {
        self.delay_seconds = (i16::from(self.delay_seconds) + i16::from(delta)).clamp(1, 30) as u8;
        ChangeSet::MODEL | ChangeSet::VISUAL
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

    pub(crate) const fn page(&self) -> OrbitPage {
        self.page
    }
    pub(crate) const fn modal(&self) -> OrbitModal {
        self.modal
    }
    pub(crate) const fn status(&self) -> OrbitStatus {
        self.status
    }
    pub(crate) const fn mission_index(&self) -> u8 {
        self.mission
    }
    pub(crate) const fn mission(&self) -> OrbitMission {
        MISSIONS[self.mission as usize]
    }
    pub(crate) const fn body(&self) -> OrbitBody {
        self.body
    }
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
    pub(crate) const fn delay_seconds(&self) -> u8 {
        self.delay_seconds
    }
    pub(crate) const fn warp(&self) -> u8 {
        self.warp
    }
    pub(crate) const fn preview(&self) -> bool {
        self.preview
    }
    pub(crate) const fn queue_len(&self) -> usize {
        self.queue_len as usize
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

    pub(crate) fn trail(&self, index: usize) -> Option<OrbitPoint> {
        ring_get(&self.trail, self.trail_start, self.trail_len, index)
    }

    pub(crate) fn telemetry(&self, index: usize) -> Option<OrbitTelemetry> {
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
