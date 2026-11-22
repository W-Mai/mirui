use crate::{
    app::FluxRun,
    gameplay::{Game, GuardPhase},
    world::{CORES, District, EXIT, GUARDS, HIGH_GATE, Location, add, scaled},
};
use mirui::{core::model::ModelHandle, prelude::*};
use mirui_sprite_stack::{
    HeightField, Pose, StackStageCommands, StackVisual, TerrainCell, TerrainGrid,
};

mod assets;
pub(crate) mod sound;
mod view;
use assets::*;
pub(crate) use view::build;

const CORE_START: usize = 1;
const GUARD_START: usize = CORE_START + CORES;
const LINK_START: usize = GUARD_START + GUARDS;
const GATE_START: usize = LINK_START + 4;
const ALERT_START: usize = GATE_START + 3;
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
        marks[GATE_START + 1] = Some(Mark::at(HIGH_GATE));
        let mut exit = Mark::at(EXIT);
        exit.tint = if game.ready() {
            Tint::Normal
        } else {
            Tint::Dim
        };
        marks[GATE_START + 2] = Some(exit);
        if game.harbor.lock_warning(game.result.ticks) || game.harbor.lock_closed(game.result.ticks)
        {
            let mut gate = Mark::at(Location::new(36, 40, true));
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
    -Fixed::atan2(x, z) * Fixed::from_ratio(57296, 1000)
}

fn visual(slot: usize, mark: Mark) -> StackVisual {
    let asset = match slot {
        0 => &SHIP,
        CORE_START..GUARD_START => &CORE,
        GUARD_START..LINK_START => &GUARD,
        LINK_START..GATE_START => &LINK,
        GATE_START..ALERT_START | LOCK | FIELD => &GATE,
        _ => &MARK,
    };
    let palette = match mark.tint {
        Tint::Normal => &ART,
        Tint::Warn => &WARNING,
        Tint::Dim => &LOCKED,
    };
    StackVisual::new(asset)
        .at(mark.pose)
        .with_palette(palette)
        .with_flash(mark.flash)
}

#[mirui::component]
struct Member {
    owner: Entity,
    slot: Option<usize>,
}

#[mirui::system(order = Animation)]
pub(crate) fn mount(
    model: model!(FluxRun),
    members: query!(Entity, Member),
    commands: commands!(),
) {
    let owner = model.read(|model| model.owner);
    let marks = model.marks();
    let mut mounted = [false; MARKS];
    let mut terrain_mounted = false;
    for (entity, member) in members {
        if member.owner != owner {
            continue;
        }
        if let Some(slot) = member.slot {
            if marks.0[slot].is_some() {
                mounted[slot] = true;
            } else {
                commands.despawn(entity);
            }
        } else {
            terrain_mounted = true;
        }
    }
    let stage = model.stage();
    let mut stage_commands = commands.stage(&stage);
    if !terrain_mounted {
        let grid = TerrainGrid::finite([9, 9], Fixed::from_int(8)).expect("harbor grid");
        let floor = HeightField::generate(grid, &[WATER, FOAM, PANEL, STEEL], |[x, z]| {
            let edge = x == 0 || x == 8 || z == 0 || z == 8;
            TerrainCell {
                level: 0,
                material: if edge {
                    if (x + z) % 3 == 0 { 1 } else { 0 }
                } else {
                    2 + ((x + z) % 2) as u8
                },
            }
        })
        .expect("harbor field");
        stage_commands.spawn(Member { owner, slot: None }, StackVisual::new(floor));
        stage_commands.spawn(
            Member { owner, slot: None },
            StackVisual::new(&BRIDGE).at(Pose::at(36, 0, 17)),
        );
    }
    for (slot, (mark, mounted)) in marks.0.into_iter().zip(mounted).enumerate() {
        if let (Some(mark), false) = (mark, mounted) {
            stage_commands.spawn(
                Member {
                    owner,
                    slot: Some(slot),
                },
                StackVisual::from_model(model, move |handle| {
                    visual(slot, handle.marks().0[slot].unwrap_or(mark))
                }),
            );
        }
    }
}
