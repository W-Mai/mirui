use crate::world::{
    CORES, District, EXIT, GUARDS, Harbor, Location, SPAWN, add, direction, dot, length_squared,
    offset, scaled,
};
use mirui::{anim::Tween, types::Fixed};

#[path = "gameplay/autopilot.rs"]
mod autopilot;
use autopilot::Pilot;

pub const STEP_MS: u16 = 16;
const TETHER_RANGE: Fixed = Fixed::from_int(11);
const ROPE_LENGTH: Fixed = Fixed::from_int(6);
const GATE_RADIUS: Fixed = Fixed::from_int(4);
const IMPACT_RADIUS: Fixed = Fixed::from_int(3);
const SAFE_RADIUS: Fixed = Fixed::from_int(8);
const PASSAGE_DISTANCE: Fixed = Fixed::from_int(7);

struct Passage {
    from: Location,
    distance: Fixed,
    motion: Tween,
    arriving: bool,
}

impl Passage {
    fn new(from: Location, distance: Fixed, arriving: bool) -> Self {
        Self {
            from,
            distance,
            motion: Tween::ease_to(Fixed::ZERO, Fixed::ONE, if arriving { 570 } else { 430 }),
            arriving,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct PassageFrame {
    pub center: [Fixed; 2],
    pub focus: Fixed,
    pub cover: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Controls {
    pub axis: [Fixed; 2],
    pub magnet: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Manual,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Session {
    Title,
    Running,
    Paused,
    Departed,
    Results,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Cue {
    #[default]
    None,
    Attach,
    Release,
    Hit,
    Warning,
    Collect,
    Revive,
    Depart,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Body {
    pub location: Location,
    pub velocity: [Fixed; 2],
}

impl Body {
    fn at(location: Location) -> Self {
        Self {
            location,
            velocity: [Fixed::ZERO; 2],
        }
    }

    fn slide(&mut self, harbor: &Harbor, tick: u32) -> Location {
        let previous = self.location;
        let desired = add(previous.point, self.velocity);
        self.location = harbor.advance(previous, desired, tick);
        for (actual, (desired, velocity)) in self
            .location
            .point
            .into_iter()
            .zip(desired.into_iter().zip(&mut self.velocity))
        {
            if actual != desired {
                *velocity = Fixed::ZERO;
            }
        }
        previous
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Core {
    pub body: Body,
    pub stability: u8,
    pub collected: bool,
    pub replenish: u16,
    armed: bool,
}

impl Core {
    fn new(at: Location) -> Self {
        Self {
            body: Body::at(at),
            stability: 3,
            collected: false,
            replenish: 0,
            armed: false,
        }
    }

    pub fn active(self) -> bool {
        !self.collected && self.replenish == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardPhase {
    Patrol,
    Warning(u16),
    Charge(u16),
    Stunned(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Guard {
    pub body: Body,
    pub phase: GuardPhase,
}

impl Guard {
    fn new(location: Location) -> Self {
        Self {
            body: Body::at(location),
            phase: GuardPhase::Patrol,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResultLine {
    pub collected: u8,
    pub score: u32,
    pub revivals: u32,
    pub ticks: u32,
}

pub struct Game {
    pub harbor: Harbor,
    pub ship: Body,
    pub cores: [Core; CORES],
    pub guards: [Guard; GUARDS],
    pub level: u32,
    pub mode: Mode,
    pub session: Session,
    pub result: ResultLine,
    pub total: u32,
    pub life: u8,
    pub invulnerable: u16,
    pub respawn: u16,
    pub heading: [Fixed; 2],
    pub tether: Option<usize>,
    pub cue: Cue,
    pub best_manual: u32,
    pub best_assisted: u32,
    seed: u32,
    combo: u8,
    magnet: bool,
    assisted: bool,
    elapsed: u16,
    passage: Option<Passage>,
    pilot: Pilot,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let harbor = Harbor::generate(seed, 1);
        Self {
            ship: Body::at(SPAWN),
            cores: harbor.cores.map(Core::new),
            guards: harbor.guards.map(Guard::new),
            harbor,
            level: 1,
            mode: Mode::Manual,
            session: Session::Title,
            result: ResultLine::default(),
            total: 0,
            life: 3,
            invulnerable: 0,
            respawn: 0,
            heading: [Fixed::ZERO, Fixed::ONE],
            tether: None,
            cue: Cue::None,
            best_manual: 0,
            best_assisted: 0,
            seed,
            combo: 0,
            magnet: false,
            assisted: false,
            elapsed: 0,
            passage: None,
            pilot: Pilot::default(),
        }
    }

    pub fn start(&mut self) {
        self.level = 1;
        self.total = 0;
        self.assisted = self.mode == Mode::Auto;
        self.restart();
    }

    pub fn restart(&mut self) {
        self.harbor = Harbor::generate(self.seed, self.level);
        self.ship = Body::at(SPAWN);
        self.cores = self.harbor.cores.map(Core::new);
        self.guards = self.harbor.guards.map(Guard::new);
        self.result = ResultLine::default();
        self.combo = 0;
        self.life = 3;
        self.respawn = 0;
        self.invulnerable = 90;
        self.heading = [Fixed::ZERO, Fixed::ONE];
        self.cancel();
        self.pilot = Pilot::default();
        self.elapsed = 0;
        self.passage = None;
        self.session = Session::Running;
    }

    pub(crate) fn passage_frame(&self) -> Option<PassageFrame> {
        let passage = self.passage.as_ref()?;
        let progress = passage.motion.value();
        let focus = if passage.arriving {
            Fixed::ONE - progress
        } else {
            progress
        };
        let cover = ((focus * 2 - Fixed::ONE).clamp(Fixed::ZERO, Fixed::ONE) * 255)
            .round()
            .to_int() as u8;
        Some(PassageFrame {
            center: self.ship.location.point,
            focus,
            cover,
        })
    }

    pub fn finish(&mut self) {
        if matches!(self.session, Session::Running | Session::Paused) {
            self.commit_score();
        }
        self.cancel();
        self.passage = None;
        self.session = Session::Results;
    }

    pub fn pause(&mut self) {
        self.session = match self.session {
            Session::Running => Session::Paused,
            Session::Paused => Session::Running,
            other => other,
        };
        self.cancel();
        self.elapsed = 0;
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.cancel();
            self.pilot = Pilot::default();
            self.mode = mode;
            self.assisted |= mode == Mode::Auto;
        }
    }

    pub fn cancel(&mut self) {
        self.magnet = false;
        if let Some(slot) = self.tether.take() {
            self.cores[slot].armed = false;
        }
    }

    pub fn elapsed_ms(&mut self, delta: u16, input: Controls) {
        self.cue = Cue::None;
        if self.session == Session::Departed {
            self.advance_passage(delta.min(100));
            return;
        }
        if self.session != Session::Running {
            self.elapsed = 0;
            return;
        }
        if delta > 100 {
            self.elapsed = 0;
            return;
        }
        self.elapsed += delta;
        while self.elapsed >= STEP_MS {
            self.elapsed -= STEP_MS;
            match self.session {
                Session::Running => {
                    let controls = if self.mode == Mode::Auto {
                        let mut pilot = core::mem::take(&mut self.pilot);
                        let input = pilot.control(self);
                        self.pilot = pilot;
                        input
                    } else {
                        input
                    };
                    self.step(controls);
                }
                _ => break,
            }
        }
    }

    pub fn target(&self) -> Option<usize> {
        self.cores
            .iter()
            .enumerate()
            .filter(|(_, core)| {
                let position = core.body.location;
                core.active()
                    && self.ship.location.within(position, TETHER_RANGE)
                    && dot(
                        offset(position.point, self.ship.location.point),
                        self.heading,
                    ) >= Fixed::ZERO
                    && line_open(
                        &self.harbor,
                        self.result.ticks,
                        self.ship.location,
                        position,
                    )
            })
            .min_by_key(|(_, core)| {
                length_squared(offset(core.body.location.point, self.ship.location.point))
            })
            .map(|(slot, _)| slot)
    }

    pub fn ready(&self) -> bool {
        self.result.collected >= self.harbor.quota
    }

    fn step(&mut self, controls: Controls) {
        self.result.ticks = self.result.ticks.saturating_add(1);
        let tick = self.result.ticks;
        if self.respawn != 0 {
            self.respawn -= 1;
            if self.respawn == 0 {
                self.ship = Body::at(SPAWN);
                self.life = 3;
                self.invulnerable = 120;
                self.cue = Cue::Revive;
            }
            return;
        }
        self.invulnerable = self.invulnerable.saturating_sub(1);
        let axis = controls
            .axis
            .map(|v| v.clamp(Fixed::from_int(-2), Fixed::from_int(2)));
        if axis != [Fixed::ZERO; 2] {
            self.heading = direction(axis);
        }
        if controls.magnet && !self.magnet {
            self.tether = self.target();
            if let Some(slot) = self.tether {
                self.cores[slot].armed = false;
                self.cue = Cue::Attach;
            }
        } else if !controls.magnet
            && let Some(slot) = self.tether.take()
        {
            self.cores[slot].armed = true;
            self.cue = Cue::Release;
        }
        self.magnet = controls.magnet;
        self.ship.velocity = limited(
            add(
                scaled(self.ship.velocity, Fixed::from_ratio(86, 100)),
                scaled(
                    self.heading,
                    if axis == [Fixed::ZERO; 2] {
                        Fixed::ZERO
                    } else {
                        Fixed::from_ratio(22, 100)
                    },
                ),
            ),
            Fixed::from_ratio(13, 10),
        );
        let previous = self.ship.slide(&self.harbor, tick);
        self.move_guards();
        self.move_cores();
        if self.invulnerable == 0
            && !self.ship.location.within(SPAWN, SAFE_RADIUS)
            && self.guards[..usize::from(self.harbor.guard_count)]
                .iter()
                .any(|guard| {
                    matches!(guard.phase, GuardPhase::Charge(_))
                        && swept(
                            previous,
                            self.ship.location,
                            guard.body.location,
                            IMPACT_RADIUS,
                        )
                })
        {
            self.life -= 1;
            self.invulnerable = 55;
            self.cue = Cue::Hit;
            if self.life == 0 {
                self.cancel();
                self.result.revivals = self.result.revivals.saturating_add(1);
                self.combo = 0;
                self.respawn = 42;
            }
        }
        if self.life != 0
            && let Some(direction) = crossed_exit(previous, self.ship.location)
        {
            if self.ready() {
                self.depart(direction);
            } else {
                self.ship.location.point[1] = previous.point[1];
                self.ship.velocity[1] = Fixed::ZERO;
            }
        }
    }

    fn depart(&mut self, direction: Fixed) {
        let spare = 3000u32.saturating_sub(self.result.ticks) / 12;
        self.result.score = self.result.score.saturating_add(spare);
        self.commit_score();
        self.cancel();
        self.heading = [Fixed::ZERO, direction];
        self.passage = Some(Passage::new(
            self.ship.location,
            direction * PASSAGE_DISTANCE,
            false,
        ));
        self.session = Session::Departed;
        self.cue = Cue::Depart;
    }

    fn advance_passage(&mut self, delta: u16) {
        let Some(mut passage) = self.passage.take() else {
            return;
        };
        passage.motion.tick(delta);
        self.ship.location.point[1] =
            passage.from.point[1] + passage.distance * passage.motion.value();
        if !passage.motion.is_finished() {
            self.passage = Some(passage);
        } else if passage.arriving {
            self.session = Session::Running;
        } else {
            self.level = self.level.saturating_add(1);
            self.restart();
            let mut entrance = SPAWN;
            entrance.point[1] -= PASSAGE_DISTANCE;
            self.ship = Body::at(entrance);
            self.session = Session::Departed;
            self.passage = Some(Passage::new(entrance, PASSAGE_DISTANCE, true));
        }
    }

    fn commit_score(&mut self) {
        self.total = self.total.saturating_add(self.result.score);
        let best = if self.assisted {
            &mut self.best_assisted
        } else {
            &mut self.best_manual
        };
        *best = (*best).max(self.total);
    }

    pub(crate) fn best_for_run(&self) -> u32 {
        if self.assisted {
            self.best_assisted
        } else {
            self.best_manual
        }
    }

    fn move_cores(&mut self) {
        for (slot, core) in self.cores.iter_mut().enumerate() {
            if core.collected {
                continue;
            }
            if core.replenish > 0 {
                core.replenish -= 1;
                if core.replenish == 0 {
                    *core = Core::new(self.harbor.cores[slot]);
                }
                continue;
            }
            let at = core.body.location;
            if self.tether == Some(slot) {
                let rope = offset(self.ship.location.point, at.point);
                let length = length_squared(rope).sqrt();
                core.body.velocity = scaled(core.body.velocity, Fixed::from_ratio(91, 100));
                if length > ROPE_LENGTH {
                    let tension = ((length - ROPE_LENGTH) * Fixed::from_ratio(18, 100))
                        .min(Fixed::from_ratio(48, 100));
                    core.body.velocity = add(core.body.velocity, scaled(direction(rope), tension));
                }
                if length > TETHER_RANGE * 2
                    || !line_open(&self.harbor, self.result.ticks, at, self.ship.location)
                {
                    self.tether = None;
                }
            } else {
                core.body.velocity = add(
                    scaled(core.body.velocity, Fixed::from_ratio(96, 100)),
                    self.harbor.magnetic_field(at, self.result.ticks),
                );
            }
            core.body.velocity = limited(core.body.velocity, Fixed::from_int(2));
            core.body.slide(&self.harbor, self.result.ticks);
            let current = core.body.location;
            let gate = if current.upper {
                self.harbor.high_gate()
            } else {
                self.harbor.gate
            };
            if swept(at, current, gate, GATE_RADIUS) {
                core.collected = true;
                if self.tether == Some(slot) {
                    self.tether = None;
                }
                self.result.collected += 1;
                self.combo = self.combo.saturating_add(1);
                self.result.score = self.result.score.saturating_add(
                    70 + u32::from(core.stability) * 10 + u32::from(self.combo) * 12,
                );
                self.cue = Cue::Collect;
                continue;
            }
            if !core.armed || length_squared(core.body.velocity) < Fixed::from_ratio(1, 4) {
                continue;
            }
            if let Some(guard) = self.guards[..usize::from(self.harbor.guard_count)]
                .iter_mut()
                .find(|guard| {
                    !matches!(guard.phase, GuardPhase::Stunned(_))
                        && swept(at, current, guard.body.location, IMPACT_RADIUS)
                })
            {
                guard.phase = GuardPhase::Stunned(180);
                core.stability -= 1;
                core.body.velocity = scaled(core.body.velocity, -Fixed::HALF);
                core.armed = false;
                if core.stability == 0 {
                    core.replenish = 115;
                }
                self.cue = Cue::Hit;
            }
        }
    }

    fn move_guards(&mut self) {
        if self.harbor.district == District::Outer && self.result.collected == 0 {
            return;
        }
        for (slot, guard) in self.guards[..usize::from(self.harbor.guard_count)]
            .iter_mut()
            .enumerate()
        {
            let at = guard.body.location;
            match guard.phase {
                GuardPhase::Stunned(t) => {
                    guard.phase = if t <= 1 {
                        GuardPhase::Patrol
                    } else {
                        GuardPhase::Stunned(t - 1)
                    };
                    continue;
                }
                GuardPhase::Warning(t) => {
                    guard.phase = if t <= 1 {
                        GuardPhase::Charge(24)
                    } else {
                        GuardPhase::Warning(t - 1)
                    };
                    continue;
                }
                GuardPhase::Charge(t) if t > 0 => guard.phase = GuardPhase::Charge(t - 1),
                _ => {
                    let ship = self.ship.location;
                    let to_ship = offset(ship.point, at.point);
                    if ship.upper == at.upper
                        && length_squared(to_ship) < Fixed::from_int(325)
                        && !ship.within(SPAWN, SAFE_RADIUS)
                        && self
                            .result
                            .ticks
                            .wrapping_add(slot as u32 * 27)
                            .is_multiple_of(86)
                    {
                        guard.body.velocity = scaled(direction(to_ship), Fixed::from_ratio(19, 10));
                        guard.phase = GuardPhase::Warning(26);
                        self.cue = Cue::Warning;
                        continue;
                    }
                    let mut home = self.harbor.guards[slot];
                    let phase = (self.result.ticks / 120 + slot as u32) % 2;
                    if !home.upper {
                        home.point[1] += Fixed::from_int(if phase == 0 { 6 } else { -6 });
                    }
                    guard.body.velocity = scaled(
                        direction(offset(home.point, at.point)),
                        Fixed::from_ratio(28, 100),
                    );
                    guard.phase = GuardPhase::Patrol;
                }
            }
            guard.body.slide(&self.harbor, self.result.ticks);
        }
    }
}

fn crossed_exit(from: Location, to: Location) -> Option<Fixed> {
    if from.upper || to.upper {
        return None;
    }
    let exit_z = EXIT.point[1];
    let from_side = from.point[1] < exit_z;
    if from_side == (to.point[1] < exit_z) {
        return None;
    }
    let forward = (exit_z - from.point[1]) / (to.point[1] - from.point[1]);
    let x = from.point[0] + (to.point[0] - from.point[0]) * forward;
    ((x - EXIT.point[0]).abs() <= GATE_RADIUS).then_some(if from_side {
        Fixed::ONE
    } else {
        -Fixed::ONE
    })
}

fn line_open(harbor: &Harbor, tick: u32, a: Location, b: Location) -> bool {
    if a.upper || b.upper {
        !(a.upper
            && b.upper
            && harbor.lock_closed(tick)
            && (a.point[1] < Fixed::from_int(40)) != (b.point[1] < Fixed::from_int(40)))
    } else {
        harbor.layout.direct(a.point, b.point)
    }
}

fn limited(value: [Fixed; 2], limit: Fixed) -> [Fixed; 2] {
    if length_squared(value) <= limit * limit {
        value
    } else {
        scaled(direction(value), limit)
    }
}

fn swept(from: Location, to: Location, target: Location, radius: Fixed) -> bool {
    if to.upper != target.upper {
        return false;
    }
    if from.upper != to.upper {
        return to.within(target, radius);
    }
    let delta = offset(to.point, from.point);
    let length = length_squared(delta);
    let scale = if length.is_zero() {
        Fixed::ZERO
    } else {
        (dot(offset(target.point, from.point), delta) / length).clamp(Fixed::ZERO, Fixed::ONE)
    };
    length_squared(offset(add(from.point, scaled(delta, scale)), target.point)) <= radius * radius
}

#[cfg(test)]
#[path = "../tests/unit/gameplay.rs"]
mod tests;
