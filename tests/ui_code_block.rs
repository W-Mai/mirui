use mirui::ecs::Entity;
use mirui::ui;
use mirui::ui::Parent;
use mirui::ui::widgets::Text;

#[test]
fn ui_macro_splices_code_block() {
    let mut app = mirui::app::App::headless(64, 64);
    let root = app.spawn_root().id();

    let mut side_effect = 0u32;
    let label = "middle";

    app.compose(root, |cx| {
        ui! {
            View () {
                Text ("before")
                ${
                    side_effect += 42;
                    let _ = label.len();
                }
                Text ("after")
            }
        }
    });
    let world = &app.world;

    assert_eq!(side_effect, 42, "code block executed exactly once");

    let texts: Vec<Entity> = world.query::<Text>().collect();
    let contents: Vec<String> = texts
        .iter()
        .map(|&t| world.get::<Text>(t).unwrap().resolve(world).to_string())
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
