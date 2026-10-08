use super::composition::build_widgets;
use crate::core::i18n::{I18n, Locale};
use crate::ecs::World;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;

#[test]
fn build_widgets_smoke() {
    for with_fonts in [false, true] {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        if with_fonts {
            world.insert_resource(crate::render::font::default_font_manager());
        }
        let previous = I18n::new(Locale::ZhCn);
        let previous_locale = previous.locale_signal();
        world.insert_resource(previous);
        let parent = WidgetBuilder::new(&mut world).id();
        {
            let mut cx = UiScope::new(&mut world, parent);
            build_widgets()(&mut cx);
        }
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
        let locale = i18n.locale_signal();
        locale.set(Locale::ZhCn);
        assert_eq!(i18n.translate("welcome"), Some("欢迎使用 mirui"));
        assert_eq!(previous_locale.get_untracked(), Locale::ZhCn);
        previous_locale.set(Locale::EnUs);
        assert_eq!(locale.get_untracked(), Locale::ZhCn);
        if let Some(manager) = world.resource::<crate::render::font::FontManager>() {
            assert_eq!(
                manager
                    .resolve(super::catalog::TOKEN_CJK.cache_key())
                    .family,
                "MiSans UI"
            );
        }
    }
}
