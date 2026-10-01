use std::cell::RefCell;

use mirui::ecs::{Entity, World};
use mirui::types::Fixed;
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::Padding;
use mirui::ui::widgets::Text;
use mirui::ui::{Children, IdMap, NicheMap, Parent, Style, UiScope, ViewRegistry};
use mirui::{compose, ui};

#[derive(Debug, PartialEq, Eq)]
struct OrderedAttachment(u8);

fn ordered_attachment(order: &RefCell<Vec<u8>>, value: u8) -> OrderedAttachment {
    order.borrow_mut().push(value);
    OrderedAttachment(value)
}

#[test]
fn attachment_expressions_keep_source_order_and_same_type_last_wins() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let root = WidgetBuilder::new(&mut world).id();
    let order = RefCell::new(Vec::new());

    {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            View () [
                ordered_attachment(&order, 1),
                ordered_attachment(&order, 2),
            ]
        }
    };

    assert_eq!(&*order.borrow(), &[1, 2]);
    let entities: Vec<_> = world.query::<OrderedAttachment>().collect();
    assert_eq!(entities.len(), 1);
    assert_eq!(
        world.get::<OrderedAttachment>(entities[0]),
        Some(&OrderedAttachment(2))
    );
}

#[compose]
fn lowercase_child(label: &'static str) -> Entity {
    ui! {
        Text (label)
    }
}

#[compose]
fn parent_with_lowercase_children() -> Entity {
    ui! {
        Column () {
            lowercase_child ("first")
            lowercase_child ("second")
        }
    }
}

#[test]
fn lowercase_compose_calls_remain_bare_tree_children() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(ViewRegistry::default());
    let root = WidgetBuilder::new(&mut world).id();

    let mut cx = UiScope::new(&mut world, root);
    let column = parent_with_lowercase_children(&mut cx);

    let children = &world
        .get::<Children>(column)
        .expect("Column has children")
        .0;
    assert_eq!(children.len(), 2);
    assert_eq!(world.get::<Parent>(children[0]).unwrap().0, column);
    assert_eq!(world.get::<Parent>(children[1]).unwrap().0, column);
    assert_eq!(
        world.get::<Text>(children[0]).unwrap().resolve(&world),
        "first"
    );
    assert_eq!(
        world.get::<Text>(children[1]).unwrap().resolve(&world),
        "second"
    );
}

struct NicheInstance(u8);

ui!(compose ParameterizedNicheCard(
    pad: Fixed,
    radius: Fixed,
    fallback: &'static str,
) {
    View (
        border_radius: radius,
        padding: Padding::all(pad)
    ) {
        @@body {
            Text(fallback)
        }
    }
});

fn tagged_entity(world: &World, tag: u8) -> Entity {
    world
        .query::<NicheInstance>()
        .collect()
        .into_iter()
        .find(|&entity| world.get::<NicheInstance>(entity).unwrap().0 == tag)
        .expect("tagged Niche instance")
}

fn single_text_child(world: &World, parent: Entity) -> String {
    let children = &world.get::<Children>(parent).expect("Niche has children").0;
    assert_eq!(children.len(), 1);
    world
        .get::<Text>(children[0])
        .expect("Niche child is Text")
        .resolve(world)
        .to_string()
}

fn assert_wrapper_style(world: &World, niche: Entity, pad: Fixed, radius: Fixed) {
    let wrapper = world.get::<Parent>(niche).expect("Niche has wrapper").0;
    let style = world.get::<Style>(wrapper).expect("wrapper has Style");
    assert_eq!(style.border_radius, radius);
    assert_eq!(style.layout.padding, Padding::all(pad));
}

#[test]
fn parameterized_niche_instances_keep_independent_fallback_and_fill() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut registry = ViewRegistry::default();
    registry.insert(ui!(compose ParameterizedNicheCard));
    world.insert_resource(registry);
    let root = WidgetBuilder::new(&mut world).id();

    {
        let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
        ui! {
            Column () {
                ParameterizedNicheCard (
                    pad: Fixed::from_int(3),
                    radius: Fixed::from_int(5),
                    fallback: "first fallback"
                ) [
                    NicheInstance(1),
                ]
                ParameterizedNicheCard (
                    pad: Fixed::from_int(7),
                    radius: Fixed::from_int(11),
                    fallback: "second fallback"
                ) [
                    NicheInstance(2),
                ] {
                    @body {
                        Text ("second fill")
                    }
                }
            }
        }
    };

    let first = tagged_entity(&world, 1);
    let second = tagged_entity(&world, 2);
    let first_body = world
        .get::<NicheMap>(first)
        .expect("first NicheMap")
        .get("body")
        .expect("first body Niche");
    let second_body = world
        .get::<NicheMap>(second)
        .expect("second NicheMap")
        .get("body")
        .expect("second body Niche");

    assert_ne!(first_body, second_body);
    assert_eq!(single_text_child(&world, first_body), "first fallback");
    assert_eq!(single_text_child(&world, second_body), "second fill");
    assert_wrapper_style(&world, first_body, Fixed::from_int(3), Fixed::from_int(5));
    assert_wrapper_style(&world, second_body, Fixed::from_int(7), Fixed::from_int(11));
}
