use mirui::app::App;
use mirui::core::i18n::{I18n, Locale, Localized};
use mirui::input::event::contract::Event;
use mirui::input::event::lifecycle::Started;
use mirui::ui::widgets::Text;

#[derive(Default)]
struct ReadResult {
    text: String,
    calls: usize,
}

#[mirui::event]
fn read_localized(
    translations: res!(optional I18n),
    texts: query!(Text),
    result: res!(mut ReadResult),
) {
    for text in texts {
        result.text = text.resolve(translations).into_owned();
        result.calls += 1;
    }
}

#[mirui::compose]
fn remove_translations() {
    res!().remove::<I18n>();
}

#[test]
fn typed_callbacks_resolve_with_optional_translation_resources() {
    for (translations, expected) in [
        (None, "greeting"),
        (Some(I18n::new(Locale::ZhCn)), "greeting"),
        (
            Some(I18n::new(Locale::ZhCn).with_translations(&[
                (Locale::EnUs, "greeting", "Hello"),
                (Locale::ZhCn, "greeting", "你好"),
            ])),
            "你好",
        ),
    ] {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        if let Some(translations) = translations {
            app.with_i18n(translations);
        } else {
            app.compose(root, remove_translations());
        }
        app.add_resource(ReadResult::default());
        app.compose(root, |tree| {
            tree.spawn(Text::build(Localized::new("greeting")));
        });
        app.on_event(Started::EVENT, read_localized::callback());
        app.tick();
        let result = app.world.resource::<ReadResult>().unwrap();
        assert_eq!(result.calls, 1);
        assert_eq!(result.text, expected);
    }
}

#[test]
fn resolving_plain_owned_text_borrows_its_content() {
    let text = Text::new(String::from("plain text"));
    let std::borrow::Cow::Borrowed(content) = text.resolve(None) else {
        panic!("resolution must not copy plain content");
    };
    assert_eq!(content, "plain text");
    assert_eq!(content.as_ptr(), text.resolve(None).as_ptr());
}
