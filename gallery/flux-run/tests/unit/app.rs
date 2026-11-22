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
