use super::state::{BASE_STAGE_HEIGHT, BASE_STAGE_WIDTH};
use crate::prelude::*;
use crate::render::path::{Path, PathCmd, PathId};

#[derive(Clone, Copy)]
pub(super) struct CurvePaths {
    pub(super) ids: [PathId; 3],
}

pub(super) fn path_window_end(width: Fixed) -> Fixed {
    (width * Fixed::from_ratio(89, 100)).max(Fixed::from_int(100))
}

fn lane_commands(
    lane: usize,
    phase: Fixed,
    amplitude: Fixed,
    stage_width: Fixed,
    stage_height: Fixed,
) -> [PathCmd; 3] {
    let lane_scale = match lane {
        0 => Fixed::ONE,
        1 => Fixed::from_ratio(3, 4),
        _ => Fixed::from_ratio(1, 2),
    };
    let lane_phase = phase + Fixed::from_int(lane as i32 * 71);
    let geometry_scale = (stage_width / BASE_STAGE_WIDTH)
        .min(stage_height / BASE_STAGE_HEIGHT)
        .max(Fixed::from_ratio(1, 4));
    let wave = amplitude * lane_scale * geometry_scale;
    let x = |value| stage_width * Fixed::from_ratio(value, 916);
    let y = |value| stage_height * Fixed::from_ratio(value, 360);
    let base_y = y(104 + lane as i32 * 106);
    let start = Point {
        x: x(34),
        y: base_y + Fixed::sin_deg(lane_phase - Fixed::from_int(38)) * wave / 3,
    };
    let middle = Point {
        x: x(456),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(124)) * wave / 2,
    };
    let end = Point {
        x: x(878),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(286)) * wave / 3,
    };
    [
        PathCmd::MoveTo(start),
        PathCmd::CubicTo {
            ctrl1: Point {
                x: x(168),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(12)) * wave,
            },
            ctrl2: Point {
                x: x(316),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(82)) * wave,
            },
            end: middle,
        },
        PathCmd::CubicTo {
            ctrl1: Point {
                x: x(596),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(168)) * wave,
            },
            ctrl2: Point {
                x: x(744),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(238)) * wave,
            },
            end,
        },
    ]
}

pub(super) fn make_lane(lane: usize) -> Path {
    let [start, first, second] = lane_commands(
        lane,
        Fixed::ZERO,
        Fixed::from_int(68),
        BASE_STAGE_WIDTH,
        BASE_STAGE_HEIGHT,
    );
    let mut path = Path::try_with_capacity(3).expect("curve path storage");
    let PathCmd::MoveTo(start) = start else {
        unreachable!();
    };
    let PathCmd::CubicTo { ctrl1, ctrl2, end } = first else {
        unreachable!();
    };
    path.move_to(start).cubic_to(ctrl1, ctrl2, end);
    let PathCmd::CubicTo { ctrl1, ctrl2, end } = second else {
        unreachable!();
    };
    path.cubic_to(ctrl1, ctrl2, end);
    path
}

pub(super) fn update_lane_for_stage(
    path: &mut Path,
    lane: usize,
    phase: Fixed,
    amplitude: Fixed,
    stage_width: Fixed,
    stage_height: Fixed,
) {
    for (index, command) in lane_commands(lane, phase, amplitude, stage_width, stage_height)
        .into_iter()
        .enumerate()
    {
        path.set_command(index, command)
            .expect("curve path topology remains fixed");
    }
}

pub(super) fn register_paths(world: &mut World) -> CurvePaths {
    let mut paths = world.paths();
    CurvePaths {
        ids: [
            paths.insert(make_lane(0)).expect("first curve path"),
            paths.insert(make_lane(1)).expect("second curve path"),
            paths.insert(make_lane(2)).expect("third curve path"),
        ],
    }
}
