use super::*;
use crate::raster::{FlightScene, Palette};
use mirui_sprite_stack::Camera;

fn assert_hint_fits(world: &mirui::ecs::World) {
    let fits = mirui::gallery::demos::text_fits(world);
    assert!(!fits.is_empty());
    for fit in fits {
        assert!(!fit.overflows(0), "{} exceeds its text bounds", fit.text);
    }
}

#[mirui::event]
fn host_key(_event: event!(Key), counter: res!(mut u32)) -> bool {
    *counter += 1;
    true
}

#[test]
fn flight_shares_the_hosts_font_manager_and_only_consumes_applied_keys() {
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let fonts = app.world.resource::<FontManager>().unwrap();
    fonts.add_static("host-font", Font::default());
    let original = fonts.resolve(FontToken::Default.cache_key());
    let host_font = fonts.resolve("host-font");
    let model = setup_app(&mut app, root);
    let fonts = app.world.resource::<FontManager>().unwrap();
    assert!(
        fonts
            .resolve(FontToken::Default.cache_key())
            .ptr_eq(&original)
    );
    assert!(!fonts.resolve(FLIGHT_FONT.cache_key()).ptr_eq(&original));
    assert!(fonts.resolve("host-font").ptr_eq(&host_font));

    app.add_resource(0u32);
    app.on_event(Key::EVENT, host_key::callback());
    let mut send = |physical, pressed, repeat| {
        let key = Key {
            physical: Some(physical),
            code: 0,
            pressed,
            modifiers: None,
            repeat: Some(repeat),
        };
        mirui::input::event::dispatch_input(&mut app.world, Some(root), &key.into(), 1, 128, 128);
        *app.world.resource::<u32>().unwrap()
    };
    model.set_autoplay(true);
    assert_eq!(send(PhysicalKey::Space, true, false), 1);
    assert_eq!(model.view(), CameraView::Overhead);
    model.set_autoplay(false);
    assert_eq!(send(PhysicalKey::Space, true, false), 1);
    assert_eq!(model.view(), CameraView::Chase);
    assert_eq!(send(PhysicalKey::Space, true, true), 2);
    assert_eq!(send(PhysicalKey::Space, false, false), 3);
    model.set_visible(false);
    assert_eq!(send(PhysicalKey::Space, true, false), 4);
}

#[test]
fn pointer_uses_the_presented_frame_at_a_fractional_scale_boundary() {
    let dimensions = [Fixed::from_ratio(257, 2), Fixed::from_int(128)];
    let point = [Fixed::from_int(39), Fixed::from_int(64)];
    let (size, pointer) = Profile::Compact.frame_input(dimensions, Some(point));
    assert_eq!(size, [65, 64]);
    assert_eq!(pointer, Some([Fixed::from_int(20), Fixed::from_int(32)]));
}

#[test]
fn additional_scene_viewports_share_one_simulation_step() {
    #[mirui::compose]
    fn minimap(game: model!(FlightModel)) -> Entity {
        let stage = game.stage();
        ui! {
            StackViewport (
                width: 16,
                height: 16,
                stage: stage,
                camera: ${Camera {
                    center: game.player_position(),
                    range: Fixed::from_int(8),
                    ..Camera::overhead()
                }}
            )
        }
    }
    let mut app = App::headless(40, 64);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let game = setup_compact_app(&mut app, root);
    let map = app.compose(root, minimap(game.clone()));
    app.prepare_text_layout().unwrap();
    app.snapshot().unwrap();
    let rev = game.revision();
    app.tick();
    assert_eq!(game.revision(), rev + 1);
    app.snapshot().unwrap();
    assert_eq!(game.revision(), rev + 1);
    assert_eq!(
        app.world.get::<StackViewport>(map).unwrap().camera.center,
        game.player_position()
    );
}

#[test]
fn moving_and_removing_the_ground_keeps_the_scene_and_cameras_live() {
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let game = setup_compact_app(&mut app, root);
    game.key(PhysicalKey::KeyE, true, false);
    for _ in 0..45 {
        game.advance(32, 128, 128, None);
    }
    app.tick();
    let stage = game.stage();
    let frame = |app: &mut App<_>| app.snapshot().unwrap().unwrap();
    let differs = |a: &mirui::render::texture::Texture, b: &mirui::render::texture::Texture| {
        a.read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .zip(b.read_pixels().unwrap().bytes())
            .any(|(a, b)| a != b)
    };
    let original = frame(&mut app);
    game.set_focus(false);
    let mut moved = Pose::at(123, 26, 17);
    moved.yaw = Fixed::from_int(90);
    game.place_ground(Some(moved));
    app.tick();
    let raised = frame(&mut app);
    assert!(differs(&original, &raised));
    let position = game.read(|game| game.flight.jets[0].position);
    let terrain = app.world.query::<stage_members::FlightTerrain>();
    let (entity, _) = terrain.iter().next().unwrap();
    let ground = app
        .world
        .get::<mirui_sprite_stack::StackVisual>(entity)
        .unwrap();
    assert_eq!(ground.pose(), moved);
    let [x, y, _] = position;
    let expected = ground
        .geometry()
        .with_field(|field| {
            let coordinate = |value: Fixed| Fixed::from_int(value.trunc_to_int());
            let dx = coordinate(x) - moved.position[0];
            let dz = -coordinate(y) - moved.position[2];
            let (sin, cos) = (Fixed::sin_deg(moved.yaw), Fixed::cos_deg(moved.yaw));
            let i = field
                .address_at(dx * cos - dz * sin, dx * sin + dz * cos)
                .unwrap();
            Fixed::from_int(i32::from(field.levels()[i])) + moved.position[1]
        })
        .unwrap();
    let sampled = game.read(|game| {
        game.terrain
            .at(game.ground_pose)
            .height(x.trunc_to_int(), y.trunc_to_int())
    });
    assert_eq!(sampled, expected);
    game.place_ground(Some(Pose::default()));
    app.tick();
    assert!(!differs(&original, &frame(&mut app)));
    game.place_ground(Some(moved));
    app.tick();
    game.set_focus(true);
    let rev = game.revision();
    game.advance(32, 128, 128, None);
    assert_eq!(game.revision(), rev + 1);
    assert_ne!(game.read(|game| game.flight.jets[0].position), position);
    let with_ground = frame(&mut app);
    game.place_ground(None);
    app.tick();
    assert_eq!(stage.len(), 1);
    let no_ground = frame(&mut app);
    assert!(differs(&with_ground, &no_ground));
    let p = game.read(|game| game.flight.jets[0].position);
    game.select_view(CameraView::Chase);
    for _ in 0..32 {
        game.advance(32, 128, 128, None);
    }
    assert_ne!(game.read(|game| game.flight.jets[0].position), p);
    assert!(differs(&no_ground, &frame(&mut app)));
}

#[test]
fn lighting_updates_palette_without_allocating() {
    use crate::raster::Daylight;
    let mut scene = FlightScene::new(Palette::shared(Daylight::Day));
    for day in [Daylight::Sunset, Daylight::Night, Daylight::Day] {
        assert_eq!(
            crate::test_support::tracked_allocations(|| scene.set_daylight(day)),
            0,
        );
        assert_eq!(scene.palette.day, day);
    }
}
#[test]
fn autoplay_reuses_flight_and_camera_without_allocating() {
    let mut scene = FlightModel::with_profile(32, 32, Profile::Compact);
    let position = scene.flight.jets[0].position;
    scene.press();
    scene.key(PhysicalKey::ShiftLeft, true, false);
    scene.set_autoplay(true);
    assert!(!scene.flight.controls.holding);
    assert_eq!(scene.flight.controls.keys, 0);
    let allocations = crate::test_support::tracked_allocations(|| {
        for expected in [CameraView::Chase, CameraView::Cockpit, CameraView::Overhead] {
            let before = scene.view();
            for _ in 0..239 {
                scene.advance(50, 32, 32, Some([Fixed::ONE; 2]));
            }
            assert_eq!(scene.view(), before);
            scene.set_autoplay(true);
            scene.key(PhysicalKey::Space, true, false);
            scene.key(PhysicalKey::ShiftLeft, true, false);
            scene.press();
            scene.advance(50, 32, 32, Some([Fixed::ONE; 2]));
            assert_eq!(scene.view(), expected);
            assert_eq!(scene.view_elapsed_ms, 0);
            assert_eq!(scene.flight.controls.keys, 0);
            assert!(!scene.flight.controls.holding);
            assert!(scene.flight.controls.pointer.is_none());
        }
    });
    assert_eq!(allocations, 0);
    assert_ne!(scene.flight.jets[0].position, position);
    scene.set_autoplay(false);
    scene.key(PhysicalKey::Space, true, false);
    assert_eq!(scene.view(), CameraView::Chase);
    scene.key(PhysicalKey::ShiftLeft, true, false);
    assert_ne!(scene.flight.controls.keys, 0);
}
#[test]
fn autoplay_pauses_and_uses_the_same_clamped_simulation_time() {
    let mut scene = FlightModel::with_profile(32, 32, Profile::Compact);
    scene.set_autoplay(true);
    for _ in 0..239 {
        scene.advance(50, 32, 32, None);
    }
    let time = scene.flight.time;
    let revision = scene.revision;
    scene.set_focus(false);
    scene.advance(u32::MAX, 32, 32, None);
    scene.set_focus(true);
    scene.set_visible(false);
    scene.advance(u32::MAX, 32, 32, None);
    scene.set_visible(true);
    scene.advance(u32::MAX, 0, 128, None);
    assert_eq!(scene.flight.time, time);
    assert_eq!(scene.revision, revision);
    assert_eq!(scene.view(), CameraView::Overhead);
    scene.advance(u32::MAX, 32, 32, None);
    assert_eq!(scene.view(), CameraView::Chase);
    assert_eq!(scene.view_elapsed_ms, 0);
    assert_eq!(scene.flight.time, Fixed::from_int(12));
    let position = scene.flight.jets[0].position;
    scene.set_autoplay(false);
    assert_eq!(scene.view(), CameraView::Chase);
    assert_eq!(scene.flight.jets[0].position, position);
    scene.set_autoplay(true);
    scene.advance(0, 32, 32, None);
    assert_eq!(scene.view_elapsed_ms, 0);
}
#[test]
fn compact_keeps_all_views_and_bounds_workspaces_across_resizes() {
    use crate::raster::{Frame, PlacedAircraft, tests::reference::Raster};
    use mirui::render::texture::ColorFormat;

    let mut scene = FlightModel::with_profile(128, 128, Profile::Compact);
    assert_eq!(Profile::Compact.scene_size(128, 128), (128, 128, 1));
    for (width, height) in [
        (128, 128),
        (160, 128),
        (128, 160),
        (320, 240),
        (64, 64),
        (800, 600),
    ] {
        let (w, h, _) = Profile::Compact.scene_size(width, height);
        let mut raster = Raster::new([w, h], true).unwrap();
        let mut frame = Frame::with_format(w, h, ColorFormat::RGB565).unwrap();
        let mut draw = |scene: &FlightModel| {
            raster.render(
                &mut frame,
                &scene.terrain,
                &scene.flight,
                &PlacedAircraft::from_flight(&scene.flight),
            );
        };
        for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
            scene.select_view(view);
            scene.advance(16, width, height, None);
            draw(&scene);
            let allocations = crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    scene.advance(16, width, height, None);
                    draw(&scene);
                }
            });
            assert_eq!(allocations, 0);
        }
        assert!(w <= 128 && h <= 128);
        assert!(scene.resident_bytes() + raster.bytes() + frame.bytes() < 128 * 1024);
    }
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let model = setup_compact_app(&mut app, root);
    for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
        model.select_view(view);
        app.render().unwrap();
        assert!(model.resident_bytes() < 128 * 1024);
        assert_hint_fits(&app.world);
        for (entity, _) in app.world.query::<Text>().iter() {
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.y >= Fixed::ZERO && rect.y + rect.h <= Fixed::from_int(126));
        }
    }
}
#[test]
fn host_focus_and_key_repeats_do_not_leave_stuck_controls() {
    let mut scene = FlightModel::new(640, 360);
    scene.key(PhysicalKey::Space, true, false);
    assert_eq!(scene.view(), CameraView::Chase);
    scene.key(PhysicalKey::Space, true, true);
    assert_eq!(scene.view(), CameraView::Chase);
    scene.key(PhysicalKey::ShiftLeft, true, false);
    scene.press();
    scene.set_focus(false);
    assert_eq!(scene.flight.controls.keys, 0);
    assert!(!scene.flight.controls.holding);
    let before = scene.flight.time;
    scene.advance(16, 640, 360, None);
    assert_eq!(scene.flight.time, before);
    scene.set_focus(true);
    scene.advance(16, 640, 360, None);
    assert!(scene.flight.time > before);
    assert!(scene.resident_bytes() < 16 * 1024 * 1024);
}
#[test]
fn app_delivers_flight_keys_and_resize_preserves_state() {
    let mut app = App::headless(640, 360);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let model = setup_app(&mut app, root);
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    assert_hint_fits(&app.world);
    for (width, height) in [(480, 320), (640, 360), (1280, 720), (720, 960)] {
        let mut preview = App::headless(width, height);
        preview.with_default_widgets().with_default_systems();
        let root = preview.spawn_root().id();
        let state = setup_app(&mut preview, root);
        for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
            state.select_view(view);
            preview.prepare_text_layout().unwrap();
            preview.render().unwrap();
            for (entity, _) in preview.world.query::<Text>().iter() {
                let rect = preview.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(rect.x >= Fixed::ZERO && rect.y >= Fixed::ZERO);
                assert!(rect.x + rect.w <= Fixed::from_int(i32::from(width)));
                assert!(rect.y + rect.h <= Fixed::from_int(i32::from(height) - 18));
            }
            assert_hint_fits(&preview.world);
        }
    }
    let key = Key {
        physical: Some(PhysicalKey::Space),
        code: mirui::input::event::input::KEY_UNKNOWN,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    mirui::input::event::dispatch_input(&mut app.world, Some(root), &key.into(), 1, 640, 360);
    assert_eq!(model.view(), CameraView::Chase);
    let mut scene = FlightModel::new(640, 360);
    scene.select_view(CameraView::Cockpit);
    scene.key(PhysicalKey::KeyT, true, false);
    let day = scene.day;
    scene.advance(16, 960, 600, None);
    assert_eq!(scene.view(), CameraView::Cockpit);
    assert_eq!(scene.day, day);
    assert!(scene.resident_bytes() < 16 * 1024 * 1024);
}

impl FlightModelHandle {
    fn place_ground(&self, pose: Option<Pose>) {
        use mirui::core::model::ModelHandle;

        self.update(|value| {
            value.ground_pose = pose;
            value.revision = value.revision.wrapping_add(1);
        });
    }
}
