use super::types::OrbitBody;
use crate::types::{Fixed, Fixed64};

pub(super) const MU: Fixed64 = Fixed64::from_int(100_000);
pub(super) const STEP: Fixed64 = Fixed64::from_ratio(1, 60);

pub(super) fn integrate(body: &mut OrbitBody, dt: Fixed64) {
    let radius = body.radius().max(Fixed64::from_int(20));
    let factor = -MU / (radius * radius * radius);
    body.velocity.x += body.position.x * factor * dt / 2;
    body.velocity.y += body.position.y * factor * dt / 2;
    body.position.x += body.velocity.x * dt;
    body.position.y += body.velocity.y * dt;
    let radius = body.radius().max(Fixed64::from_int(20));
    let factor = -MU / (radius * radius * radius);
    body.velocity.x += body.position.x * factor * dt / 2;
    body.velocity.y += body.position.y * factor * dt / 2;
}

pub(super) fn impulse(body: &mut OrbitBody, dv: Fixed64, angle_degrees: i16) {
    let speed = body.speed().max(Fixed64::ONE);
    let ux = body.velocity.x / speed;
    let uy = body.velocity.y / speed;
    let angle = Fixed::from_int(i32::from(angle_degrees));
    let cosine = Fixed64::from_fixed(Fixed::cos_deg(angle));
    let sine = Fixed64::from_fixed(Fixed::sin_deg(angle));
    body.velocity.x += dv * (ux * cosine - uy * sine);
    body.velocity.y += dv * (ux * sine + uy * cosine);
}
