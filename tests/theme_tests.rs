//! Theme resource is wired into App at construction and can be
//! replaced through the `with_theme` builder.

use mirui::app::App;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Color;
use mirui::ui::Theme;
use mirui::ui::theme::ColorToken;

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
    let mut app = App::new(backend);
    app.with_theme(Theme::light());
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
    let mut app = App::new(backend);
    app.with_theme(t);
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
    let mut app = App::new(backend);
    app.with_theme(t);
    assert_eq!(
        app.world.resource::<Theme>().unwrap().resolve(BRAND),
        Color::rgb(220, 60, 70),
    );
}

#[test]
fn set_theme_swaps_resource_and_marks_tree_dirty() {
    use mirui::ui::builder::WidgetBuilder;
    use mirui::ui::dirty::Dirty;

    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();

    let child_a = WidgetBuilder::new(&mut app.world).id();
    let child_b = WidgetBuilder::new(&mut app.world).id();
    let root = WidgetBuilder::new(&mut app.world)
        .child(child_a)
        .child(child_b)
        .id();
    app.set_root(root);

    // Sanity: we cleared the freshly-spawned tree's Dirty so the swap signal is unambiguous.
    app.world.remove::<Dirty>(root);
    app.world.remove::<Dirty>(child_a);
    app.world.remove::<Dirty>(child_b);
    assert!(app.world.get::<Dirty>(root).is_none());

    app.set_theme(Theme::light());

    let theme = app.world.resource::<Theme>().unwrap();
    assert_eq!(
        theme.resolve(ColorToken::Surface),
        Theme::light().resolve(ColorToken::Surface),
    );

    // Live repaint contract: every entity in the rooted tree carries Dirty
    // after a theme swap, so the next render frame redraws unconditionally.
    assert!(app.world.get::<Dirty>(root).is_some());
    assert!(app.world.get::<Dirty>(child_a).is_some());
    assert!(app.world.get::<Dirty>(child_b).is_some());
}

#[test]
fn set_root_publishes_widget_root_resource() {
    use mirui::ui::WidgetRoot;
    use mirui::ui::builder::WidgetBuilder;

    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    let root = WidgetBuilder::new(&mut app.world).id();
    app.set_root(root);

    let cached = app.world.resource::<WidgetRoot>().expect(
        "WidgetRoot must be published so handlers can reach the live root without an App ref",
    );
    assert_eq!(cached.0, root);
}

#[test]
fn set_theme_without_root_only_swaps_resource() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.set_theme(Theme::light());
    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Surface),
        Theme::light().resolve(ColorToken::Surface),
    );
}
