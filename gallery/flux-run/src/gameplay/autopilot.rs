use super::{Controls, Game, GuardPhase, line_open};
use crate::world::{
    BRIDGE_START, EXIT, Layout, Location, add, dot, length_squared, offset, scaled,
};
use mirui::types::Fixed;

#[derive(Default)]
pub(super) struct Pilot {
    following: Option<usize>,
    released: u16,
    retry_at: u32,
    route: Option<Route>,
    towing: Option<(usize, u32)>,
    avoid: Option<(usize, u32)>,
}

struct Route {
    cells: [[usize; 2]; 2],
    waypoint: [Fixed; 2],
}

impl Pilot {
    pub(super) fn control(&mut self, game: &Game) -> Controls {
        if game.respawn != 0 {
            return Controls::default();
        }
        let ship = game.ship.location;
        let tick = game.result.ticks;
        if let Some(slot) = game.tether {
            if self.towing.is_none_or(|(old, _)| old != slot) {
                self.towing = Some((slot, tick));
            }
            if let Some((_, started)) = self.towing
                && tick.saturating_sub(started) >= 420
            {
                self.avoid = Some((slot, tick.saturating_add(540)));
                self.towing = None;
                self.following = None;
                self.released = 40;
                return Controls::default();
            }
            let core = game.cores[slot].body;
            let gate = if core.location.upper {
                game.harbor.high_gate()
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
                && line_open(&game.harbor, game.result.ticks, core.location, gate);
            if self.released == 0 && safe_throw && (game.result.ticks / 500).is_multiple_of(2) {
                self.released = 36;
                return self.steer(game, gate, false);
            }
            let mut past_gate = gate;
            past_gate.point[1] += Fixed::from_int(if core.location.point[1] <= gate.point[1] {
                7
            } else {
                -7
            });
            return self.steer(game, past_gate, true);
        }
        self.towing = None;
        if game.magnet {
            return Controls::default();
        }
        if game.ready() {
            let near = ship.point[1] < EXIT.point[1];
            let aligned = (ship.point[0] - EXIT.point[0]).abs() <= Fixed::from_int(3);
            let mut goal = EXIT;
            goal.point[1] += Fixed::from_int(if near == aligned { 6 } else { -6 });
            return self.steer(game, goal, false);
        }
        if self.released != 0 {
            self.released -= 1;
            return Controls::default();
        }
        let avoid = self.avoid;
        let available = |slot| avoid.is_none_or(|(avoid, until)| avoid != slot || tick >= until);
        if self.following.is_none_or(|slot| !game.cores[slot].active()) || tick >= self.retry_at {
            let previous = self.following;
            self.following = game
                .cores
                .iter()
                .enumerate()
                .filter(|(slot, item)| item.active() && available(*slot))
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
        let mut control = self.steer(game, target, false);
        if let Some(candidate) = game.target().filter(|&candidate| available(candidate)) {
            // Preserve the facing used to choose the visible target.
            self.following = Some(candidate);
            control = Controls {
                axis: [Fixed::ZERO; 2],
                magnet: true,
            };
        } else if ship.within(target, Fixed::from_int(3))
            && line_open(&game.harbor, tick, ship, target)
        {
            control.axis = axis_for(offset(target.point, ship.point), Fixed::ZERO);
        }
        control
    }
    fn steer(&mut self, game: &Game, mut target: Location, magnet: bool) -> Controls {
        let ship = game.ship.location;
        if ship.upper != target.upper {
            let bridge = game.harbor.bridge;
            let ramp = Fixed::from_int(BRIDGE_START);
            target = if !ship.upper
                && ship.point[1] < ramp
                && (ship.point[0] - Fixed::from_int(bridge)).abs() <= Fixed::from_int(3)
            {
                Location::new(bridge, 24, true)
            } else {
                Location::new(bridge, 8, false)
            };
        }
        let mut routing = false;
        if !ship.upper && !target.upper && !game.harbor.layout.direct(ship.point, target.point) {
            let cells = [Layout::cell(ship.point), Layout::cell(target.point)];
            if self.route.as_ref().is_none_or(|route| route.cells != cells) {
                self.route = game
                    .harbor
                    .layout
                    .way_to(ship.point, target.point)
                    .map(|waypoint| Route { cells, waypoint });
            }
            if let Some(route) = &self.route {
                routing = true;
                target.point = if game.harbor.layout.direct(ship.point, route.waypoint) {
                    route.waypoint
                } else {
                    Layout::center(cells[0])
                };
            }
        }
        let predicted = add(ship.point, scaled(game.ship.velocity, Fixed::from_int(4)));
        let mut toward = offset(target.point, predicted);
        if !routing
            && let Some(guard) = game.guards[..usize::from(game.harbor.guard_count)]
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
            axis: axis_for(
                toward,
                if routing {
                    Fixed::HALF
                } else {
                    Fixed::from_ratio(6, 5)
                },
            ),
            magnet,
        }
    }
}

fn axis_for(toward: [Fixed; 2], dead_zone: Fixed) -> [Fixed; 2] {
    toward.map(|v| {
        if v > dead_zone {
            Fixed::ONE
        } else if v < -dead_zone {
            -Fixed::ONE
        } else {
            Fixed::ZERO
        }
    })
}
