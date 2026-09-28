use crate::prelude::*;

pub struct FlipCard {
    pub angle_deg: Fixed,
    pub speed_deg_per_second: Fixed,
    pub front_color: ColorToken,
    pub back_color: ColorToken,
    pub root: Entity,
}
