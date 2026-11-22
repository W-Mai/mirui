use crate::flight::Aircraft;
use mirui::types::{Fixed, Fixed64};
use mirui_sprite_stack::Pose;

fn degrees(radians: Fixed) -> Fixed {
    (Fixed64::from(radians) * 180 / Fixed64::from(Fixed::PI)).to_fixed()
}
pub(super) fn radians(degrees: Fixed) -> Fixed {
    (Fixed64::from(degrees) * Fixed64::from(Fixed::PI) / 180 + Fixed64::from_ratio(1, 512))
        .to_fixed()
}

pub(super) fn position(aircraft: &Aircraft) -> [Fixed; 3] {
    let [x, y, altitude] = aircraft.position;
    [x, altitude, -y]
}

pub(super) fn pose(aircraft: &Aircraft) -> Pose {
    Pose {
        position: position(aircraft),
        yaw: degrees(aircraft.yaw),
        pitch: degrees(aircraft.pitch),
        roll: -degrees(aircraft.roll + aircraft.spin()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jet;
    use core::cell::RefCell;
    use mirui::prelude::*;
    use mirui_sprite_stack::{Geometry, StackStage, StackStageCommands, StackVisual};

    #[mirui::component]
    struct AddedObject;

    struct PendingEdit {
        old: Option<Entity>,
        new: Option<(Geometry, Pose)>,
    }

    #[derive(Default)]
    struct ObjectEdit(RefCell<Option<PendingEdit>>);

    #[mirui::system]
    fn edit(stage: model!(StackStage), pending: res!(ObjectEdit), commands: commands!()) {
        let Some(PendingEdit { old, new }) = pending.0.borrow_mut().take() else {
            return;
        };
        if let Some(id) = old {
            if new.is_none() {
                commands.despawn(id);
                return;
            }
            commands.remove::<StackVisual>(id);
        }
        if let Some((geometry, pose)) = new {
            let visual = StackVisual::new(geometry).at(pose);
            if let Some(id) = old {
                commands.stage(stage).attach(id, visual);
            } else {
                commands.stage(stage).spawn(AddedObject, visual);
            }
        }
    }

    fn run_edit<B, F>(
        app: &mut App<B, F>,
        old: Option<Entity>,
        new: Option<(Geometry, Pose)>,
    ) -> Option<Entity>
    where
        B: mirui::surface::Surface,
        F: mirui::render::RendererFactory<B>,
    {
        let previous: alloc::vec::Vec<_> = app
            .world
            .query::<AddedObject>()
            .iter()
            .map(|(id, _)| id)
            .collect();
        app.add_resource(ObjectEdit(RefCell::new(Some(PendingEdit { old, new }))));
        app.tick();
        app.world
            .query::<AddedObject>()
            .iter()
            .map(|(id, _)| id)
            .find(|id| !previous.contains(id))
    }
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
        app.add_resource(ObjectEdit::default());
        app.add_system(edit::system(game.stage()));
        game.set_focus(false);
        app.tick();
        let player = game.read(|state| pose(&state.flight.jets[0]));
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
        let object = run_edit(&mut app, None, Some((field.clone().into(), pose))).unwrap();
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
        let rear_field = HeightField::generate(grid, &COLORS, |_| TerrainCell {
            level: 0,
            material: 0,
        })
        .unwrap();
        let rear = run_edit(&mut app, None, Some((rear_field.into(), back))).unwrap();
        let overlapped = app.snapshot().unwrap().unwrap();
        assert_eq!(
            sample(&overlapped, a),
            COLORS[1],
            "near field covers a later placement"
        );
        pose.position[0] -= Fixed::from_int(96);
        run_edit(&mut app, Some(object), Some((field.clone().into(), pose)));
        let b = point(pose);
        let moved = app.snapshot().unwrap().unwrap();
        assert_eq!(
            sample(&moved, a),
            COLORS[0],
            "moving the near field exposes the far field"
        );
        run_edit(&mut app, Some(rear), None);
        assert_eq!(sample(&moved, b), COLORS[1]);
        run_edit(&mut app, Some(object), None);
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
        let repeating = mirui::core::cache::Handle::new(repeating);
        let sheet_pose = Pose::at(pose.position[0], player.position[1], pose.position[2]);
        let sheet = run_edit(&mut app, None, Some((repeating.clone().into(), sheet_pose))).unwrap();
        let visible = app.snapshot().unwrap().unwrap();
        assert_eq!(sample(&visible, a), COLORS[0]);
        let buried_pose = Pose::at(pose.position[0], -250, pose.position[2]);
        run_edit(&mut app, Some(sheet), Some((repeating.into(), buried_pose)));
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
        use mirui::core::model::ModelHandle;
        app.add_resource(ObjectEdit::default());
        app.add_system(edit::system(game.stage()));
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
            game.set_focus(false);
            app.tick();
            let mut pose = game.read(|state| pose(&state.flight.jets[0]));
            pose.position[0] += Fixed::from_int(30);
            pose.position[1] += Fixed::from_int(8);
            for geometry in [
                Geometry::Sprite(atlas.clone()),
                Geometry::Prepared(&jet::MODEL),
                Geometry::Sampled(&jet::MODEL),
            ] {
                let before = app.snapshot().unwrap().unwrap();
                let id = run_edit(&mut app, None, Some((geometry, pose))).unwrap();
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
                run_edit(&mut app, Some(id), None);
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
            game.set_focus(true);
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
