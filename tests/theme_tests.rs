//! Theme resource is wired into App at construction and can be
//! replaced through the `with_theme` builder.

use mirui::app::App;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Color;
use mirui::widget::Theme;
use mirui::widget::theme::ColorToken;

#[test]
fn app_new_inserts_default_theme() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend);
    let theme = app
        .world
        .resource::<Theme>()
        .expect("App::new must insert Theme");
    assert_eq!(
        theme.resolve(ColorToken::Primary),
        Theme::dark().resolve(ColorToken::Primary),
    );
}

#[test]
fn with_theme_replaces_default() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend).with_theme(Theme::light());
    let theme = app.world.resource::<Theme>().unwrap();
    assert_eq!(
        theme.resolve(ColorToken::Surface),
        Color::rgb(248, 248, 250)
    );
}

#[test]
fn with_theme_can_use_custom_palette() {
    let mut t = Theme::dark();
    t.set(ColorToken::Primary, Color::rgb(255, 0, 128));
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend).with_theme(t);
    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Primary),
        Color::rgb(255, 0, 128),
    );
}

#[test]
fn user_defined_token_round_trips_through_app() {
    const BRAND: ColorToken = ColorToken::custom("brand_red");
    let mut t = Theme::dark();
    t.set(BRAND, Color::rgb(220, 60, 70));
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend).with_theme(t);
    assert_eq!(
        app.world.resource::<Theme>().unwrap().resolve(BRAND),
        Color::rgb(220, 60, 70),
    );
}
