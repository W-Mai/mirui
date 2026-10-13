use super::composition::build_widgets;
use crate::core::i18n::{I18n, Locale};
use crate::prelude::*;
use crate::render::font::FontManager;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    for locale in [Locale::EnUs, Locale::ZhCn] {
        let mut app = App::headless(64, 64);
        let previous = I18n::new(locale);
        let previous_locale = previous.locale_signal();
        app.add_resource(previous);
        let parent = app.spawn_root().id();
        app.compose(parent, build_widgets());
        let world = &app.world;
        let column = world
            .get::<Children>(parent)
            .and_then(|c| c.0.first().copied())
            .expect("Column spawned under parent");
        let children = &world
            .get::<Children>(column)
            .expect("Column has children")
            .0;
        assert_eq!(children.len(), 4, "3 labels + 1 toggle button");
        let i18n = world.resource::<I18n>().expect("I18n resource inserted");
        assert_eq!(i18n.locale(), Locale::EnUs);
        assert_eq!(i18n.translate("welcome"), Some("Welcome to mirui"));
        let next_locale = i18n.locale_signal();
        next_locale.set(Locale::ZhCn);
        assert_eq!(i18n.translate("welcome"), Some("欢迎使用 mirui"));
        assert_eq!(previous_locale.get_untracked(), locale);
        previous_locale.set(Locale::EnUs);
        assert_eq!(next_locale.get_untracked(), Locale::ZhCn);
        let manager = world.resource::<FontManager>().unwrap();
        assert_eq!(
            manager
                .resolve(super::catalog::TOKEN_CJK.cache_key())
                .family,
            "MiSans UI"
        );
    }
}
