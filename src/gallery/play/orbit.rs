use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
use crate::types::{Fixed, Fixed64};

pub(crate) const MAX_NODES: usize = 3;
pub(crate) const MAX_TRAIL: usize = 192;
pub(crate) const MAX_TELEMETRY: usize = 80;
pub(crate) const MAX_EVENTS: usize = 12;
pub(crate) const MAX_PREVIEW: usize = 240;
pub(crate) const MISSION_COUNT: usize = 3;

const MU: Fixed64 = Fixed64::from_int(100_000);
const STEP: Fixed64 = Fixed64::from_ratio(1, 60);
const RECORD_INTERVAL: Fixed64 = Fixed64::from_ratio(4, 25);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitPoint {
    pub(crate) x: Fixed64,
    pub(crate) y: Fixed64,
}

impl OrbitPoint {
    fn radius(self) -> Fixed64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitBody {
    pub(crate) position: OrbitPoint,
    pub(crate) velocity: OrbitPoint,
}

impl OrbitBody {
    pub(crate) fn radius(self) -> Fixed64 {
        self.position.radius()
    }

    pub(crate) fn speed(self) -> Fixed64 {
        self.velocity.radius()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitGoal {
    Band {
        min: i16,
        max: i16,
    },
    Beacon {
        x: i16,
        y: i16,
        range: i16,
        max_speed: i16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OrbitMission {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) default_dv_tenths: u8,
    goals: [Option<OrbitGoal>; 2],
}

pub(crate) const MISSIONS: [OrbitMission; MISSION_COUNT] = [
    OrbitMission {
        name: "轨道抬升",
        description: "进入橙色采样带后手动采样。",
        hint: "沿速度方向 +3.8，滑行到远端。",
        default_dv_tenths: 38,
        goals: [Some(OrbitGoal::Band { min: 113, max: 130 }), None],
    },
    OrbitMission {
        name: "往返测绘",
        description: "先采外轨，再回内轨采样。",
        hint: "外轨采样后继续滑行回内轨。",
        default_dv_tenths: 38,
        goals: [
            Some(OrbitGoal::Band { min: 113, max: 130 }),
            Some(OrbitGoal::Band { min: 75, max: 90 }),
        ],
    },
    OrbitMission {
        name: "远端通信",
        description: "接近远端信标，并降低到接入速度。",
        hint: "沿速度方向 +5.2，到远端信标接入。",
        default_dv_tenths: 52,
        goals: [
            Some(OrbitGoal::Beacon {
                x: -148,
                y: 0,
                range: 23,
                max_speed: 24,
            }),
            None,
        ],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitPage {
    Map,
    Plan,
    Record,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitModal {
    None,
    Missions,
    Confirm(u8),
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitStatus {
    Ready,
    Running,
    Paused,
    Won,
    Crashed,
    Escaped,
}

impl OrbitStatus {
    pub(crate) const fn terminal(self) -> bool {
        matches!(self, Self::Won | Self::Crashed | Self::Escaped)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitError {
    InvalidBurn,
    Fuel,
    Heat,
    Terminal,
    InvalidDelay,
    QueueFull,
    MissingNode,
    OutsideWindow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ManeuverNode {
    pub(crate) id: u8,
    pub(crate) at: Fixed64,
    pub(crate) dv_tenths: u8,
    pub(crate) angle_degrees: i16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrbitTelemetry {
    pub(crate) time: Fixed64,
    pub(crate) radius: Fixed64,
    pub(crate) speed: Fixed64,
    pub(crate) fuel: Fixed64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitEventKind {
    Loaded,
    Burn,
    NodeSkipped,
    Goal,
    Won,
    Crashed,
    Escaped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OrbitEvent {
    pub(crate) time: Fixed64,
    pub(crate) kind: OrbitEventKind,
}

impl Default for OrbitEvent {
    fn default() -> Self {
        Self {
            time: Fixed64::ZERO,
            kind: OrbitEventKind::Loaded,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OrbitModel {
    body: OrbitBody,
    trail: [OrbitPoint; MAX_TRAIL],
    telemetry: [OrbitTelemetry; MAX_TELEMETRY],
    events: [OrbitEvent; MAX_EVENTS],
    queue: [Option<ManeuverNode>; MAX_NODES],
    clock: BoundedClock,
    time: Fixed64,
    fuel: Fixed64,
    heat: Fixed64,
    next_record: Fixed64,
    page: OrbitPage,
    modal: OrbitModal,
    status: OrbitStatus,
    mission: u8,
    completed: u8,
    dv_tenths: u8,
    angle_degrees: i16,
    delay_seconds: u8,
    warp: u8,
    next_node_id: u8,
    queue_len: u8,
    trail_start: u8,
    trail_len: u8,
    telemetry_start: u8,
    telemetry_len: u8,
    event_start: u8,
    event_len: u8,
    preview: bool,
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

    fn push_event(&mut self, kind: OrbitEventKind) {
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

fn integrate(body: &mut OrbitBody, dt: Fixed64) {
    let radius = body.radius().max(Fixed64::from_int(20));
    let factor = -MU / (radius * radius * radius);
    body.velocity.x += body.position.x * factor * dt / 2;
    body.velocity.y += body.position.y * factor * dt / 2;
    body.position.x += body.velocity.x * dt;
    body.position.y += body.velocity.y * dt;
    let radius = body.radius().max(Fixed64::from_int(20));
    let factor = -MU / (radius * radius * radius);
    body.velocity.x += body.position.x * factor * dt / 2;
    body.velocity.y += body.position.y * factor * dt / 2;
}

fn impulse(body: &mut OrbitBody, dv: Fixed64, angle_degrees: i16) {
    let speed = body.speed().max(Fixed64::ONE);
    let ux = body.velocity.x / speed;
    let uy = body.velocity.y / speed;
    let angle = Fixed::from_int(i32::from(angle_degrees));
    let cosine = Fixed64::from_fixed(Fixed::cos_deg(angle));
    let sine = Fixed64::from_fixed(Fixed::sin_deg(angle));
    body.velocity.x += dv * (ux * cosine - uy * sine);
    body.velocity.y += dv * (ux * sine + uy * cosine);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(mission: u8) -> OrbitModel {
        let mut model = OrbitModel::default();
        model.load_mission(mission);
        model.burn().unwrap();
        for _ in 0..3_600 {
            model.step();
            if model.eligible() {
                model.scan().unwrap();
            }
            if model.status() == OrbitStatus::Won {
                break;
            }
        }
        model
    }

    #[test]
    fn documented_maneuvers_complete_all_missions() {
        for mission in 0..MISSION_COUNT as u8 {
            let model = complete(mission);
            assert_eq!(model.status(), OrbitStatus::Won, "mission {mission}");
            assert_eq!(
                model.completed as usize,
                model.mission().goals.iter().flatten().count()
            );
            assert_eq!(model.queue_len(), 0);
        }
    }

    #[test]
    fn prediction_is_bounded_and_does_not_mutate_model() {
        let model = OrbitModel::default();
        let before = model;
        let mut points = [OrbitPoint::default(); MAX_PREVIEW];
        assert!(model.predict(&mut points) <= MAX_PREVIEW);
        assert_eq!(model.body(), before.body());
        assert_eq!(model.fuel(), before.fuel());
        assert_eq!(model.time(), before.time());
    }

    #[test]
    fn burn_costs_once_and_failures_are_atomic() {
        let mut model = OrbitModel::default();
        model.dv_tenths = 30;
        model.burn().unwrap();
        assert_eq!(model.fuel(), Fixed64::from_int(42));
        assert_eq!(model.heat(), Fixed64::from_int(21));

        let mut fuel_failure = OrbitModel::default();
        fuel_failure.fuel = Fixed64::ONE;
        let body = fuel_failure.body();
        assert_eq!(fuel_failure.burn(), Err(OrbitError::Fuel));
        assert_eq!(fuel_failure.body(), body);

        let mut heat_failure = OrbitModel::default();
        heat_failure.heat = Fixed64::from_int(99);
        heat_failure.dv_tenths = 10;
        let body = heat_failure.body();
        assert_eq!(heat_failure.burn(), Err(OrbitError::Heat));
        assert_eq!(heat_failure.body(), body);
    }

    #[test]
    fn scheduled_nodes_snapshot_values_execute_once_and_cancel_by_id() {
        let mut model = OrbitModel::default();
        model.dv_tenths = 20;
        model.angle_degrees = 15;
        model.delay_seconds = 1;
        model.schedule().unwrap();
        model.dv_tenths = 70;
        model.angle_degrees = -90;
        assert_eq!(model.queue(0).unwrap().dv_tenths, 20);
        assert_eq!(model.queue(0).unwrap().angle_degrees, 15);
        model.schedule().unwrap();
        model.schedule().unwrap();
        assert_eq!(model.schedule(), Err(OrbitError::QueueFull));
        let middle = model.queue(1).unwrap().id;
        model.cancel(middle).unwrap();
        assert_eq!(model.queue_len(), 2);
        for _ in 0..180 {
            model.step();
        }
        assert_eq!(model.queue_len(), 0);
        assert!(model.fuel() < Fixed64::from_int(48));
    }

    #[test]
    fn nominal_orbit_and_energy_remain_bounded() {
        let mut circular = OrbitModel::default();
        for _ in 0..7_200 {
            circular.step();
        }
        let drift = (circular.body().radius() - Fixed64::from_int(78)).abs();
        assert!(drift < Fixed64::from_ratio(3, 10));

        let mut raised = OrbitModel::default();
        raised.burn().unwrap();
        let initial = energy(raised.body());
        for _ in 0..3_600 {
            raised.step();
        }
        let drift = (energy(raised.body()) - initial).abs() / initial.abs();
        assert!(drift < Fixed64::from_ratio(2, 1_000));
    }

    #[test]
    fn terminal_boundaries_stop_simulation() {
        let mut crash = OrbitModel::default();
        crash.body.position.x = Fixed64::from_int(25);
        crash.status = OrbitStatus::Running;
        crash.step();
        assert_eq!(crash.status(), OrbitStatus::Crashed);
        assert_eq!(crash.burn(), Err(OrbitError::Terminal));

        let mut escape = OrbitModel::default();
        escape.body.position.x = Fixed64::from_int(261);
        escape.body.velocity.x = Fixed64::from_int(10);
        escape.status = OrbitStatus::Running;
        escape.step();
        assert_eq!(escape.status(), OrbitStatus::Escaped);
    }

    #[test]
    fn histories_and_model_memory_are_bounded() {
        let mut model = OrbitModel::default();
        for _ in 0..4_000 {
            model.step();
        }
        for _ in 0..40 {
            model.push_event(OrbitEventKind::Burn);
        }
        assert_eq!(model.trail_len(), MAX_TRAIL);
        assert_eq!(model.telemetry_len(), MAX_TELEMETRY);
        assert_eq!(model.event_len(), MAX_EVENTS);
        assert!(core::mem::size_of::<OrbitModel>() <= 8 * 1024);
    }

    #[test]
    fn modal_time_does_not_accumulate_debt() {
        let mut model = OrbitModel::default();
        model.toggle_running();
        model.update(16);
        let time = model.time();
        model.set_modal(OrbitModal::Help);
        model.update(1_000);
        assert_eq!(model.time(), time);
        model.set_modal(OrbitModal::None);
        model.update(1);
        assert_eq!(model.time(), time);
    }

    #[test]
    fn prograde_and_retrograde_impulses_diverge() {
        let mut prograde = OrbitModel::default().body();
        let mut retrograde = prograde;
        impulse(&mut prograde, Fixed64::from_int(3), 0);
        impulse(&mut retrograde, Fixed64::from_int(3), 180);
        assert!(prograde.speed() > retrograde.speed());
    }

    fn energy(body: OrbitBody) -> Fixed64 {
        body.speed() * body.speed() / 2 - MU / body.radius()
    }
}
