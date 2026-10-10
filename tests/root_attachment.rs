use mirui::app::App;
use mirui::ecs::Entity;
use mirui::types::{Color, Dimension};
use mirui::ui::layout::LayoutStyle;
use mirui::ui::{Children, IgnoreHitTest, RespectSafeArea, Style, Widget, WidgetRoot};
use mirui::{compose, ui, view};

#[derive(Default)]
struct Attachments(Vec<Entity>);

#[mirui::event]
fn record_attachment(entity: entity!(), attachments: res!(mut Attachments)) {
    attachments.0.push(entity);
}

#[view(component = Widget, name = "AttachmentProbe", attach = record_attachment::callback())]
fn attachment_probe() {}

ui!(compose Card {
    @@body {
        View (width: 12, height: 10)
    }
});

#[compose]
fn card_panel() -> Entity {
    ui! {
        Card ()
    }
}

#[test]
fn selecting_roots_does_not_repeat_widget_attachment() {
    let mut app = App::headless(64, 64);
    app.add_resource(Attachments::default());
    app.with_widget(attachment_probe::view())
        .with_widget(ui!(compose Card));
    let root = app.spawn_root().id();
    let card = app.compose(root, card_panel());
    let attached = app.world.resource::<Attachments>().unwrap().0.clone();
    let descendants = app.world.get::<Children>(card).unwrap().0.clone();
    assert_eq!(attached.len(), app.world.query::<Widget>().iter().count());
    for (index, entity) in attached.iter().enumerate() {
        assert!(!attached[..index].contains(entity));
    }

    for selected in [card, root, root, card] {
        app.set_root(selected);
        assert_eq!(app.world.resource::<WidgetRoot>().unwrap().0, selected);
        assert_eq!(app.world.resource::<Attachments>().unwrap().0, attached);
        assert_eq!(app.world.get::<Children>(card).unwrap().0, descendants);
    }
}

#[derive(Default)]
struct RootAppearance(Option<(Color, Dimension, bool, bool)>);

#[mirui::event]
fn record_root_appearance(
    entity: entity!(),
    widgets: query!(Style, optional RespectSafeArea, optional IgnoreHitTest),
    theme: res!(mirui::ui::Theme),
    result: res!(mut RootAppearance),
) {
    let (style, safe_area, ignore_hit) = widgets.get(entity).unwrap();
    let appearance = (
        style.bg_color.unwrap().resolve(theme),
        style.layout.width,
        safe_area.is_some(),
        ignore_hit.is_some(),
    );
    let recorded = &mut result.0;
    assert!(recorded.is_none(), "root is attached once");
    *recorded = Some(appearance);
}

#[view(component = Widget, name = "RootAppearance", attach = record_root_appearance::callback())]
fn root_appearance() {}

#[test]
fn root_attachment_observes_final_builder_overrides() {
    let mut app = App::headless(64, 64);
    app.add_resource(RootAppearance::default());
    app.with_widget(root_appearance::view());
    let color = Color::rgb(18, 42, 76);
    let root = app
        .spawn_root()
        .bg_color(color)
        .layout(LayoutStyle {
            width: Dimension::px(40),
            height: Dimension::px(32),
            ..Default::default()
        })
        .ignore_safe_area()
        .id();
    assert_eq!(
        app.world.resource::<RootAppearance>().unwrap().0,
        Some((color, Dimension::px(40), false, true))
    );
    app.set_root(root);
}
