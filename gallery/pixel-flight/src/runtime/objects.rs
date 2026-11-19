use super::{Flight, Terrain};
use crate::{
    flight::{Aircraft, V3},
    jet,
    raster::PlacedAircraft,
};
use mirui::types::{Fixed, Fixed64};
use mirui_sprite_stack::{Geometry, Object, Pose, StackScene, StackSceneOwned};

pub(super) struct FlightObjects {
    ground: Object,
    aircraft: [Option<Object>; 3],
}

fn degrees(radians: Fixed) -> Fixed {
    (Fixed64::from(radians) * 180 / Fixed64::from(Fixed::PI)).to_fixed()
}
fn radians(degrees: Fixed) -> Fixed {
    (Fixed64::from(degrees) * Fixed64::from(Fixed::PI) / 180 + Fixed64::from_ratio(1, 512))
        .to_fixed()
}

fn pose(aircraft: &Aircraft) -> Pose {
    let [x, y, altitude] = aircraft.position;
    Pose {
        position: [x, altitude, -y],
        yaw: degrees(aircraft.yaw),
        pitch: degrees(aircraft.pitch),
        roll: -degrees(aircraft.roll + aircraft.spin()),
    }
}

impl FlightObjects {
    pub fn ground(&self) -> Object {
        self.ground
    }
    pub fn supports_terrain(&self, scene: &StackScene, terrain: &Terrain) -> bool {
        scene
            .with_object(self.ground, |source, pose| {
                matches!(source,Geometry::Field(field) if terrain.uses_field(field))
                    && pose == Pose::default()
            })
            .unwrap_or(false)
    }
    pub fn player(&self) -> Object {
        self.aircraft[0].expect("the player aircraft is always active")
    }
    pub fn new(terrain: &Terrain, flight: &Flight) -> (StackScene, Self) {
        let mut scene = StackScene::bounded(4);
        let ground = scene
            .place(terrain.field(), Pose::default())
            .expect("prepared terrain slot");
        let aircraft = core::array::from_fn(|i| {
            (i < flight.count).then(|| {
                scene
                    .place(&jet::MODEL, pose(&flight.jets[i]))
                    .expect("prepared aircraft slot")
            })
        });
        (scene, Self { ground, aircraft })
    }
    pub fn sync(&mut self, scene: &mut StackSceneOwned, flight: &mut Flight) {
        for (i, entry) in self.aircraft.iter_mut().enumerate() {
            match (*entry, i < flight.count) {
                (Some(id), true) => {
                    let _ = scene.set_pose(id, pose(&flight.jets[i]));
                }
                (None, true) => match scene.place(&jet::MODEL, pose(&flight.jets[i])) {
                    Ok(id) => *entry = Some(id),
                    Err(reason) => {
                        flight.count = i;
                        mirui::warn!("flight wingman placement failed: {reason:?}");
                    }
                },
                (Some(id), false) => {
                    let _ = scene.remove(id);
                    *entry = None;
                }
                (None, false) => {}
            }
        }
    }
    pub fn placed(&self, scene: &StackScene) -> [Option<PlacedAircraft>; 3] {
        self.aircraft.map(|id| {
            scene.with_object(id?, |geometry, p| {
                let Geometry::Prepared(model) = geometry else {
                    return None;
                };
                let [x, altitude, z] = p.position;
                let position: V3 = [x, -z, altitude];
                Some(PlacedAircraft {
                    position,
                    rotation: [p.yaw, p.pitch, -p.roll].map(radians),
                    model,
                })
            })?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flight_angles_round_trip_through_scene_pose() {
        for q in -3500..=3500 {
            let v = Fixed::from_ratio(q, 256);
            assert_eq!(radians(degrees(v)), v);
        }
    }
}
