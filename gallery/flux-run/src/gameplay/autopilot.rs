use super::{Controls, Game, GuardPhase, line_open};
use crate::world::{EXIT, HIGH_GATE, Location, RAMP, add, dot, length_squared, offset, scaled};
use mirui::types::Fixed;

#[derive(Default)]
pub(super) struct Pilot {
    following: Option<usize>,
    released: u16,
    retry_at: u32,
}

impl Pilot {
    pub(super) fn control(&mut self, game: &Game) -> Controls {
        if game.respawn != 0 {
            return Controls::default();
        }
        let ship = game.ship.location;
        if let Some(slot) = game.tether {
            let core = game.cores[slot].body;
            let gate = if core.location.upper {
                HIGH_GATE
            } else {
                game.harbor.gate
            };
            let to_gate = offset(gate.point, core.location.point);
            let speed = length_squared(core.velocity);
            let distance = length_squared(to_gate);
            let projection = dot(to_gate, core.velocity);
            let lateral = to_gate[0] * core.velocity[1] - to_gate[1] * core.velocity[0];
            let safe_throw = distance < Fixed::from_int(135)
                && speed > Fixed::from_ratio(4, 10)
                && projection > Fixed::ONE
                && lateral * lateral < speed * Fixed::from_int(5)
                && line_open(game.harbor, game.result.ticks, core.location, gate);
            if self.released == 0 && safe_throw && (game.result.ticks / 500).is_multiple_of(2) {
                self.released = 36;
                return steer(game, gate, false);
            }
            let mut past_gate = gate;
            past_gate.point[1] += Fixed::from_int(if core.location.point[1] <= gate.point[1] {
                7
            } else {
                -7
            });
            return steer(game, past_gate, true);
        }
        if game.magnet {
            return Controls::default();
        }
        if game.ready() {
            return steer(game, EXIT, false);
        }
        if self.released != 0 {
            self.released -= 1;
            return Controls::default();
        }
        let tick = game.result.ticks;
        if self.following.is_none_or(|slot| !game.cores[slot].active()) || tick >= self.retry_at {
            let previous = self.following;
            self.following = game
                .cores
                .iter()
                .enumerate()
                .filter(|(_, item)| item.active())
                .min_by_key(|(slot, item)| {
                    let distance = length_squared(offset(item.body.location.point, ship.point));
                    distance
                        + Fixed::from_int(if item.body.location.upper == ship.upper {
                            0
                        } else {
                            2600
                        })
                        + Fixed::from_int(if Some(*slot) == previous { 500 } else { 0 })
                })
                .map(|(slot, _)| slot);
            self.retry_at = tick.saturating_add(330);
        }
        let Some(slot) = self.following else {
            return Controls::default();
        };
        let target = game.cores[slot].body.location;
        let mut control = steer(game, target, false);
        if let Some(candidate) = game.target() {
            // Preserve the facing used to choose the visible target.
            self.following = Some(candidate);
            control = Controls {
                axis: [0; 2],
                magnet: true,
            };
        } else if ship.within(target, Fixed::from_int(3)) {
            control.axis = axis_for(offset(target.point, ship.point), Fixed::ZERO);
        }
        control
    }
}

fn steer(game: &Game, mut target: Location, magnet: bool) -> Controls {
    let ship = game.ship.location;
    if ship.upper != target.upper {
        target = if !ship.upper
            && ship.point[1] < RAMP.point[1] - Fixed::ONE
            && (ship.point[0] - RAMP.point[0]).abs() < Fixed::from_int(4)
        {
            Location::new(36, 24, true)
        } else {
            Location::new(36, 8, false)
        };
    }
    let predicted = add(ship.point, scaled(game.ship.velocity, Fixed::from_int(4)));
    let mut toward = offset(target.point, predicted);
    if let Some(guard) = game.guards[..usize::from(game.harbor.guard_count)]
        .iter()
        .find(|guard| {
            matches!(guard.phase, GuardPhase::Warning(_) | GuardPhase::Charge(_))
                && ship.within(guard.body.location, Fixed::from_int(12))
        })
    {
        let side = [-guard.body.velocity[1], guard.body.velocity[0]];
        let bias = if dot(side, toward).is_negative() {
            -12
        } else {
            12
        };
        toward = add(toward, scaled(side, Fixed::from_int(bias)));
    }
    Controls {
        axis: axis_for(toward, Fixed::from_ratio(6, 5)),
        magnet,
    }
}

fn axis_for(toward: [Fixed; 2], dead_zone: Fixed) -> [i8; 2] {
    toward.map(|v| {
        if v > dead_zone {
            1
        } else if v < -dead_zone {
            -1
        } else {
            0
        }
    })
}
