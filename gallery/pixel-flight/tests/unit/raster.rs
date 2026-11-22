#[path = "raster/compact.rs"]
mod compact;
#[path = "raster/reference.rs"]
pub(crate) mod reference;

use super::super::flight::{Settings, View};
use super::*;
use crate::world::World;
use mirui::types::Color;
use mirui_sprite_stack::raster::Workspace;
use reference::Raster;
#[test]
fn deferred_effect_queries_see_the_same_aircraft_depth_as_the_visible_tiles() {
    let world = World::with_size(7, Fixed::HALF, 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.intro = Fixed::from_int(8);
    let palette = Palette::new(Daylight::Day);
    let placed = PlacedAircraft::from_flight(&flight);
    let ground = world.at(Some(Default::default()));
    let scene = Scene::new(
        &placed,
        ground,
        &flight,
        &palette,
        128,
        flight.projection(128, 128),
    );
    let models = scene.prepare([128; 2], [&crate::jet::MODEL; 3]);
    let order = scene.order();
    let mut frame = Frame::new(128, 128).unwrap();
    let mut work = Workspace::new(128, 128).unwrap();
    let mut depths = |only_depth: bool, aircraft: bool| {
        work.with_canvas(&mut frame, |tile| {
            if !aircraft {
                tile.clear_depth();
                scene.draw_ground::<true>(tile, Fixed::MAX);
            } else if only_depth {
                scene
                    .draw_ordered::<true>(tile, &order, &models, Fixed::MAX)
                    .unwrap();
            } else {
                scene
                    .draw_ordered::<false>(tile, &order, &models, Fixed::MAX)
                    .unwrap();
            }
            tile.depth.to_vec()
        })
    };
    let ground = depths(true, false);
    let visible = depths(false, true);
    assert!(visible.iter().zip(ground).any(|(v, g)| *v < g));
    assert_eq!(depths(true, true), visible);
}

#[test]
fn placement_selects_the_visible_object_position_and_its_sparse_level() {
    use mirui_sprite_stack::format::{PreparedSpriteStack, SparseLevel};
    const EMPTY: SparseLevel<'static> = SparseLevel::new(1, &[0, 0], &[]).unwrap();
    static MODEL: PreparedSpriteStack<'static> =
        PreparedSpriteStack::new([1, 1, 1], [0; 3], 256, &[[0; 4]], &[EMPTY]).unwrap();
    let world = World::with_size(7, Fixed::HALF, 64, 32, 16);
    let flight = Flight::new(&world, Settings::default());
    let projection = flight.projection(128, 128);
    let palette = Palette::new(Daylight::Day);
    let mut placed = PlacedAircraft::from_flight(&flight);
    let mut active = placed[0].unwrap();
    active.position[0] += Fixed::from_int(5);
    placed[0] = Some(active);
    placed[1] = None;
    let scene = Scene::new(
        &placed,
        world.at(Some(Default::default())),
        &flight,
        &palette,
        128,
        flight.projection(128, 128),
    );
    let prepared = scene.prepare([128; 2], [&MODEL; 3]);
    let jet = prepared[0].as_ref().unwrap();
    assert_eq!(
        jet.sprite.center(),
        projection.project(active.position).unwrap()
    );
    assert!(jet.sprite.level().layer(0).unwrap().is_empty());
    assert!(prepared[1].is_none());
}
#[test]
fn region_writes_preserve_global_scene_samples_and_depth() {
    let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.set_wingmen(2);
    flight.controls.key(0, true);
    let raster = Raster::new([128, 128], false).unwrap();
    let mut full = Frame::new(128, 128).unwrap();
    let mut full_workspace = Workspace::new(128, 128).unwrap();
    let mut region = Frame::new(40, 40).unwrap();
    let mut region_workspace = Workspace::new(40, 40).unwrap();
    let bounds = [3, 2, 126, 124];
    for view in [View::Overhead, View::Chase, View::Cockpit] {
        flight.view = view;
        flight.roll();
        for _ in 0..300 {
            flight.step(&world, 16, 128, 128);
        }
        flight.settings.clouds = match view {
            View::Overhead => Fixed::ZERO,
            View::Chase => Fixed::from_ratio(1, 16),
            View::Cockpit => Fixed::ONE,
        };
        let placed = PlacedAircraft::from_flight(&flight);
        let scene = Scene::new(
            &placed,
            world.at(Some(Default::default())),
            &flight,
            raster.scene.palette,
            128,
            flight.projection(128, 128),
        );
        let reference_depth =
            full_workspace.with_region(&mut full, [128, 128], [0, 0], bounds, |canvas| {
                scene.draw(canvas);
                canvas.depth.to_vec()
            });
        for origin in [
            [-42, -42],
            [-2, -2],
            [0, 0],
            [17, 37],
            [44, 20],
            [104, 90],
            [128, 128],
        ] {
            let local_depth =
                region_workspace.with_region(&mut region, [128, 128], origin, bounds, |canvas| {
                    assert_eq!(
                        canvas.sample_depth(80, 80),
                        if canvas.color.contains(80, 80) {
                            Some(canvas.depth[canvas.color.index(80, 80)])
                        } else {
                            None
                        }
                    );
                    scene.draw(canvas);
                    if canvas.color.clip[0] < canvas.color.clip[2]
                        && canvas.color.clip[1] < canvas.color.clip[3]
                    {
                        assert!(canvas.columns.iter().all(|y| *y <= canvas.color.clip[3]));
                    }
                    canvas.depth.to_vec()
                });
            let reference = full.texture.read_pixels().unwrap();
            let local = region.texture.read_pixels().unwrap();
            for y in (origin[1] + 1).max(bounds[1])..(origin[1] + 39).min(bounds[3]) {
                for x in (origin[0] + 1).max(bounds[0])..(origin[0] + 39).min(bounds[2]) {
                    let a = y as usize * 128 + x as usize;
                    let b = ((y - origin[1]) * 40 + x - origin[0]) as usize;
                    assert_eq!(
                        &reference.bytes()[a * 4..a * 4 + 4],
                        &local.bytes()[b * 4..b * 4 + 4],
                        "view {view:?} origin {origin:?} at {x},{y}"
                    );
                    assert_eq!(reference_depth[a], local_depth[b]);
                }
            }
        }
    }
}
#[test]
fn all_camera_frames_are_opaque_and_storage_is_bounded() {
    let world = World::new(7, Fixed::from_ratio(33, 100));
    let mut flight = Flight::new(&world, Settings::default());
    let mut raster = Raster::new([240, 150], false).unwrap();
    let mut frame = Frame::new(240, 150).unwrap();
    for view in [View::Overhead, View::Chase, View::Cockpit] {
        flight.view = view;
        for _ in 0..240 {
            flight.step(&world, 16, 240, 150);
        }
        raster.render(
            &mut frame,
            &world,
            &flight,
            &PlacedAircraft::from_flight(&flight),
        );
        let pixels = frame.texture.read_pixels().unwrap();
        assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
        let first = &pixels.bytes()[..4];
        assert!(pixels.bytes().chunks_exact(4).any(|c| c != first));
    }
    let allocation = crate::test_support::tracked_allocations(|| {
        raster.render(
            &mut frame,
            &world,
            &flight,
            &PlacedAircraft::from_flight(&flight),
        )
    });
    assert_eq!(allocation, 0);
    let max = Raster::new([MAX_WIDTH, MAX_HEIGHT], false).unwrap();
    assert!(max.bytes() + MAX_WIDTH * MAX_HEIGHT * 4 < 8 * 1024 * 1024);
    for size in [(0, 0), (1280, 720), (65535, 65535), (1, 65535), (65535, 1)] {
        let (w, h, _) = scene_size(size.0, size.1);
        assert!(w <= MAX_WIDTH && h <= MAX_HEIGHT);
    }
}
#[test]
fn foreground_depth_blocks_background_writes() {
    let mut frame = Frame::new(8, 8).unwrap();
    let mut workspace = Workspace::new(8, 8).unwrap();
    assert_eq!(frame.bytes(), 8 * 8 * 4);
    assert_eq!(workspace.bytes(), 8 * 8 * 5 + 8 * 4);
    workspace.with_canvas(&mut frame, |canvas| {
        canvas.color.bytes.fill(0xa5);
        canvas.clear_depth();
        canvas.fill_depth(
            3,
            2,
            5,
            rgba(
                Fixed::from_int(10),
                Fixed::from_int(20),
                Fixed::from_int(30),
            ),
            Fixed::ONE,
        );
        canvas.fill_depth(
            3,
            0,
            8,
            rgba(
                Fixed::from_int(200),
                Fixed::from_int(20),
                Fixed::from_int(30),
            ),
            Fixed::from_int(9),
        );
        assert_eq!(
            canvas.get(3 * 8 + 3),
            rgba(
                Fixed::from_ratio(10, 1),
                Fixed::from_ratio(20, 1),
                Fixed::from_ratio(30, 1)
            )
        );
        assert_eq!(canvas.depth[3 * 8 + 3], Fixed::from_ratio(1, 1));
        canvas.color.bounds = [0, 0, 2, 2];
        assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::MAX));
        canvas.color.bounds = [0, 0, 8, 8];
        assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::ONE));
    });
}
