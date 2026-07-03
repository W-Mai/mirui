use mirui::ecs::{Entity, World};
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Text;
use mirui::ui::{IdMap, Parent, ViewRegistry};

#[test]
fn ui_macro_splices_code_block() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    let mut side_effect = 0u32;
    let label = "middle";

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        View () {
            Text ("before") {}
            ${
                side_effect += 42;
                let _ = label.len();
            }
            Text ("after") {}
        }
    };

    assert_eq!(side_effect, 42, "code block executed exactly once");

    let texts: Vec<Entity> = world.query::<Text>().collect();
    let contents: Vec<String> = texts
        .iter()
        .map(|&t| world.get::<Text>(t).unwrap().resolve(&world).to_string())
        .collect();
    assert!(contents.iter().any(|s| s == "before"));
    assert!(contents.iter().any(|s| s == "after"));

    let all_with_parent: Vec<Entity> = world.query::<Parent>().collect();
    let view_entities: Vec<Entity> = all_with_parent
        .into_iter()
        .filter(|&e| world.get::<Parent>(e).unwrap().0 == root)
        .collect();
    assert_eq!(view_entities.len(), 1, "one View under root");
}
