use super::data::{Page, Phase};
use super::*;
use crate::surface::FramebufferAccess;

#[test]
fn pixel_fonts_cover_the_demo_charset_at_native_sizes() {
    for (size, bytes) in [
        (12, include_bytes!("../assets/nearby-menu.mirx").as_slice()),
        (24, include_bytes!("../assets/nearby-title.mirx").as_slice()),
    ] {
        let font =
            Font::from_mirx("Nearby", size, bytes, &mirx::reader::PayloadLimits::HOST).unwrap();
        for character in include_str!("../assets/nearby-charset.txt")
            .chars()
            .filter(|c| !c.is_control())
        {
            assert!(
                font.map_char(character).is_some(),
                "missing {character} at {size}px"
            );
        }
    }
}

#[test]
fn local_session_transitions_do_not_retain_closed_network_state() {
    let mut model = model::DisplayState::default();
    model.activate(0);
    assert_eq!(model.snapshot().page, Page::Choice);
    model.activate(0);
    for _ in 0..3 {
        model.advance();
    }
    assert_eq!(model.snapshot().phase, Phase::NetworkConnected);
    assert!(!model.snapshot().peers.is_empty());
    model.back_page();
    assert_eq!(model.snapshot().page, Page::Leaving);
    assert!(model.snapshot().peers.is_empty());
    assert!(model.snapshot().local_address.is_empty());
}

#[test]
fn all_pages_render_through_one_registered_model() {
    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let state = setup_app(&mut app, root);
    for theme in [
        crate::gallery::showcase_theme::DARK_ID,
        crate::gallery::showcase_theme::LIGHT_ID,
    ] {
        app.set_theme(theme).unwrap();
    }
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    assert!(app.dirty_regions().unwrap().is_empty());
    for action in [1, 0, 0, 9, 9, 9, 9, 8, 8, 8, 0, 1, 0, 9, 9, 9, 8, 9] {
        match action {
            9 => state.advance(),
            8 => state.back_page(),
            index => state.activate(index),
        }
        app.prepare_text_layout().unwrap();
        app.render_dirty().unwrap();
        let pixels = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(
            pixels,
            app.backend.framebuffer().buf.as_slice(),
            "{:?}",
            state.snapshot().page
        );
    }
    state.restart();
    state.activate(1);
    state.activate(2);
    assert_eq!(state.snapshot().page, Page::Unavailable);
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
}
