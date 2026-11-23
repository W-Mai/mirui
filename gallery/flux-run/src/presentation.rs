use crate::{
    app::FluxRun,
    gameplay::{Game, GuardPhase},
    world::{
        BRIDGE_START, CELL_SIZE, CORES, District, EXIT, GRID, GUARDS, Location, SPAWN, add, scaled,
    },
};
use mirui::{core::model::ModelHandle, prelude::*};
use mirui_sprite_stack::{
    HeightField, Pose, StackStageCommands, StackVisual, TerrainCell, TerrainGrid,
};

mod assets;
pub(crate) mod sound;
mod view;
pub(crate) use assets::SceneTone;
use assets::*;
pub(crate) use view::build;

const CORE_START: usize = 1;
const GUARD_START: usize = CORE_START + CORES;
const LINK_START: usize = GUARD_START + GUARDS;
const GATE_START: usize = LINK_START + 4;
const EXIT_MARK: usize = GATE_START + 2;
const ENTRANCE_MARK: usize = EXIT_MARK + 1;
const ALERT_START: usize = ENTRANCE_MARK + 1;
const LOCK: usize = ALERT_START + GUARDS;
const FIELD: usize = LOCK + 1;
const MARKS: usize = FIELD + 1;
pub(crate) const STAGE_CAPACITY: usize = MARKS + 2;
pub(crate) const CAMERA_YAW: Fixed = Fixed::from_int(45);

pub(crate) fn steer_on_screen(right: i8, up: i8) -> [i8; 2] {
    // The inverse of the viewport's 45-degree yaw, omitting the common scale.
    [right + up, up - right]
}

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
    fn at(location: Location) -> Self {
        let [x, z] = location.point;
        Self {
            pose: Pose::at(x, location.height(), z),
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
        if game.respawn == 0 {
            let mut ship = Mark::at(game.ship.location);
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
            let mut mark = Mark::at(core.body.location);
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
            let mut mark = Mark::at(guard.body.location);
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
                let mut mark = Mark::at(Location {
                    point: p,
                    upper: boat.upper,
                });
                mark.pose.position[1] =
                    boat.height() * (Fixed::ONE - amount) + core.height() * amount + Fixed::ONE;
                mark.flash = if (game.result.ticks / 3 + i as u32).is_multiple_of(4) {
                    Fixed::ONE
                } else {
                    Fixed::ZERO
                };
                marks[LINK_START + i] = Some(mark);
            }
        }
        marks[GATE_START] = Some(Mark::at(game.harbor.gate));
        marks[GATE_START + 1] = Some(Mark::at(game.harbor.high_gate()));
        let mut exit = Mark::at(EXIT);
        exit.tint = if game.ready() {
            Tint::Normal
        } else {
            Tint::Dim
        };
        marks[EXIT_MARK] = Some(exit);
        marks[ENTRANCE_MARK] = Some(Mark::at(SPAWN));
        if game.harbor.lock_warning(game.result.ticks) || game.harbor.lock_closed(game.result.ticks)
        {
            let mut gate = Mark::at(Location::new(game.harbor.bridge, 40, true));
            gate.tint = Tint::Warn;
            marks[LOCK] = Some(gate);
        }
        if game.harbor.district == District::Storm {
            let at = Location::new(38, 43, false);
            let mut mark = Mark::at(at);
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
    Bridge,
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
    let mut bridge_mounted = false;
    for (entity, member) in members {
        if member.owner != owner {
            continue;
        }
        match member.visual {
            MemberVisual::Mark(slot) if marks.0[slot].is_some() => mounted[slot] = true,
            MemberVisual::Ground(current, previous) if current == tone && previous == level => {
                terrain_mounted = true
            }
            MemberVisual::Bridge => bridge_mounted = true,
            _ => commands.despawn(entity),
        }
    }
    let stage = model.stage();
    let mut stage_commands = commands.stage(&stage);
    if !terrain_mounted {
        let grid =
            TerrainGrid::finite([GRID as u16; 2], Fixed::from_int(CELL_SIZE)).expect("harbor grid");
        let layout = model.read(|model| model.game.harbor.layout);
        let floor = HeightField::generate(grid, &tone.palette().terrain, |[x, z]| {
            let rim = |coord| coord == 0 || coord == GRID as i32 - 1;
            if layout.blocked(x as usize, z as usize) {
                TerrainCell {
                    level: if x % 2 == 0 { 4 } else { 5 },
                    material: 1,
                }
            } else {
                let material = if rim(x) && rim(z) {
                    0
                } else if rim(x) || rim(z) {
                    1
                } else if (x / 3 + z / 4) % 2 == 0 {
                    3
                } else {
                    2
                };
                TerrainCell { level: 0, material }
            }
        })
        .expect("harbor field");
        stage_commands.spawn(
            Member {
                owner,
                visual: MemberVisual::Ground(tone, level),
            },
            StackVisual::new(floor),
        );
    }
    if !bridge_mounted {
        stage_commands.spawn(
            Member {
                owner,
                visual: MemberVisual::Bridge,
            },
            StackVisual::from_model(model, move |handle| {
                StackVisual::new(&BRIDGE[detail])
                    .at(Pose::at(handle.bridge_x(), 0, BRIDGE_START))
                    .with_palette(&handle.tone().palette().normal)
            }),
        );
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
