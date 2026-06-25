//! Built-in vector icon set — 20 hand-drawn, 24×24 viewBox, closed-path
//! filled glyphs. Each `Path` is const-emitted by the `path!` macro,
//! so a static icon costs zero allocation.

use crate::render::path::Path;
use mirui_macros::path;

// Navigation / state
pub static ICON_HOME: Path =
    path!(M 4 12 L 12 4 L 20 12 L 20 20 L 14 20 L 14 14 L 10 14 L 10 20 L 4 20 Z);
pub static ICON_CHECK: Path = path!(M 4 12 L 10 18 L 20 6 L 18 4 L 10 14 L 6 10 Z);
pub static ICON_CROSS: Path = path!(M 6 4 L 12 10 L 18 4 L 20 6 L 14 12 L 20 18 L 18 20 L 12 14 L 6 20 L 4 18 L 10 12 L 4 6 Z);
pub static ICON_PLUS: Path = path!(M 11 4 L 13 4 L 13 11 L 20 11 L 20 13 L 13 13 L 13 20 L 11 20 L 11 13 L 4 13 L 4 11 L 11 11 Z);
pub static ICON_MINUS: Path = path!(M 4 11 L 20 11 L 20 13 L 4 13 Z);

// Direction arrows (stem + head, single closed contour)
pub static ICON_ARROW_RIGHT: Path = path!(M 4 11 L 14 11 L 14 6 L 22 12 L 14 18 L 14 13 L 4 13 Z);
pub static ICON_ARROW_LEFT: Path = path!(M 20 11 L 10 11 L 10 6 L 2 12 L 10 18 L 10 13 L 20 13 Z);
pub static ICON_ARROW_UP: Path = path!(M 11 20 L 11 10 L 6 10 L 12 2 L 18 10 L 13 10 L 13 20 Z);
pub static ICON_ARROW_DOWN: Path = path!(M 11 4 L 11 14 L 6 14 L 12 22 L 18 14 L 13 14 L 13 4 Z);

// Chevrons (thick V shape)
pub static ICON_CHEVRON_RIGHT: Path = path!(M 8 4 L 16 12 L 8 20 L 6 18 L 12 12 L 6 6 Z);
pub static ICON_CHEVRON_LEFT: Path = path!(M 16 4 L 8 12 L 16 20 L 18 18 L 12 12 L 18 6 Z);
pub static ICON_CHEVRON_UP: Path = path!(M 4 16 L 12 8 L 20 16 L 18 18 L 12 12 L 6 18 Z);
pub static ICON_CHEVRON_DOWN: Path = path!(M 4 8 L 12 16 L 20 8 L 18 6 L 12 12 L 6 6 Z);

// 5-point star — vertices precomputed at 18° increments, alternating
// outer r=8 and inner r=3.4 from center (12,12). Even-odd fill picks
// the star body, not the interior pentagram.
pub static ICON_STAR: Path =
    path!(M 12 4 L 13 9 L 18 9 L 14 12 L 16 17 L 12 14 L 8 17 L 10 12 L 6 9 L 11 9 Z);

// Heart — two A-arc humps + V point. Arcs use ≤90° per the path!
// macro's cubic-bezier approximation.
pub static ICON_HEART: Path = path!(M 12 20 L 4 12 A 4 4 0 0 1 12 8 A 4 4 0 0 1 20 12 Z);

// Media controls
pub static ICON_PLAY: Path = path!(M 7 4 L 7 20 L 20 12 Z);
pub static ICON_PAUSE: Path = path!(M 6 4 L 10 4 L 10 20 L 6 20 Z M 14 4 L 18 4 L 18 20 L 14 20 Z);
pub static ICON_STOP: Path = path!(M 5 5 L 19 5 L 19 19 L 5 19 Z);

// Primitive shapes (useful as bullets / placeholders)
pub static ICON_CIRCLE: Path = path!(M 4 12 A 8 8 0 1 0 20 12 A 8 8 0 1 0 4 12 Z);
pub static ICON_SQUARE: Path = path!(M 5 5 L 19 5 L 19 19 L 5 19 Z);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::path::PathCmd;

    fn all_icons() -> [&'static Path; 20] {
        [
            &ICON_HOME,
            &ICON_CHECK,
            &ICON_CROSS,
            &ICON_PLUS,
            &ICON_MINUS,
            &ICON_ARROW_RIGHT,
            &ICON_ARROW_LEFT,
            &ICON_ARROW_UP,
            &ICON_ARROW_DOWN,
            &ICON_CHEVRON_RIGHT,
            &ICON_CHEVRON_LEFT,
            &ICON_CHEVRON_UP,
            &ICON_CHEVRON_DOWN,
            &ICON_STAR,
            &ICON_HEART,
            &ICON_PLAY,
            &ICON_PAUSE,
            &ICON_STOP,
            &ICON_CIRCLE,
            &ICON_SQUARE,
        ]
    }

    #[test]
    fn icon_set_has_twenty_icons() {
        assert_eq!(all_icons().len(), 20);
    }

    #[test]
    fn every_icon_starts_with_moveto_and_is_closed() {
        for (i, icon) in all_icons().iter().enumerate() {
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
