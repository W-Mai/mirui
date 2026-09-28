use crate::render::scene::SceneOp;
use crate::scene;

pub(super) const LOGICAL_WIDTH: i32 = 480;
pub(super) const LOGICAL_HEIGHT: i32 = 240;

pub(super) const SCENE: &[SceneOp] = scene! {
    rect 0 0 480 240 0 18 20 28 255 255;

    group filter "blur:3:3" disjoint;
    rect 28 58 118 58 14 255 80 110 255 220;
    rect 82 104 92 60 14 90 210 255 255 220;
    rect 130 44 68 100 14 255 200 80 255 220;
    fill_path {
        M 156 128;
        C 156 152 136 172 112 172;
        C 88 172 68 152 68 128;
        C 68 104 88 84 112 84;
        C 136 84 156 104 156 128;
        Z
    } 145 255 120 255 230 fill_rule evenodd;
    endgroup;

    rect 270 58 118 58 14 255 80 110 255 220;
    rect 324 104 92 60 14 90 210 255 255 220;
    rect 372 44 68 100 14 255 200 80 255 220;
    fill_path {
        M 398 128;
        C 398 152 378 172 354 172;
        C 330 172 310 152 310 128;
        C 310 104 330 84 354 84;
        C 378 84 398 104 398 128;
        Z
    } 145 255 120 255 230 fill_rule evenodd;
};
