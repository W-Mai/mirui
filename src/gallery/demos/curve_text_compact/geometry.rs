use crate::prelude::*;

pub(super) const MARQUEE_CYCLE: i32 = 192;
pub(super) const TEXT_WINDOW: i32 = 400;

pub(super) static WAVE: Path = path!(
    "M -160 38 C -140 10 -100 10 -80 38 C -60 66 -20 66 0 38 C 20 10 60 10 80 38 C 100 66 140 66 160 38 C 180 10 220 10 240 38 C 260 66 300 66 320 38 C 340 10 380 10 400 38 C 420 66 460 66 480 38"
);

pub(super) fn text_path(path: PathId, phase: Fixed) -> crate::text::TextPath {
    let offset = phase * Fixed::from_int(MARQUEE_CYCLE) / Fixed::from_int(360);
    crate::text::TextPath::new(path).with_window(offset, Fixed::from_int(TEXT_WINDOW))
}
