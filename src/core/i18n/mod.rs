//! i18n — locale + key-value translation table, addressable via `Localized` and `t!`.

extern crate alloc;

use alloc::vec::Vec;

use crate::core::reactive::Signal;
use crate::ecs::{Entity, World};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    EnUs,
    ZhCn,
    Custom(&'static str),
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Locale::EnUs => "en-US",
            Locale::ZhCn => "zh-CN",
            Locale::Custom(s) => s,
        }
    }
}

pub type Translation = (Locale, &'static str, &'static str);

pub struct I18n {
    locale: Signal<Locale>,
    table: &'static [Translation],
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        Self {
            locale: Signal::new(locale),
            table: &[],
        }
    }

    pub fn with_translations(mut self, table: &'static [Translation]) -> Self {
        self.table = table;
        self
    }

    pub fn locale(&self) -> Locale {
        self.locale.get_untracked()
    }

    pub fn locale_signal(&self) -> Signal<Locale> {
        self.locale.clone()
    }

    pub fn set_locale(&self, locale: Locale) {
        self.locale.set(locale);
    }

    pub fn translate(&self, key: &str) -> Option<&'static str> {
        // `get()` (tracked) so reactive bindings re-fire on `set_locale`.
        let current = self.locale.get();
        self.lookup(current, key)
            .or_else(|| self.lookup(Locale::EnUs, key))
    }

    fn lookup(&self, locale: Locale, key: &str) -> Option<&'static str> {
        for (loc, k, v) in self.table {
            if *loc == locale && *k == key {
                return Some(v);
            }
        }
        None
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::EnUs)
    }
}

#[derive(Default)]
struct LocaleWatch {
    last_locale: Option<Locale>,
}

// First tick (`last_locale = None`) marks every Localized Text Dirty so
// the layout pass picks up the resolved string before the first render.
#[mirui_macros::system]
pub fn i18n_dirty_system(world: &mut World) {
    let current = match world.resource::<I18n>() {
        Some(i) => i.locale(),
        None => return,
    };
    let force_refresh = {
        let watch = world.resource::<LocaleWatch>();
        !matches!(watch, Some(w) if w.last_locale == Some(current))
    };
    if !force_refresh {
        return;
    }

    let mut entities: Vec<Entity> = Vec::new();
    world
        .query::<crate::ui::widgets::text::Text>()
        .collect_into(&mut entities);

    for entity in entities {
        let is_localized = world
            .get::<crate::ui::widgets::text::Text>(entity)
            .map(|t| t.is_localized())
            .unwrap_or(false);
        if is_localized {
            world.insert(entity, crate::ui::dirty::Dirty);
        }
    }

    world.insert_resource(LocaleWatch {
        last_locale: Some(current),
    });
}

/// Translation-key reference returned by `t!`. Resolves to a `&'static str`
/// at usage time, so call sites read `loc.resolve(world)` or rely on
/// `impl ToString` for reactive bindings.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Localized {
    pub key: &'static str,
}

impl Localized {
    pub const fn new(key: &'static str) -> Self {
        Self { key }
    }

    /// `None` when no I18n resource is installed (`App::with_i18n` not called).
    pub fn resolve(self, world: &World) -> Option<&'static str> {
        world.resource::<I18n>()?.translate(self.key)
    }

    /// World-free fallback for `impl ToString` — call this in reactive
    /// closures where `with_world` is available; static call sites use
    /// `resolve(world)` directly.
    pub fn resolve_or_key(self, world: &World) -> &'static str {
        self.resolve(world).unwrap_or(self.key)
    }
}

impl core::fmt::Display for Localized {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let key = self.key;
        let resolved =
            crate::core::reactive::with_world(|world| self.resolve(world).unwrap_or(key))
                .unwrap_or(key);
        f.write_str(resolved)
    }
}

/// Build a `Localized` from a key literal. Returned wrapper resolves
/// lazily through the World's `I18n` resource.
///
/// `t!("welcome")` is a once-only seed. Reactive locale switching uses
/// `text: ${ Localized::new("welcome") }` so the signal subscription
/// fires on `set_locale`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::core::i18n::Localized::new($key)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::with_world_scope;
    use crate::ecs::World;

    const TABLE: &[Translation] = &[
        (Locale::EnUs, "welcome", "Welcome"),
        (Locale::EnUs, "bye", "Goodbye"),
        (Locale::ZhCn, "welcome", "欢迎"),
    ];

    #[test]
    fn translate_returns_locale_match() {
        let i18n = I18n::new(Locale::EnUs).with_translations(TABLE);
        assert_eq!(i18n.translate("welcome"), Some("Welcome"));
    }

    #[test]
    fn translate_falls_back_to_en_us() {
        let i18n = I18n::new(Locale::ZhCn).with_translations(TABLE);
        // bye missing in zh-CN — fallback to en-US
        assert_eq!(i18n.translate("bye"), Some("Goodbye"));
    }

    #[test]
    fn translate_unknown_key_returns_none() {
        let i18n = I18n::new(Locale::EnUs).with_translations(TABLE);
        assert_eq!(i18n.translate("missing"), None);
    }

    #[test]
    fn set_locale_changes_lookup() {
        let i18n = I18n::new(Locale::EnUs).with_translations(TABLE);
        assert_eq!(i18n.translate("welcome"), Some("Welcome"));
        i18n.set_locale(Locale::ZhCn);
        assert_eq!(i18n.translate("welcome"), Some("欢迎"));
    }

    #[test]
    fn localized_resolve_uses_world_i18n() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::ZhCn).with_translations(TABLE));
        let loc = Localized::new("welcome");
        assert_eq!(loc.resolve(&world), Some("欢迎"));
    }

    #[test]
    fn localized_resolve_without_i18n_returns_none() {
        let world = World::new();
        let loc = Localized::new("welcome");
        assert_eq!(loc.resolve(&world), None);
    }

    #[test]
    fn localized_resolve_or_key_falls_back() {
        let world = World::new();
        let loc = Localized::new("greeting");
        assert_eq!(loc.resolve_or_key(&world), "greeting");
    }

    #[test]
    fn to_string_resolves_in_world_scope() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::ZhCn).with_translations(TABLE));
        let s = with_world_scope(&mut world, || {
            alloc::string::ToString::to_string(&Localized::new("welcome"))
        });
        assert_eq!(s, "欢迎");
    }

    #[test]
    fn to_string_outside_world_scope_returns_key() {
        let s = alloc::string::ToString::to_string(&Localized::new("welcome"));
        assert_eq!(s, "welcome");
    }

    #[test]
    fn t_macro_returns_localized() {
        let loc = crate::t!("welcome");
        assert_eq!(loc.key, "welcome");
    }

    #[test]
    fn i18n_dirty_system_marks_localized_text_on_first_tick() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::EnUs).with_translations(TABLE));
        let e = world.spawn_empty();
        world.insert(e, crate::ui::widgets::text::Text::from(t!("welcome")));

        let sys = i18n_dirty_system::system();
        (sys.run)(&mut world);

        assert!(world.has::<crate::ui::dirty::Dirty>(e));
    }

    #[test]
    fn localized_text_resolves_to_active_locale() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::EnUs).with_translations(TABLE));
        let text = crate::ui::widgets::text::Text::from(t!("welcome"));
        assert_eq!(&*text.bytes(&world), b"Welcome");

        world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
        assert_eq!(&*text.bytes(&world), "欢迎".as_bytes());
    }

    #[test]
    fn i18n_dirty_system_marks_on_locale_change() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::EnUs).with_translations(TABLE));
        let e = world.spawn_empty();
        world.insert(e, crate::ui::widgets::text::Text::from(t!("welcome")));

        let sys = i18n_dirty_system::system();
        (sys.run)(&mut world);
        world.remove::<crate::ui::dirty::Dirty>(e);

        world.resource::<I18n>().unwrap().set_locale(Locale::ZhCn);
        (sys.run)(&mut world);

        assert!(world.has::<crate::ui::dirty::Dirty>(e));
    }

    #[test]
    fn i18n_dirty_system_idempotent_when_locale_unchanged() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::EnUs).with_translations(TABLE));
        let e = world.spawn_empty();
        world.insert(e, crate::ui::widgets::text::Text::from(t!("welcome")));

        let sys = i18n_dirty_system::system();
        (sys.run)(&mut world);
        world.remove::<crate::ui::dirty::Dirty>(e);
        (sys.run)(&mut world);

        assert!(
            !world.has::<crate::ui::dirty::Dirty>(e),
            "no locale change → no Dirty re-insert"
        );
    }

    #[test]
    fn i18n_dirty_system_skips_owned_text() {
        let mut world = World::new();
        world.insert_resource(I18n::new(Locale::EnUs).with_translations(TABLE));
        let e = world.spawn_empty();
        world.insert(e, crate::ui::widgets::text::Text::from("static"));
        let sys = i18n_dirty_system::system();
        (sys.run)(&mut world);
        assert!(
            !world.has::<crate::ui::dirty::Dirty>(e),
            "static Text shouldn't be touched"
        );
    }

    #[test]
    fn i18n_dirty_system_no_i18n_no_op() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, crate::ui::widgets::text::Text::from(t!("welcome")));
        let sys = i18n_dirty_system::system();
        (sys.run)(&mut world);
        assert!(!world.has::<crate::ui::dirty::Dirty>(e));
    }
}
