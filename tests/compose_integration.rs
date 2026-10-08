use mirui::app::App;
use mirui::ecs::Entity;
use mirui::ui::Parent;
use mirui::ui::widgets::Text;
use mirui::{compose, ui};

#[compose]
fn hello_only() {
    ui! {
        Text ("scoped hello")
    };
}

#[test]
fn composition_spawns_widgets_under_app_parent() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();

    app.compose(root, hello_only());
    let world = &app.world;

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(world.get::<Parent>(texts[0]).unwrap().0, root);
}

#[compose]
fn call_helper() {
    ui!(hello_only());
    ui!(hello_only());
}

#[compose]
fn titled_row(title: &'static str) -> Entity {
    ui! {
        Row () {
            Text (title)
        }
    }
}

#[compose]
fn nested_helpers() -> Entity {
    ui! {
        Column () {
            titled_row ("alpha")
            Text ("middle")
            titled_row ("beta")
        }
    }
}

#[test]
fn ui_call_executes_compositions_in_the_current_scope() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();

    app.compose(root, call_helper());
    let world = &app.world;

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(
        texts.len(),
        2,
        "each ui!(hello_only()) invocation spawned one Text"
    );
    for &t in &texts {
        assert_eq!(world.get::<Parent>(t).unwrap().0, root);
    }
}

#[test]
fn compose_functions_are_tree_nodes() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();

    let column = app.compose(root, nested_helpers());
    let world = &app.world;

    assert_eq!(world.get::<Parent>(column).unwrap().0, root);
    let children = &world.get::<mirui::ui::Children>(column).unwrap().0;
    assert_eq!(children.len(), 3);
    for &child in children {
        assert_eq!(world.get::<Parent>(child).unwrap().0, column);
    }
    let first = world.get::<mirui::ui::Children>(children[0]).unwrap().0[0];
    let last = world.get::<mirui::ui::Children>(children[2]).unwrap().0[0];
    assert_eq!(world.get::<Text>(first).unwrap().resolve(world), "alpha");
    assert_eq!(
        world.get::<Text>(children[1]).unwrap().resolve(world),
        "middle"
    );
    assert_eq!(world.get::<Text>(last).unwrap().resolve(world), "beta");
}
