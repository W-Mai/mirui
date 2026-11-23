use crate::{
    app::FluxRun,
    gameplay::{Game, GuardPhase},
    world::{
        BRIDGE_START, CORES, District, GUARDS, Harbor, LOCK_POSITION, Location, SPAWN, add, scaled,
    },
};
use mirui::{core::model::ModelHandle, prelude::*};
use mirui_sprite_stack::{Pose, StackStageCommands, StackVisual};

mod assets;
pub(crate) mod camera;
mod scenery;
pub(crate) mod sound;
mod view;
pub(crate) use assets::SceneTone;
use assets::*;
pub(crate) use view::build;

const CORE_START: usize = 1;
const GUARD_START: usize = CORE_START + CORES;
const LINK_START: usize = GUARD_START + GUARDS;
const GATE_START: usize = LINK_START + 4;
const UPPER_GATE_START: usize = GATE_START + 2;
const EXIT_MARK: usize = UPPER_GATE_START + 2;
const ENTRANCE_MARK: usize = EXIT_MARK + 1;
const ALERT_START: usize = ENTRANCE_MARK + 1;
const LOCK: usize = ALERT_START + GUARDS;
const FIELD: usize = LOCK + 1;
const MARKS: usize = FIELD + 1;
pub(crate) const STAGE_CAPACITY: usize = MARKS + 3 + scenery::MAX_PROPS;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Tint {
    Normal,
    Warn,
    Dim,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Mark {
    pose: Pose,
    tint: Tint,
    flash: Fixed,
}

impl Mark {
    fn at(harbor: &Harbor, location: Location) -> Self {
        let [x, z] = location.point;
        Self {
            pose: Pose::at(x, harbor.height(location), z),
            tint: Tint::Normal,
            flash: Fixed::ZERO,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Marks([Option<Mark>; MARKS]);

impl Marks {
    pub fn from_game(game: &Game) -> Self {
        let mut marks = [None; MARKS];
        let harbor = &game.harbor;
        let mark_at = |location| Mark::at(harbor, location);
        if game.respawn == 0 {
            let mut ship = mark_at(game.ship.location);
            ship.pose.yaw = heading(game.heading);
            if game.invulnerable != 0 && (game.result.ticks / 6).is_multiple_of(2) {
                ship.flash = Fixed::HALF;
            }
            marks[0] = Some(ship);
        }
        let target = game.target();
        for (slot, core) in game.cores.iter().enumerate() {
            if !core.active() {
                continue;
            }
            let mut mark = mark_at(core.body.location);
            mark.pose.position[1] += Fixed::from_ratio(1, 2);
            mark.pose.yaw = Fixed::from_int((game.result.ticks % 360) as i32 * 2);
            mark.tint = if core.stability == 1 {
                Tint::Warn
            } else {
                Tint::Normal
            };
            if target == Some(slot) || game.tether == Some(slot) {
                mark.flash = Fixed::from_ratio(1, 3);
            }
            marks[CORE_START + slot] = Some(mark);
        }
        for (slot, guard) in game.guards[..usize::from(game.harbor.guard_count)]
            .iter()
            .enumerate()
        {
            let mut mark = mark_at(guard.body.location);
            mark.pose.yaw = heading(guard.body.velocity);
            mark.tint = match guard.phase {
                GuardPhase::Patrol => Tint::Normal,
                GuardPhase::Stunned(_) => Tint::Dim,
                _ => Tint::Warn,
            };
            marks[GUARD_START + slot] = Some(mark);
            if matches!(guard.phase, GuardPhase::Warning(_)) {
                let mut alert = mark;
                let front = scaled(guard.body.velocity, Fixed::from_int(6));
                alert.pose.position[0] += front[0];
                alert.pose.position[2] += front[1];
                marks[ALERT_START + slot] = Some(alert);
            }
        }
        if let Some(slot) = game.tether {
            let core = game.cores[slot].body.location;
            let boat = game.ship.location;
            for i in 0..4 {
                let amount = Fixed::from_ratio(i as i32 + 1, 5);
                let p = add(
                    scaled(boat.point, Fixed::ONE - amount),
                    scaled(core.point, amount),
                );
                let mut mark = mark_at(Location {
                    point: p,
                    upper: boat.upper,
                });
                mark.pose.position[1] = harbor.height(boat) * (Fixed::ONE - amount)
                    + harbor.height(core) * amount
                    + Fixed::ONE;
                mark.flash = if (game.result.ticks / 3 + i as u32).is_multiple_of(4) {
                    Fixed::ONE
                } else {
                    Fixed::ZERO
                };
                marks[LINK_START + i] = Some(mark);
            }
        }
        marks[GATE_START] = Some(mark_at(harbor.gate));
        marks[GATE_START + 1] = harbor.cargo_gate.map(mark_at);
        for (index, gate) in harbor.recovery_gates(true).into_iter().enumerate() {
            marks[UPPER_GATE_START + index] = gate.map(mark_at);
        }
        let mut exit = mark_at(game.harbor.exit);
        exit.tint = if game.ready() {
            Tint::Normal
        } else {
            Tint::Dim
        };
        marks[EXIT_MARK] = Some(exit);
        marks[ENTRANCE_MARK] = Some(mark_at(SPAWN));
        if game.harbor.lock_warning(game.result.ticks) || game.harbor.lock_closed(game.result.ticks)
        {
            let mut gate = mark_at(Location::new(harbor.bridge.x, LOCK_POSITION, true));
            gate.tint = Tint::Warn;
            marks[LOCK] = Some(gate);
        }
        if game.harbor.district == District::Storm {
            let at = Location::new(38, 43, false);
            let mut mark = mark_at(at);
            mark.tint = if game.harbor.magnetic_field(at, game.result.ticks) == [Fixed::ZERO; 2] {
                Tint::Dim
            } else {
                Tint::Warn
            };
            marks[FIELD] = Some(mark);
        }
        Self(marks)
    }
}

fn heading([x, z]: [Fixed; 2]) -> Fixed {
    Fixed::atan2(x, z) * Fixed::from_ratio(57296, 1000)
}

fn visual(slot: usize, mark: Mark, detail: usize, tone: SceneTone) -> StackVisual {
    let asset = match slot {
        0 => &SHIP,
        CORE_START..GUARD_START => &CORE,
        GUARD_START..LINK_START => &GUARD,
        LINK_START..GATE_START => &LINK,
        EXIT_MARK | ENTRANCE_MARK => &PORTAL,
        GATE_START..EXIT_MARK | LOCK | FIELD => &GATE,
        _ => &MARK,
    };
    let colors = tone.palette();
    let palette = match mark.tint {
        Tint::Normal => &colors.normal,
        Tint::Warn => &colors.warning,
        Tint::Dim => &colors.locked,
    };
    StackVisual::new(&asset[detail])
        .at(mark.pose)
        .with_palette(palette)
        .with_flash(mark.flash)
}

#[mirui::component]
struct Member {
    owner: Entity,
    visual: MemberVisual,
}

enum MemberVisual {
    Ground(SceneTone, u32),
    Prop(SceneTone, u32),
    Mark(usize),
}

#[mirui::system(order = Animation)]
pub(crate) fn mount(
    model: model!(FluxRun),
    members: query!(Entity, Member),
    commands: commands!(),
) {
    let (owner, detail, tone, level) = model.read(|model| {
        (
            model.owner,
            usize::from(!model.compact),
            model.tone,
            model.game.level,
        )
    });
    let marks = model.marks();
    let mut mounted = [false; MARKS];
    let mut terrain_mounted = false;
    for (entity, member) in members {
        if member.owner != owner {
            continue;
        }
        match member.visual {
            MemberVisual::Mark(slot) if marks.0[slot].is_some() => mounted[slot] = true,
            MemberVisual::Ground(current, previous) if current == tone && previous == level => {
                terrain_mounted = true
            }
            MemberVisual::Prop(current, previous) if current == tone && previous == level => (),
            _ => commands.despawn(entity),
        }
    }
    let stage = model.stage();
    let mut stage_commands = commands.stage(&stage);
    if !terrain_mounted {
        model.read(|model| {
            let harbor = &model.game.harbor;
            stage_commands.spawn(
                Member {
                    owner,
                    visual: MemberVisual::Ground(tone, level),
                },
                StackVisual::new(scenery::floor(harbor, detail, tone)),
            );
            for bridge in harbor.bridges().into_iter().flatten() {
                stage_commands.spawn(
                    Member {
                        owner,
                        visual: MemberVisual::Prop(tone, level),
                    },
                    StackVisual::new(bridge_model(bridge, detail))
                        .at(Pose::at(bridge.x, 0, BRIDGE_START))
                        .with_palette(&tone.palette().normal),
                );
            }
            scenery::props(harbor, detail, tone, |visual| {
                stage_commands.spawn(
                    Member {
                        owner,
                        visual: MemberVisual::Prop(tone, level),
                    },
                    visual,
                );
            });
        });
    }
    for (slot, (mark, mounted)) in marks.0.into_iter().zip(mounted).enumerate() {
        if let (Some(mark), false) = (mark, mounted) {
            stage_commands.spawn(
                Member {
                    owner,
                    visual: MemberVisual::Mark(slot),
                },
                StackVisual::from_model(model, move |handle| {
                    visual(
                        slot,
                        handle.marks().0[slot].unwrap_or(mark),
                        detail,
                        handle.tone(),
                    )
                }),
            );
        }
    }
}
