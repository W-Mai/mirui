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

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
pub(crate) struct OrbitModel {
    pub(super) body: OrbitBody,
    pub(super) trail: [OrbitPoint; MAX_TRAIL],
    pub(super) telemetry: [OrbitTelemetry; MAX_TELEMETRY],
    pub(super) events: [OrbitEvent; MAX_EVENTS],
    #[observe]
    pub(crate) queue: [Option<ManeuverNode>; MAX_NODES],
    pub(super) clock: BoundedClock,
    #[observe]
    pub(crate) time: Fixed64,
    #[observe]
    pub(crate) fuel: Fixed64,
    #[observe]
    pub(crate) heat: Fixed64,
    pub(super) next_record: Fixed64,
    #[observe]
    pub(crate) page: OrbitPage,
    #[observe]
    pub(crate) modal: OrbitModal,
    #[observe]
    pub(crate) status: OrbitStatus,
    #[observe]
    pub(crate) mission: u8,
    pub(super) completed: u8,
    #[observe]
    pub(crate) dv_tenths: u8,
    #[observe]
    pub(crate) angle_degrees: i16,
    #[observe]
    pub(crate) delay_seconds: u8,
    #[observe]
    pub(crate) warp: u8,
    pub(super) next_node_id: u8,
    #[observe]
    pub(crate) queue_len: u8,
    pub(super) trail_start: u8,
    #[observe]
    pub(crate) trail_len: u8,
    pub(super) telemetry_start: u8,
    #[observe]
    pub(crate) telemetry_len: u8,
    pub(super) event_start: u8,
    #[observe]
    pub(crate) event_len: u8,
    #[observe]
    pub(crate) preview: bool,
    preview_points: [OrbitPoint; MAX_PREVIEW],
    preview_len: u8,
    preview_sampled_at: Fixed64,
    preview_body: OrbitBody,
    preview_dv_tenths: u8,
    preview_angle_degrees: i16,
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
            preview_points: [OrbitPoint::default(); MAX_PREVIEW],
            preview_len: 0,
            preview_sampled_at: Fixed64::from_int(-1),
            preview_body: OrbitBody::default(),
            preview_dv_tenths: 0,
            preview_angle_degrees: 0,
        };
        model.load_mission_raw(0);
        model
    }
}

impl OrbitModel {
    fn load_mission_raw(&mut self, mission: u8) -> ChangeSet {
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
        self.refresh_preview(true);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn update_raw(&mut self, elapsed_ms: u16) -> ChangeSet {
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
        self.refresh_preview(false);
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

    fn burn_raw(&mut self) -> Result<ChangeSet, OrbitError> {
        let changes = self.burn_values(self.dv_tenths, self.angle_degrees)?;
        self.refresh_preview(true);
        Ok(changes)
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

    fn schedule_raw(&mut self) -> Result<ChangeSet, OrbitError> {
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

    pub(super) fn cancel_raw(&mut self, id: u8) -> Result<ChangeSet, OrbitError> {
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

    fn scan_raw(&mut self) -> Result<ChangeSet, OrbitError> {
        if !self.eligible_raw() {
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
            let _ = self.cancel_raw(node.id);
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

    fn refresh_preview(&mut self, force: bool) {
        let changed = (self.preview_body != self.body
            && (self.time - self.preview_sampled_at).abs() >= Fixed64::from_ratio(7, 10))
            || self.preview_dv_tenths != self.dv_tenths
            || self.preview_angle_degrees != self.angle_degrees;
        if !force && !changed {
            return;
        }
        let mut points = [OrbitPoint::default(); MAX_PREVIEW];
        let len = self.predict(&mut points);
        self.preview_points = points;
        self.preview_len = len as u8;
        self.preview_sampled_at = self.time;
        self.preview_body = self.body;
        self.preview_dv_tenths = self.dv_tenths;
        self.preview_angle_degrees = self.angle_degrees;
    }

    fn eligible_raw(&self) -> bool {
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
        self.refresh_preview(false);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn reset_dv_raw(&mut self) -> ChangeSet {
        let dv_tenths = self.mission().default_dv_tenths;
        if self.dv_tenths == dv_tenths {
            return ChangeSet::NONE;
        }
        self.dv_tenths = dv_tenths;
        self.refresh_preview(false);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn adjust_angle_raw(&mut self, delta: i16) -> ChangeSet {
        let angle_degrees = (self.angle_degrees + delta).clamp(-180, 180);
        if self.angle_degrees == angle_degrees {
            return ChangeSet::NONE;
        }
        self.angle_degrees = angle_degrees;
        self.refresh_preview(false);
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

    fn toggle_preview_raw(&mut self) -> ChangeSet {
        self.preview = !self.preview;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn cycle_warp_raw(&mut self) -> ChangeSet {
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
    #[cfg(test)]
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
    #[cfg(test)]
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

    pub(crate) const fn preview_len(&self) -> usize {
        self.preview_len as usize
    }

    pub(crate) fn preview_point(&self, index: usize) -> Option<OrbitPoint> {
        self.preview_points.get(index).copied()
    }
}

#[crate::model]
impl OrbitModel {
    #[model(local)]
    pub(crate) fn load_mission(&mut self, mission: u8) -> ChangeSet {
        self.load_mission_raw(mission)
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        self.update_raw(elapsed_ms)
    }

    pub(crate) fn toggle_running(&mut self) -> ChangeSet {
        self.toggle_running_raw()
    }

    pub(crate) fn burn(&mut self) -> Result<ChangeSet, OrbitError> {
        self.burn_raw()
    }

    pub(crate) fn schedule(&mut self) -> Result<ChangeSet, OrbitError> {
        self.schedule_raw()
    }

    pub(crate) fn cancel_at(&mut self, index: usize) -> Result<ChangeSet, OrbitError> {
        let Some(node) = self.queue(index) else {
            return Err(OrbitError::MissingNode);
        };
        self.cancel_raw(node.id)
    }

    pub(crate) fn scan(&mut self) -> Result<ChangeSet, OrbitError> {
        self.scan_raw()
    }

    pub(crate) fn set_page(&mut self, page: OrbitPage) -> ChangeSet {
        self.set_page_raw(page)
    }

    pub(crate) fn set_modal(&mut self, modal: OrbitModal) -> ChangeSet {
        self.set_modal_raw(modal)
    }

    pub(crate) fn request_mission_reload(&mut self) -> ChangeSet {
        self.set_modal_raw(OrbitModal::Confirm(self.mission))
    }

    pub(crate) fn select_modal_option(&mut self, index: usize) -> ChangeSet {
        match self.modal {
            OrbitModal::Missions if index < MISSION_COUNT => {
                self.set_modal_raw(OrbitModal::Confirm(index as u8))
            }
            OrbitModal::Missions if index == MISSION_COUNT => self.set_modal_raw(OrbitModal::None),
            OrbitModal::Confirm(_) if index == 0 => self.set_modal_raw(OrbitModal::None),
            OrbitModal::Confirm(mission) if index == 1 => self.load_mission(mission),
            OrbitModal::Help => self.set_modal_raw(OrbitModal::None),
            _ => ChangeSet::NONE,
        }
    }

    pub(crate) fn adjust_dv(&mut self, delta_tenths: i8) -> ChangeSet {
        self.adjust_dv_raw(delta_tenths)
    }

    pub(crate) fn reset_dv(&mut self) -> ChangeSet {
        self.reset_dv_raw()
    }

    pub(crate) fn adjust_angle(&mut self, delta: i16) -> ChangeSet {
        self.adjust_angle_raw(delta)
    }

    pub(crate) fn adjust_delay(&mut self, delta: i8) -> ChangeSet {
        self.adjust_delay_raw(delta)
    }

    pub(crate) fn toggle_preview(&mut self) -> ChangeSet {
        self.toggle_preview_raw()
    }

    pub(crate) fn cycle_warp(&mut self) -> ChangeSet {
        self.cycle_warp_raw()
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
    pub(crate) fn latest_event_kind(&self) -> Option<OrbitEventKind> {
        self.event_len()
            .checked_sub(1)
            .and_then(|index| self.event(index))
            .map(|event| event.kind)
    }

    #[observe]
    pub(crate) fn can_schedule(&self) -> bool {
        !self.status.terminal() && usize::from(self.queue_len) < MAX_NODES
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
