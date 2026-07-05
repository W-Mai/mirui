use mirui::ecs::{Entity, World};
use mirui::ui;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::widgets::Text;
use mirui::ui::{IdMap, NicheMap, Parent, ViewRegistry};

ui!(compose MoldCard {
    @@header @@body @@footer
});

ui!(compose NestedCard {
    Column () {
        View (height: 20) {
            @@header
        }
        View (grow: 1.0) {
            @@body
        }
    }
});

#[test]
fn mold_generates_niche_map_via_attach() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::default();
    reg.insert(ui!(compose MoldCard));
    world.insert_resource(reg);

    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        MoldCard () {
            @body { Text("body content") {} }
            @footer { Text("footer text") {} }
        }
    };

    let cards: Vec<Entity> = world.query::<MoldCard>().collect();
    assert_eq!(cards.len(), 1, "mold spawns exactly one MoldCard entity");
    let card = cards[0];

    let map = world
        .get::<NicheMap>(card)
        .expect("mold attach registered NicheMap on the entity");
    let header = map.get("header").expect("header slot registered");
    let body = map.get("body").expect("body slot registered");
    let footer = map.get("footer").expect("footer slot registered");

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(texts.len(), 2, "@body + @footer filled two texts");

    for &t in &texts {
        let parent = world.get::<Parent>(t).expect("text has Parent").0;
        let text_val = world.get::<Text>(t).expect("text has Text");
        let bytes = text_val.resolve(&world);
        match &*bytes {
            "body content" => assert_eq!(parent, body),
            "footer text" => assert_eq!(parent, footer),
            other => panic!("unexpected text content {other:?}"),
        }
    }

    let all_with_parent: Vec<Entity> = world.query::<Parent>().collect();
    let unused_header_children: Vec<Entity> = all_with_parent
        .into_iter()
        .filter(|&e| world.get::<Parent>(e).unwrap().0 == header)
        .collect();
    assert!(
        unused_header_children.is_empty(),
        "header niche should be empty since @header was not used"
    );
}

#[test]
fn mold_body_expands_nested_widget_tree() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::default();
    reg.insert(ui!(compose NestedCard));
    world.insert_resource(reg);

    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        NestedCard () {
            @header { Text("title") {} }
            @body { Text("content") {} }
        }
    };

    let cards: Vec<Entity> = world.query::<NestedCard>().collect();
    assert_eq!(cards.len(), 1);
    let card = cards[0];

    let map = world
        .get::<NicheMap>(card)
        .expect("attach registered NicheMap");
    let header_slot = map.get("header").expect("header slot registered");
    let body_slot = map.get("body").expect("body slot registered");

    let header_parent = world
        .get::<Parent>(header_slot)
        .expect("header has parent")
        .0;
    let body_parent = world.get::<Parent>(body_slot).expect("body has parent").0;
    assert_ne!(
        header_parent, card,
        "header slot lives under the wrapper View, not the card root"
    );
    assert_ne!(
        body_parent, card,
        "body slot lives under the wrapper View, not the card root"
    );
    assert_ne!(
        header_parent, body_parent,
        "header and body sit inside two different wrapper Views"
    );

    let column_parent = world
        .get::<Parent>(header_parent)
        .expect("wrapper View has parent")
        .0;
    let column_parent_of_body = world
        .get::<Parent>(body_parent)
        .expect("wrapper View has parent")
        .0;
    assert_eq!(
        column_parent, column_parent_of_body,
        "both wrapper Views share the Column"
    );
    assert_eq!(
        world
            .get::<Parent>(column_parent)
            .expect("column has parent")
            .0,
        card,
        "Column is the direct child of the NestedCard root"
    );

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(texts.len(), 2);
    for &t in &texts {
        let parent = world.get::<Parent>(t).unwrap().0;
        let text_val = world.get::<Text>(t).unwrap();
        let bytes = text_val.resolve(&world);
        match &*bytes {
            "title" => assert_eq!(parent, header_slot),
            "content" => assert_eq!(parent, body_slot),
            other => panic!("unexpected text content {other:?}"),
        }
    }
}

ui!(compose FallbackCard {
    @@header {
        Text ("Default header") {}
    }
    @@body {
        Text ("Default body") {}
    }
});

#[test]
fn mold_unfilled_slot_keeps_fallback() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::default();
    reg.insert(ui!(compose FallbackCard));
    world.insert_resource(reg);

    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        FallbackCard () {
        }
    };

    let cards: Vec<Entity> = world.query::<FallbackCard>().collect();
    assert_eq!(cards.len(), 1);
    let card = cards[0];
    let map = world.get::<NicheMap>(card).unwrap();
    let header = map.get("header").unwrap();
    let body = map.get("body").unwrap();

    let texts: Vec<Entity> = world.query::<Text>().collect();
    assert_eq!(texts.len(), 2, "both slots keep their fallback text");
    for &t in &texts {
        let parent = world.get::<Parent>(t).unwrap().0;
        let bytes = world.get::<Text>(t).unwrap().resolve(&world);
        match &*bytes {
            "Default header" => assert_eq!(parent, header),
            "Default body" => assert_eq!(parent, body),
            other => panic!("unexpected fallback text {other:?}"),
        }
    }
}

#[test]
fn mold_filled_slot_despawns_fallback() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::default();
    reg.insert(ui!(compose FallbackCard));
    world.insert_resource(reg);

    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        FallbackCard () {
            @body { Text ("Custom body") {} }
        }
    };

    let cards: Vec<Entity> = world.query::<FallbackCard>().collect();
    assert_eq!(cards.len(), 1);
    let card = cards[0];
    let map = world.get::<NicheMap>(card).unwrap();
    let header = map.get("header").unwrap();
    let body = map.get("body").unwrap();

    let texts: Vec<Entity> = world.query::<Text>().collect();
    let contents: Vec<String> = texts
        .iter()
        .map(|&t| (t, world.get::<Text>(t).unwrap().resolve(&world).to_string()))
        .map(|(_, s)| s)
        .collect();

    assert!(
        contents.iter().any(|s| s == "Default header"),
        "header fallback kept (slot not filled). got {contents:?}"
    );
    assert!(
        contents.iter().any(|s| s == "Custom body"),
        "body replaced with caller's text. got {contents:?}"
    );
    assert!(
        !contents.iter().any(|s| s == "Default body"),
        "old fallback body must be despawned. got {contents:?}"
    );

    for &t in &texts {
        let parent = world.get::<Parent>(t).unwrap().0;
        let bytes = world.get::<Text>(t).unwrap().resolve(&world);
        match &*bytes {
            "Default header" => assert_eq!(parent, header),
            "Custom body" => assert_eq!(parent, body),
            other => panic!("unexpected text {other:?}"),
        }
    }
}

use mirui::types::Fixed;
use mirui::ui::Style;

ui!(compose SizedCard(pad: Fixed, radius: Fixed) {
    View (
        border_radius: radius,
        padding: mirui::ui::layout::Padding::all(pad)
    ) {
        @@body
    }
});

#[test]
fn mold_param_values_flow_into_body_attrs() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::default();
    reg.insert(ui!(compose SizedCard));
    world.insert_resource(reg);

    let root = WidgetBuilder::new(&mut world).id();

    ui! {
        :(
            parent: root
            world: &mut world
        :)

        SizedCard (
            pad: Fixed::from_int(8),
            radius: Fixed::from_int(12)
        ) {
            @body { Text ("bordered") {} }
        }
    };

    let cards: Vec<Entity> = world.query::<SizedCard>().collect();
    assert_eq!(cards.len(), 1);
    let card = cards[0];

    let map = world.get::<NicheMap>(card).unwrap();
    let body = map.get("body").expect("body slot registered");

    let wrapper = world.get::<Parent>(body).expect("body has parent").0;
    let style = world
        .get::<Style>(wrapper)
        .expect("wrapper View has a Style");
    assert_eq!(
        style.border_radius,
        Fixed::from_int(12),
        "wrapper border_radius should be the radius param, got {:?}",
        style.border_radius
    );
}
