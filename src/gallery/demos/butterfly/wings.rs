//! Outlines of the butterfly, drawn once for its right side and mirrored for
//! the left. The origin is the thorax and one unit is about half a percent of
//! the wingspan.

use crate::path;
use crate::prelude::draw::*;

/// Radius-one disc, scaled to a spot or an ellipse.
pub(super) static DISC: Path = path!(
    "M 1 0 C 1 0.5523 0.5523 1 0 1 C -0.5523 1 -1 0.5523 -1 0 C -1 -0.5523 -0.5523 -1 0 -1 C 0.5523 -1 1 -0.5523 1 0 Z"
);

pub(super) static FOREWING: Path =
    path!("M 6 -4 C 20 -46 58 -86 100 -74 C 108 -46 96 -6 66 12 C 44 22 22 16 6 8 Z");
pub(super) static FOREWING_GLOW: Path =
    path!("M 10 -6 C 22 -36 52 -62 84 -58 C 88 -38 76 -10 52 6 C 36 12 20 10 10 6 Z");
pub(super) static FOREWING_VEINS: Path = path!("M 8 0 L 94 -66 M 8 2 L 98 -38 M 8 5 L 80 8");

/// The hindwing ends in a tail at the bottom.
pub(super) static HINDWING: Path =
    path!("M 6 6 C 30 4 62 8 76 30 C 84 52 66 70 50 66 C 42 78 28 92 20 98 C 18 78 12 50 6 28 Z");
pub(super) static HINDWING_GLOW: Path = path!(
    "M 10 8 C 30 8 54 14 62 32 C 66 46 54 56 44 52 C 34 58 24 70 18 76 C 16 60 12 40 10 26 Z"
);
pub(super) static HINDWING_VEINS: Path = path!("M 8 10 L 68 34 M 8 16 L 48 62");

/// Spots along the outer edge of the forewing: centre x, centre y, radius.
pub(super) const FOREWING_SPOTS: [(i32, i32, i32); 3] = [(82, -52, 5), (92, -30, 4), (80, -12, 3)];
/// The eye spot on the hindwing.
pub(super) const EYE_SPOT: (i32, i32) = (58, 44);

/// Tapered abdomen, thorax and head in one outline.
pub(super) static BODY: Path = path!(
    "M 0 -38 C 5 -38 7 -32 6 -26 C 9 -20 8 -8 5 0 C 5 30 3 62 0 76 C -3 62 -5 30 -5 0 C -8 -8 -9 -20 -6 -26 C -7 -32 -5 -38 0 -38 Z"
);

/// Both antennae, curving out from the head.
pub(super) static ANTENNAE: Path =
    path!("M -2 -36 C -4 -54 -14 -66 -28 -72 M 2 -36 C 4 -54 14 -66 28 -72");
pub(super) const ANTENNA_TIPS: [(i32, i32); 2] = [(-28, -72), (28, -72)];
