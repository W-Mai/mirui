//! Theme resource is wired into App at construction and can be
//! replaced through the `with_theme` builder.

use mirui::app::App;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Color;
use mirui::widget::Theme;

#[test]
fn app_new_inserts_default_theme() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend);
    let theme = app
        .world
        .resource::<Theme>()
        .expect("App::new must insert Theme");
    assert_eq!(*theme, Theme::dark());
}

#[test]
fn with_theme_replaces_default() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend).with_theme(Theme::light());
    let theme = app.world.resource::<Theme>().unwrap();
    assert_eq!(theme.surface, Color::rgb(248, 248, 250));
}

#[test]
fn with_theme_can_use_custom_palette() {
    let mut t = Theme::dark();
    t.primary = Color::rgb(255, 0, 128);
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let app = App::new(backend).with_theme(t);
    assert_eq!(
        app.world.resource::<Theme>().unwrap().primary,
        Color::rgb(255, 0, 128)
    );
}
