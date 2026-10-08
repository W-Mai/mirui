use mirui::compose;
use mirui::ui;
use mirui::ui::widgets::Text;
use mirui::ui::{Children, NicheMap};

#[compose]
fn composed_middle() -> mirui::ecs::Entity {
    ui! {
        View (id: "composed")
    }
}

ui!(compose OrderedCard {
    Text("before", id: "before")
    @@middle
    composed_middle()
    if ${ true } {
        View (id: "if-true")
    } else {
        View (id: "if-false")
    }
    match ${ 0u8 } {
        0 => { View (id: "match-zero") }
        _ => { View (id: "match-other") }
    }
    Text("after", id: "after")
});

#[test]
fn compose_roots_attach_in_declaration_order() {
    let mut app = mirui::app::App::headless(64, 64);
    app.with_widget(ui!(compose OrderedCard));
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
        ui! {
            OrderedCard ()
        }
    });
    let world = &app.world;

    let card = world.get::<Children>(root).unwrap().0[0];
    let middle = world.get::<NicheMap>(card).unwrap().get("middle").unwrap();
    let lookup = |name| world.find_by_id(name).unwrap();
    assert_eq!(
        world.get::<Children>(card).unwrap().0,
        [
            lookup("before"),
            middle,
            lookup("composed"),
            lookup("if-true"),
            lookup("if-false"),
            lookup("match-zero"),
            lookup("match-other"),
            lookup("after"),
        ]
    );
}
