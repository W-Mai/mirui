use mirui::ecs::{Entity, World};
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Text;
use mirui::ui::{IdMap, Parent, UiScope, ViewRegistry};
use mirui::{compose, ui};

#[compose]
fn hello_only() {
    ui! {
        Text("scoped hello") {}
    };
}

#[test]
fn ui_scope_alone_spawns_widget_under_cx_parent() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    let mut cx = UiScope::new(&mut world, root);
    hello_only(&mut cx);

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
            Text(title)
        }
    }
}

#[compose]
fn nested_helpers() -> Entity {
    ui! {
        Column () {
            titled_row("alpha")
            Text("middle")
            titled_row("beta")
        }
    }
}

#[test]
fn ui_fn_call_form_injects_cx() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    let mut cx = UiScope::new(&mut world, root);
    call_helper(&mut cx);

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
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    let mut cx = UiScope::new(&mut world, root);
    let column = nested_helpers(&mut cx);

    assert_eq!(world.get::<Parent>(column).unwrap().0, root);
    let children = &world.get::<mirui::ui::Children>(column).unwrap().0;
    assert_eq!(children.len(), 3);
    for &child in children {
        assert_eq!(world.get::<Parent>(child).unwrap().0, column);
    }
    let first = world.get::<mirui::ui::Children>(children[0]).unwrap().0[0];
    let last = world.get::<mirui::ui::Children>(children[2]).unwrap().0[0];
    assert_eq!(world.get::<Text>(first).unwrap().resolve(&world), "alpha");
    assert_eq!(
        world.get::<Text>(children[1]).unwrap().resolve(&world),
        "middle"
    );
    assert_eq!(world.get::<Text>(last).unwrap().resolve(&world), "beta");
}

#[test]
fn ui_bang_still_accepts_explicit_header() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)
        Text("legacy header") {}
    };

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(texts.len(), 1);
    let bytes = world.get::<Text>(texts[0]).unwrap().resolve(&world);
    assert_eq!(&*bytes, "legacy header");
}
