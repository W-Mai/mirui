use crate::flight::Aircraft;
use mirui::types::{Fixed, Fixed64};
use mirui_sprite_stack::Pose;

fn degrees(radians: Fixed) -> Fixed {
    (Fixed64::from(radians) * 180 / Fixed64::from(Fixed::PI)).to_fixed()
}
pub(crate) fn radians(degrees: Fixed) -> Fixed {
    (Fixed64::from(degrees) * Fixed64::from(Fixed::PI) / 180 + Fixed64::from_ratio(1, 512))
        .to_fixed()
}

pub(crate) fn position(aircraft: &Aircraft) -> [Fixed; 3] {
    let [x, y, altitude] = aircraft.position;
    [x, altitude, -y]
}

pub(crate) fn pose(aircraft: &Aircraft) -> Pose {
    Pose {
        position: position(aircraft),
        yaw: degrees(aircraft.yaw),
        pitch: degrees(aircraft.pitch),
        roll: -degrees(aircraft.roll + aircraft.spin()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/world/coordinates.rs"]
mod tests;
