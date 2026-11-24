use mirui::core::reactive::Signal;
use mirui::prelude::*;
use mirui::ui::property::prop;
use mirui::ui::widgets::{Button, Text};
use mirui::ui::{FontSize, Style};

#[compose]
fn class_button(names: prop!(ClassList), inline: prop!(StylePreset)) -> Entity {
    ui! {
        Button (class: ${ names }, style: ${ inline }, text: "Go", width: 60, id: "go")
    }
}

#[compose]
fn class_page(selected: Signal<bool>, inline: Signal<bool>) {
    ui! {
        Column (font_size: 12) {
            class_button (
                ${ ("base", selected.get().then_some("selected")) },
                ${ if inline.get() { style!(height : 44, min_height : 24) } else { style!() } }
            )
            Text (class: "late", text: "Late", id: "late-member")
        }
    };
}

#[compose]
fn register_late_rule() {
    res!(mut StyleSheet).with(|mut styles| {
        styles.set("late", style!(font_size: 16));
    });
}

#[test]
fn inline_withdrawal_uses_the_final_class_rule_and_final_membership() {
    use std::{cell::Cell, rc::Rc};

    let mut app = App::headless(100, 100);
    let selected = Signal::new(false);
    let inline = Signal::new(true);
    let height = Signal::new(27);
    let reads = Rc::new(Cell::new(0));
    let watched = height.clone();
    let count = reads.clone();
    app.with_default_widgets().add_plugin(
        StyleSheetPlugin::new()
            .class("base", style!(width: 28, height: 34, min_height: 19))
            .bind_class("selected", move || {
                count.set(count.get() + 1);
                style!(height: watched.get(), min_height: 20)
            }),
    );
    let root = app.spawn_root().id();
    app.compose(root, class_page(selected.clone(), inline.clone()));

    app.render().unwrap();
    let button = app.world.find_by_id("go").unwrap();
    let late = app.world.find_by_id("late-member").unwrap();
    let style = app.world.get::<Style>(button).unwrap();
    assert_eq!(style.layout.width, Dimension::px(60));
    assert_eq!(style.layout.height, Dimension::px(44));
    assert_eq!(style.layout.min_height, Dimension::px(24));
    assert_eq!(reads.get(), 1);
    assert_eq!(
        app.world.get::<Style>(late).unwrap().font_size,
        FontSize::Inherit
    );

    selected.set(true);
    inline.set(false);
    height.set(35);
    app.compose(root, register_late_rule());
    app.render().unwrap();
    let style = app.world.get::<Style>(button).unwrap();
    assert_eq!(style.layout.height, Dimension::px(35));
    assert_eq!(style.layout.min_height, Dimension::px(20));
    assert_eq!(reads.get(), 2);
    assert_eq!(
        app.world.get::<Style>(late).unwrap().font_size,
        FontSize::Pixels(16)
    );

    height.set(39);
    let styles = app.world.resource_mut::<StyleSheet>().unwrap();
    styles.set("selected", style!(font_size: 10));
    app.render().unwrap();
    let style = app.world.get::<Style>(button).unwrap();
    assert_eq!(style.layout.height, Dimension::px(34));
    assert_eq!(style.font_size, FontSize::Pixels(10));
    assert_eq!(reads.get(), 2);

    assert!(
        app.world
            .resource_mut::<StyleSheet>()
            .unwrap()
            .remove("selected")
    );
    app.render().unwrap();
    assert_eq!(
        app.world.get::<Style>(button).unwrap().font_size,
        FontSize::Inherit
    );
    assert_eq!(app.world.find_by_id("go"), Some(button));
}

#[test]
fn class_builders_and_class_property_share_the_existing_entity() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets().add_plugin(
        StyleSheetPlugin::new()
            .class("label", style!(font_size: 16))
            .class(
                "button",
                style!(height: 20, normal_color: ColorToken::Primary),
            ),
    );
    let root = app.spawn_root().id();
    let button = app.compose(root, |scope| {
        scope.spawn(
            Button::build()
                .class("button")
                .text_color(ColorToken::OnSurface),
        )
    });
    let label = app.compose(root, |scope| {
        scope.spawn(Text::build("Text").bind_class(|| ("label", Some("button"))))
    });
    app.render().unwrap();
    assert_eq!(
        app.world.get::<Style>(button).unwrap().layout.height,
        Dimension::px(20)
    );
    assert_eq!(
        app.world.get::<Style>(label).unwrap().layout.height,
        Dimension::Auto
    );
    assert_eq!(
        app.world.get::<Style>(label).unwrap().font_size,
        FontSize::Pixels(16)
    );

    app.world
        .widget_mut(button)
        .unwrap()
        .set::<prop::Class>(ClassList::default());
    app.render().unwrap();
    let style = app.world.get::<Style>(button).unwrap();
    assert_eq!(style.layout.height, Dimension::Auto);
    assert_eq!(style.layout.min_height, Dimension::px(28));
    assert_eq!(
        app.world.get::<Button>(button).unwrap().normal_color,
        ColorToken::SurfaceVariant.into()
    );
}

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
