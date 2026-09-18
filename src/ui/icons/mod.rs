//! Built-in vector icon assets with a 24×24 view box.

use crate::render::path::Path;
use mirui_macros::path;

#[derive(Clone, Copy, Debug)]
pub struct IconAsset {
    path: &'static Path,
}

impl IconAsset {
    pub const VIEWBOX: i32 = 24;

    pub const fn new(path: &'static Path) -> Self {
        Self { path }
    }

    pub const fn path(self) -> &'static Path {
        self.path
    }
}

impl From<IconAsset> for Path {
    fn from(asset: IconAsset) -> Self {
        asset.path.clone()
    }
}

// Navigation / state
static HOME_PATH: Path =
    path!(M 4 12 L 12 4 L 20 12 L 20 20 L 14 20 L 14 14 L 10 14 L 10 20 L 4 20 Z);
static CHECK_PATH: Path = path!(M 4 12 L 10 18 L 20 6 L 18 4 L 10 14 L 6 10 Z);
static CROSS_PATH: Path = path!(M 6 4 L 12 10 L 18 4 L 20 6 L 14 12 L 20 18 L 18 20 L 12 14 L 6 20 L 4 18 L 10 12 L 4 6 Z);
static PLUS_PATH: Path = path!(M 11 4 L 13 4 L 13 11 L 20 11 L 20 13 L 13 13 L 13 20 L 11 20 L 11 13 L 4 13 L 4 11 L 11 11 Z);
static MINUS_PATH: Path = path!(M 4 11 L 20 11 L 20 13 L 4 13 Z);

// Direction arrows (stem + head, single closed contour)
static ARROW_RIGHT_PATH: Path = path!(M 4 11 L 14 11 L 14 6 L 22 12 L 14 18 L 14 13 L 4 13 Z);
static ARROW_LEFT_PATH: Path = path!(M 20 11 L 10 11 L 10 6 L 2 12 L 10 18 L 10 13 L 20 13 Z);
static ARROW_UP_PATH: Path = path!(M 11 20 L 11 10 L 6 10 L 12 2 L 18 10 L 13 10 L 13 20 Z);
static ARROW_DOWN_PATH: Path = path!(M 11 4 L 11 14 L 6 14 L 12 22 L 18 14 L 13 14 L 13 4 Z);

// Chevrons (thick V shape)
static CHEVRON_RIGHT_PATH: Path = path!(M 8 4 L 16 12 L 8 20 L 6 18 L 12 12 L 6 6 Z);
static CHEVRON_LEFT_PATH: Path = path!(M 16 4 L 8 12 L 16 20 L 18 18 L 12 12 L 18 6 Z);
static CHEVRON_UP_PATH: Path = path!(M 4 16 L 12 8 L 20 16 L 18 18 L 12 12 L 6 18 Z);
static CHEVRON_DOWN_PATH: Path = path!(M 4 8 L 12 16 L 20 8 L 18 6 L 12 12 L 6 6 Z);

// 5-point star — vertices precomputed at 18° increments, alternating
// outer r=8 and inner r=3.4 from center (12,12). Even-odd fill picks
// the star body, not the interior pentagram.
static STAR_PATH: Path =
    path!(M 12 4 L 13 9 L 18 9 L 14 12 L 16 17 L 12 14 L 8 17 L 10 12 L 6 9 L 11 9 Z);

// Heart — two A-arc humps + V point. Arcs use ≤90° per the path!
// macro's cubic-bezier approximation.
static HEART_PATH: Path = path!(M 12 20 L 4 12 A 4 4 0 0 1 12 8 A 4 4 0 0 1 20 12 Z);

// Media controls
static PLAY_PATH: Path = path!(M 7 4 L 7 20 L 20 12 Z);
static PAUSE_PATH: Path = path!(M 6 4 L 10 4 L 10 20 L 6 20 Z M 14 4 L 18 4 L 18 20 L 14 20 Z);
static STOP_PATH: Path = path!(M 5 5 L 19 5 L 19 19 L 5 19 Z);

// Primitive shapes (useful as bullets / placeholders)
static CIRCLE_PATH: Path = path!(M 4 12 A 8 8 0 1 0 20 12 A 8 8 0 1 0 4 12 Z);
static SQUARE_PATH: Path = path!(M 5 5 L 19 5 L 19 19 L 5 19 Z);

pub const ICON_HOME: IconAsset = IconAsset::new(&HOME_PATH);
pub const ICON_CHECK: IconAsset = IconAsset::new(&CHECK_PATH);
pub const ICON_CROSS: IconAsset = IconAsset::new(&CROSS_PATH);
pub const ICON_PLUS: IconAsset = IconAsset::new(&PLUS_PATH);
pub const ICON_MINUS: IconAsset = IconAsset::new(&MINUS_PATH);
pub const ICON_ARROW_RIGHT: IconAsset = IconAsset::new(&ARROW_RIGHT_PATH);
pub const ICON_ARROW_LEFT: IconAsset = IconAsset::new(&ARROW_LEFT_PATH);
pub const ICON_ARROW_UP: IconAsset = IconAsset::new(&ARROW_UP_PATH);
pub const ICON_ARROW_DOWN: IconAsset = IconAsset::new(&ARROW_DOWN_PATH);
pub const ICON_CHEVRON_RIGHT: IconAsset = IconAsset::new(&CHEVRON_RIGHT_PATH);
pub const ICON_CHEVRON_LEFT: IconAsset = IconAsset::new(&CHEVRON_LEFT_PATH);
pub const ICON_CHEVRON_UP: IconAsset = IconAsset::new(&CHEVRON_UP_PATH);
pub const ICON_CHEVRON_DOWN: IconAsset = IconAsset::new(&CHEVRON_DOWN_PATH);
pub const ICON_STAR: IconAsset = IconAsset::new(&STAR_PATH);
pub const ICON_HEART: IconAsset = IconAsset::new(&HEART_PATH);
pub const ICON_PLAY: IconAsset = IconAsset::new(&PLAY_PATH);
pub const ICON_PAUSE: IconAsset = IconAsset::new(&PAUSE_PATH);
pub const ICON_STOP: IconAsset = IconAsset::new(&STOP_PATH);
pub const ICON_CIRCLE: IconAsset = IconAsset::new(&CIRCLE_PATH);
pub const ICON_SQUARE: IconAsset = IconAsset::new(&SQUARE_PATH);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::path::PathCmd;

    fn all_icons() -> [IconAsset; 20] {
        [
            ICON_HOME,
            ICON_CHECK,
            ICON_CROSS,
            ICON_PLUS,
            ICON_MINUS,
            ICON_ARROW_RIGHT,
            ICON_ARROW_LEFT,
            ICON_ARROW_UP,
            ICON_ARROW_DOWN,
            ICON_CHEVRON_RIGHT,
            ICON_CHEVRON_LEFT,
            ICON_CHEVRON_UP,
            ICON_CHEVRON_DOWN,
            ICON_STAR,
            ICON_HEART,
            ICON_PLAY,
            ICON_PAUSE,
            ICON_STOP,
            ICON_CIRCLE,
            ICON_SQUARE,
        ]
    }

    #[test]
    fn icon_set_has_twenty_icons() {
        assert_eq!(all_icons().len(), 20);
    }

    #[test]
    fn icon_asset_converts_to_a_borrowed_path() {
        let path = Path::from(ICON_HOME);
        assert!(path.is_borrowed());
        assert_eq!(IconAsset::VIEWBOX, 24);
    }

    #[test]
    fn every_icon_starts_with_moveto_and_is_closed() {
        for (i, icon) in all_icons().iter().enumerate() {
            let icon = icon.path();
            assert!(!icon.cmds.is_empty(), "icon {i} empty");
            assert!(
                matches!(icon.cmds[0], PathCmd::MoveTo(_)),
                "icon {i} missing MoveTo at index 0",
            );
            assert!(
                icon.cmds.iter().any(|c| matches!(c, PathCmd::Close)),
                "icon {i} missing Close",
            );
        }
    }
}
