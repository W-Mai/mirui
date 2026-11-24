use mirui::core::reactive::Signal;
use mirui::prelude::*;
use mirui::ui::property::prop;
use mirui::ui::widgets::{Button, Text};
use mirui::ui::{FontSize, Style};

#[compose]
fn step(preset: prop!(StylePreset)) -> Entity {
    ui! {
        Row () {
            Button (style: ${ preset }, text: "+", id: "step")
            Button (style: ${ preset }, height: 48, text: "-", id: "fixed-step")
        }
    }
}

#[compose]
fn controls(compact: Signal<bool>) {
    ui! {
        Column (font_size: 14) {
            step (${ if compact.get() { style!(height : 18, font_size : 9) } else { style!() } })
        }
    };
}

#[test]
fn reactive_styles_forward_and_withdraw_on_the_same_entity() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let compact = Signal::new(true);
    app.compose(root, controls(compact.clone()));
    app.render().unwrap();
    let step = app.world.find_by_id("step").unwrap();
    let fixed = app.world.find_by_id("fixed-step").unwrap();
    let style = app.world.get::<Style>(step).unwrap();
    assert_eq!(style.layout.height, Dimension::px(18));
    assert_eq!(style.layout.min_height, Dimension::Auto);
    assert_eq!(style.font_size, FontSize::Pixels(9));

    compact.set(false);
    app.render().unwrap();
    assert_eq!(app.world.find_by_id("step"), Some(step));
    let style = app.world.get::<Style>(step).unwrap();
    assert_eq!(style.layout.height, Dimension::Auto);
    assert_eq!(style.layout.min_height, Dimension::px(28));
    assert_eq!(style.font_size, FontSize::Inherit);
    assert_eq!(
        app.world.get::<Style>(fixed).unwrap().layout.height,
        Dimension::px(48)
    );
}

#[test]
fn reactive_builder_respects_direct_bindings_and_explicit_defaults() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let selected = Signal::new(true);
    let state = selected.clone();
    let button = app.compose(root, |scope| {
        scope.spawn(
            Button::build()
                .bind_property::<prop::Height>(|| Dimension::px(42))
                .bind_style(move || {
                    if state.get() {
                        style!(height: 18, font_size: 9, normal_color: ColorToken::Primary)
                    } else {
                        StylePreset::<Button>::default()
                    }
                })
                .normal_color(ColorToken::Secondary),
        )
    });
    let state = selected.clone();
    let label = app.compose(root, |scope| {
        scope.spawn(
            Text::build("Label")
                .bind_style(move || {
                    if state.get() {
                        style!(font_size: 12)
                    } else {
                        style!()
                    }
                })
                .font_size(FontSize::Native),
        )
    });
    for enabled in [true, false] {
        selected.set(enabled);
        app.render().unwrap();
        let style = app.world.get::<Style>(button).unwrap();
        assert_eq!(style.layout.height, Dimension::px(42));
        assert_eq!(
            style.font_size,
            if enabled {
                FontSize::Pixels(9)
            } else {
                FontSize::Inherit
            }
        );
        assert_eq!(
            app.world.get::<Button>(button).unwrap().normal_color,
            ColorToken::Secondary.into()
        );
        assert_eq!(
            app.world.get::<Style>(label).unwrap().font_size,
            FontSize::Native
        );
    }
}
