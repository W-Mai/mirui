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
    pub fn player(&self) -> Object {
        self.aircraft[0].expect("the player aircraft is always active")
    }
    pub fn new(terrain: &Terrain, flight: &Flight) -> (StackScene, Self) {
        let mut scene = StackScene::with_capacity(4);
        let ground = scene
            .place(terrain.field(), Pose::default())
            .expect("prepared terrain slot");
        let aircraft = core::array::from_fn(|i| {
            (i < flight.count).then(|| {
                scene
                    .place(Geometry::Sampled(&jet::MODEL), pose(&flight.jets[i]))
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
                (None, true) => {
                    match scene.place(Geometry::Sampled(&jet::MODEL), pose(&flight.jets[i])) {
                        Ok(id) => *entry = Some(id),
                        Err(reason) => {
                            flight.count = i;
                            mirui::warn!("flight wingman placement failed: {reason:?}");
                        }
                    }
                }
                (Some(id), false) => {
                    let _ = scene.remove(id);
                    *entry = None;
                }
                (None, false) => {}
            }
        }
    }
    pub fn managed(&self, draws_ground: bool) -> [Option<Object>; 4] {
        [
            draws_ground.then_some(self.ground),
            self.aircraft[0],
            self.aircraft[1],
            self.aircraft[2],
        ]
    }
    pub fn placed(&self, scene: &StackScene) -> [Option<PlacedAircraft>; 3] {
        self.aircraft.map(|id| {
            scene.with_object(id?, |geometry, p| {
                let Geometry::Sampled(model) = geometry else {
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
    fn placed_ground_uses_the_flight_camera_depth_and_live_materials() {
        use crate::math::FixedMath;
        use mirui::{core::model::ModelHandle, prelude::*};
        use mirui_sprite_stack::{HeightField, TerrainCell, TerrainGrid};
        const COLORS: [Color; 2] = [Color::rgb(255, 0, 255), Color::rgb(0, 255, 0)];
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let game = super::super::setup_compact_app(&mut app, root);
        let grid = TerrainGrid::finite([1, 1], Fixed::from_int(24)).unwrap();
        let field = app
            .add_model(
                HeightField::generate(grid, &COLORS, |_| TerrainCell {
                    level: 0,
                    material: 0,
                })
                .unwrap(),
            )
            .finish();
        let scene = game.scene();
        let player = scene.pose(game.player()).unwrap();
        let mut pose = player;
        pose.position[0] += Fixed::from_int(42);
        pose.yaw = Fixed::ZERO;
        let point = |pose: Pose| {
            game.read(|g| {
                let p = g.flight.projection(128, 128);
                let [x, y, z] = pose.position;
                let [sx, sy, _, _] = p
                    .project([x + Fixed::from_int(12), -(z + Fixed::from_int(12)), y])
                    .unwrap();
                (sx.round().to_int(), sy.round().to_int())
            })
        };
        let a = point(pose);
        let before = app.snapshot().unwrap().unwrap();
        let sample = |frame: &mirui::render::texture::Texture, (x, y): (i32, i32)| {
            frame.read_pixels().unwrap().get_pixel(x, y)
        };
        let object = scene.place(field.clone(), pose).unwrap();
        let added = app.snapshot().unwrap().unwrap();
        assert_eq!(sample(&added, a), COLORS[0], "field at {a:?}");
        field
            .set_cell(
                [0, 0],
                TerrainCell {
                    level: 0,
                    material: 1,
                },
            )
            .unwrap();
        let changed = app.snapshot().unwrap().unwrap();
        assert_eq!(sample(&changed, a), COLORS[1]);
        let back = game.read(|g| {
            let (s, c) = g.flight.projection(128, 128).yaw.sin_cos();
            let mut back = pose;
            back.position[0] -= s * 30;
            back.position[2] += c * 30;
            back.position[1] -= Fixed::from_int(30);
            back
        });
        let rear = scene
            .place(
                HeightField::generate(grid, &COLORS, |_| TerrainCell {
                    level: 0,
                    material: 0,
                })
                .unwrap(),
                back,
            )
            .unwrap();
        let overlapped = app.snapshot().unwrap().unwrap();
        assert_eq!(
            sample(&overlapped, a),
            COLORS[1],
            "near field covers a later placement"
        );
        pose.position[0] -= Fixed::from_int(96);
        scene.set_pose(object, pose).unwrap();
        let b = point(pose);
        let moved = app.snapshot().unwrap().unwrap();
        assert_eq!(
            sample(&moved, a),
            COLORS[0],
            "moving the near field exposes the far field"
        );
        scene.remove(rear).unwrap();
        assert_eq!(sample(&moved, b), COLORS[1]);
        scene.remove(object).unwrap();
        let after = app.snapshot().unwrap().unwrap();
        assert_eq!(sample(&after, b), sample(&before, b));
        assert_eq!(sample(&after, a), sample(&before, a));
        // A repeating surface shares the flight ray sampler over its full visible distance.
        let repeating = HeightField::generate(
            TerrainGrid::repeating([2, 1], Fixed::from_int(16)).unwrap(),
            &COLORS,
            |_| TerrainCell {
                level: 0,
                material: 0,
            },
        )
        .unwrap();
        let sheet = scene
            .place(
                repeating,
                Pose::at(pose.position[0], player.position[1], pose.position[2]),
            )
            .unwrap();
        let visible = app.snapshot().unwrap().unwrap();
        assert_eq!(sample(&visible, a), COLORS[0]);
        scene
            .set_pose(sheet, Pose::at(pose.position[0], -250, pose.position[2]))
            .unwrap();
        let buried = app.snapshot().unwrap().unwrap();
        assert_eq!(
            sample(&buried, a),
            sample(&before, a),
            "the original ground hides a lower repeating surface"
        );
    }
    #[test]
    fn the_flight_view_draws_placed_atlases_and_both_sparse_coverage_modes() {
        use mirui::{core::cache::Handle, prelude::*};
        use mirui_sprite_stack::{Geometry, SpriteStackAsset};
        let atlas = Handle::new(
            SpriteStackAsset::from_mirx(include_bytes!("../../assets/jet.mirx")).unwrap(),
        );
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let game = super::super::setup_compact_app(&mut app, root);
        let scene = game.scene();
        // Expanded layers turn off exhaust so its visibility smoothing cannot change a paused frame.
        game.key(mirui::input::event::input::PhysicalKey::KeyE, true, false);
        // Fill every flight slot before adding a game-authored object.
        game.key(mirui::input::event::input::PhysicalKey::KeyF, true, false);
        game.key(mirui::input::event::input::PhysicalKey::KeyF, true, false);
        game.advance(16, 128, 128, None);
        for camera in [crate::flight::View::Overhead, crate::flight::View::Chase] {
            game.select_view(camera);
            for _ in 0..180 {
                game.advance(16, 128, 128, None);
            }
            let mut pose = scene.pose(game.player()).unwrap();
            pose.position[0] += Fixed::from_int(30);
            pose.position[1] += Fixed::from_int(8);
            for geometry in [
                Geometry::Sprite(atlas.clone()),
                Geometry::Prepared(&jet::MODEL),
                Geometry::Sampled(&jet::MODEL),
            ] {
                let before = app.snapshot().unwrap().unwrap();
                let id = scene.place(geometry, pose).unwrap();
                let added = app.snapshot().unwrap().unwrap();
                let pixels = added.read_pixels().unwrap();
                let before = before.read_pixels().unwrap();
                assert!(
                    pixels
                        .bytes()
                        .chunks_exact(4)
                        .zip(before.bytes().chunks_exact(4))
                        .filter(|(a, b)| a != b)
                        .count()
                        > 8,
                    "camera {camera:?}"
                );
                scene.remove(id).unwrap();
                let restored = app.snapshot().unwrap().unwrap();
                assert_eq!(
                    restored
                        .read_pixels()
                        .unwrap()
                        .bytes()
                        .iter()
                        .zip(before.bytes())
                        .position(|(a, b)| a != b),
                    None,
                    "camera {camera:?}"
                );
            }
        }
    }
    #[test]
    fn flight_angles_round_trip_through_scene_pose() {
        for q in -3500..=3500 {
            let v = Fixed::from_ratio(q, 256);
            assert_eq!(radians(degrees(v)), v);
        }
    }
}
