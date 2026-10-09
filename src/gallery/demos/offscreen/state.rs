const WIN_W: i32 = 360;
const WIN_H: i32 = 360;

pub const DEFAULT_VIEW: (u16, u16) = (WIN_W as u16, WIN_H as u16);

#[crate::component]
pub struct PanelTarget;

#[crate::component]
pub struct FpsReadout {
    pub counter: u32,
    pub accum_render_ns: u64,
}

pub struct ModeToggle {
    pub last_flip_ns: u64,
    pub elapsed_ns: u64,
    pub offscreen: bool,
}
