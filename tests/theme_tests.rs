//! Theme resource is wired into App at construction and can be
//! replaced through the `with_theme` builder.

use mirui::app::App;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Color;
use mirui::ui::Theme;
use mirui::ui::theme::{ColorToken, ThemeError, ThemeId, ThemeInfo};

#[mirui::event]
fn select_light(theme: res!(Theme), commands: commands!()) {
    assert_eq!(theme.id(), ThemeId::new("dark"));
    commands.set_theme("light");
    assert_eq!(theme.id(), ThemeId::new("dark"));
}

#[test]
fn typed_theme_change_commits_after_resource_borrows_end() {
    use mirui::input::event::contract::Event;
    use mirui::input::event::lifecycle::Started;

    let mut app = App::headless(16, 16);
    let other = App::headless(16, 16);
    app.on_event(Started::EVENT, select_light::callback());
    app.tick();
    assert_eq!(app.theme().id(), ThemeId::new("light"));
    assert_eq!(other.theme().id(), ThemeId::new("dark"));
}

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
fn app_exposes_identity_and_scoped_theme_edits() {
    const BRAND: ColorToken = ColorToken::custom("brand");
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.with_theme(Theme::dark().with_info(ThemeInfo::new(
        "studio",
        "Studio",
        "High-contrast studio palette",
    )));

    assert_eq!(app.theme().id(), ThemeId::new("studio"));
    assert_eq!(app.theme().name(), "Studio");
    app.edit_theme(|theme| {
        theme.set(BRAND, Color::rgb(12, 34, 56));
    });
    assert_eq!(app.theme().resolve(BRAND), Color::rgb(12, 34, 56));
}

#[test]
fn registered_themes_switch_by_id_and_keep_catalog_edits() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    let ocean = Theme::dark().with_info(ThemeInfo::new("ocean", "Ocean", "Low-glare cyan palette"));

    app.register_theme(ocean);
    app.set_theme("ocean").unwrap();
    assert_eq!(app.theme().name(), "Ocean");
    assert_eq!(app.themes().len(), 3);

    app.edit_theme_id("ocean", |theme| {
        theme.set(ColorToken::Primary, Color::rgb(12, 34, 56));
    })
    .unwrap();
    app.set_theme("light").unwrap();
    app.set_theme(ThemeId::new("ocean")).unwrap();
    assert_eq!(
        app.theme().resolve(ColorToken::Primary),
        Color::rgb(12, 34, 56)
    );

    assert_eq!(
        app.set_theme("missing"),
        Err(ThemeError::NotFound(ThemeId::new("missing")))
    );
    assert_eq!(app.theme().id(), ThemeId::new("ocean"));
}

#[test]
fn theme_types_are_available_from_the_prelude() {
    use mirui::prelude::*;

    let theme = Theme::dark().with_info(ThemeInfo::new("prelude", "Prelude", "Visible"));
    let mut catalog = ThemeCatalog::new();
    catalog.insert(theme);
    assert_eq!(
        catalog.get(ThemeId::new("prelude")).unwrap().name(),
        "Prelude"
    );
}

#[test]
fn set_theme_swaps_resource_and_marks_tree_dirty() {
    use mirui::ui::Style;
    use mirui::ui::dirty::Dirty;

    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();

    let root = app.spawn_root().id();
    let (child_a, child_b) = app.compose(root, |tree| {
        (tree.spawn(Style::default()), tree.spawn(Style::default()))
    });

    // Sanity: we cleared the freshly-spawned tree's Dirty so the swap signal is unambiguous.
    app.world.remove::<Dirty>(root);
    app.world.remove::<Dirty>(child_a);
    app.world.remove::<Dirty>(child_b);
    assert!(app.world.get::<Dirty>(root).is_none());

    app.set_theme(Theme::light()).unwrap();

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
    use mirui::ui::{Style, WidgetRoot};

    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let selected = app.compose(root, |tree| tree.spawn(Style::default()));
    app.set_root(selected);

    let cached = app.world.resource::<WidgetRoot>().expect(
        "WidgetRoot must be published so handlers can reach the live root without an App ref",
    );
    assert_eq!(cached.0, selected);
}

#[test]
fn set_theme_without_root_only_swaps_resource() {
    let backend = FramebufSurface::new(64, 64, |_, _| {});
    let mut app = App::new(backend);
    app.set_theme(Theme::light()).unwrap();
    assert_eq!(
        app.world
            .resource::<Theme>()
            .unwrap()
            .resolve(ColorToken::Surface),
        Theme::light().resolve(ColorToken::Surface),
    );
}
