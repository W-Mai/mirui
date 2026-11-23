use super::*;
use mirui::render::texture::Texture;

fn hash(frame: Texture<'_>) -> u32 {
    frame
        .read_pixels()
        .unwrap()
        .bytes()
        .iter()
        .fold(2_166_136_261, |sum, value| {
            (sum ^ u32::from(*value)).wrapping_mul(16_777_619)
        })
}

#[test]
fn headless_game_uses_the_same_scene_for_title_play_and_restart() {
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let model = setup_compact_app(&mut app, root);
    assert!(!app.tick());
    assert_eq!(model.hud().session, Session::Title);
    let title = hash(app.snapshot().unwrap().unwrap());
    model.set_autoplay(true);
    assert!(!app.tick());
    let first = hash(app.snapshot().unwrap().unwrap());
    assert_ne!(first, title);
    for _ in 0..20 {
        model.advance(16);
        assert!(!app.tick());
    }
    assert_ne!(hash(app.snapshot().unwrap().unwrap()), first);
    model.menu();
    assert_eq!(model.hud().session, Session::Paused);
    model.retry();
    assert_eq!(model.hud().session, Session::Running);
    assert_eq!(model.hud().cores, 0);
    assert!(!app.tick());
}

#[test]
fn keyboard_and_touch_steer_relative_to_the_viewport() {
    for (key, touch, expected) in [
        (PhysicalKey::ArrowUp, Control::Up, [1, 1]),
        (PhysicalKey::ArrowDown, Control::Down, [-1, -1]),
        (PhysicalKey::ArrowLeft, Control::Left, [-1, 1]),
        (PhysicalKey::ArrowRight, Control::Right, [1, -1]),
    ] {
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let model = setup_compact_app(&mut app, root);
        model.update(|state| {
            state.start(Mode::Manual);
            assert!(state.on_key(key, true, false));
            state.advance(crate::gameplay::STEP_MS);
            let keyboard = state.game.ship.location;
            state.retry();
            state.button(touch, true);
            state.advance(crate::gameplay::STEP_MS);
            assert_eq!(state.game.ship.location, keyboard);
            let start = crate::world::SPAWN.point;
            for axis in 0..2 {
                assert!((keyboard.point[axis] - start[axis]) * expected[axis] > Fixed::ZERO);
            }
        });
    }
}

#[test]
fn hud_draws_lettering_in_both_host_themes() {
    for (width, height) in [(128, 128), (320, 300), (800, 600)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let model = install(&mut app, root, width == 128);
        model.start(Mode::Manual);
        for theme in ["light", "dark"] {
            app.set_theme(theme).unwrap();
            assert!(!app.tick());
            if width == 320 {
                assert!(model.camera_scale() < Fixed::from_int(2));
            }
            let snapshot = app.snapshot().unwrap().unwrap();
            let pixels = snapshot.read_pixels().unwrap();
            let ink = app.theme().resolve(ColorToken::OnSurface);
            let lettering = |x, y| pixels.get_pixel(x, y) == ink;
            let (header, footer) = if width == 128 { (24, 40) } else { (60, 116) };
            let (width, height) = (i32::from(width), i32::from(height));
            assert!((0..header).any(|y| (0..width / 2).any(|x| lettering(x, y))));
            assert!(
                (height - footer..height).any(|y| (width * 4 / 5..width).any(|x| lettering(x, y)))
            );
            let ground = model.tone().palette().terrain[3];
            assert!(
                (header..height - footer)
                    .any(|y| (0..width).any(|x| pixels.get_pixel(x, y) == ground))
            );
        }
    }
}

#[test]
fn session_panels_leave_the_viewport_and_zoom_in_place() {
    for (width, height) in [(128, 128), (320, 300), (800, 600)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let model = install(&mut app, root, width == 128);
        let mut initial = None;
        for session in [
            Session::Title,
            Session::Running,
            Session::Paused,
            Session::Departed,
            Session::Running,
            Session::Results,
            Session::Title,
        ] {
            model.update(|state| {
                state.game.session = session;
                state.refresh();
            });
            assert!(!app.tick());
            let viewport = app.world.query::<mirui_sprite_stack::StackViewport>();
            let (entity, viewport) = viewport.iter().next().unwrap();
            let rect = app.world.get::<mirui::ui::ComputedRect>(entity).unwrap().0;
            let view = (rect, viewport.camera);
            assert_eq!(
                view,
                *initial.get_or_insert(view),
                "{width}x{height}: {session:?}"
            );
        }
    }
}
