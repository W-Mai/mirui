use std::cell::RefCell;

use mirui::ecs::{Entity, World};
use mirui::types::Fixed;
use mirui::ui::layout::Padding;
use mirui::ui::widgets::{Switch, Text};
use mirui::ui::{Children, NicheMap, Parent, Style};
use mirui::{compose, ui};

#[mirui::component]
struct MountSeed(u8);

#[mirui::model]
struct AttachmentModel {
    #[observe]
    value: u16,
}

#[mirui::model]
impl AttachmentModel {
    fn set(&mut self, value: u16) {
        self.value = value;
    }
}

#[mirui::component]
struct ModelAttachment {
    game: model!(AttachmentModel),
}

#[compose]
fn moved_model_attachment(game: model!(AttachmentModel)) {
    ui! {
        View (width: ${ mirui::types::Dimension::px(i32::from(game.value())) }) [
            ModelAttachment { game },
        ]
    };
}

#[test]
fn attachment_model_move_preserves_binding_capture_and_updates() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let game = app.add_model(AttachmentModel { value: 7 });
    app.compose(root, moved_model_attachment(game.clone()));
    let entity = app
        .world
        .query::<ModelAttachment>()
        .iter()
        .next()
        .unwrap()
        .0;
    assert_eq!(
        app.world
            .get::<ModelAttachment>(entity)
            .unwrap()
            .game
            .value(),
        7
    );
    assert_eq!(
        app.world.get::<Style>(entity).unwrap().layout.width,
        mirui::types::Dimension::px(7)
    );
    game.set(11);
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Style>(entity).unwrap().layout.width,
        mirui::types::Dimension::px(11)
    );
}

#[derive(Default)]
struct MountProbe {
    entries: [Option<MountEntry>; 2],
    count: usize,
}

#[derive(Clone, Copy)]
struct MountEntry {
    seed: u8,
    on: Option<bool>,
    entity: Entity,
    parent: Option<Entity>,
    registered: bool,
}

#[compose]
fn static_label_with_consuming_capture(label: String) {
    ui! {
        Text (
            text: label,
            capacity: 32,
            width: ${ mirui::types::Dimension::px(label.len() as i32) }
        )
    };
}

#[test]
fn static_bounded_value_precedes_consuming_binding_capture() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(
        root,
        static_label_with_consuming_capture("label".to_owned()),
    );
    let entity = app.world.query::<Text>().iter().next().unwrap().0;
    assert_eq!(
        app.world
            .get::<Text>(entity)
            .unwrap()
            .resolve(app.world.resource()),
        "label"
    );
    assert_eq!(
        app.world.get::<Style>(entity).unwrap().layout.width,
        mirui::types::Dimension::px(5)
    );
}

#[mirui::event]
fn inspect_static_attachment(
    entity: entity!(),
    mounted: query!(MountSeed, optional Switch, optional Parent, optional mirui::ui::NamedId),
    ids: res!(mirui::ui::IdMap),
    probe: res!(mut MountProbe),
) {
    let (seed, switch, parent, name) = mounted.get(entity).unwrap();
    let seed = seed.0;
    let on = switch.map(|switch| switch.on);
    let parent = parent.map(|parent| parent.0);
    let registered = name.is_some_and(|id| ids.get(id.0) == Some(entity));
    probe.entries[probe.count] = Some(MountEntry {
        seed,
        on,
        entity,
        parent,
        registered,
    });
    probe.count += 1;
}

#[mirui::view(component = MountSeed, name = "MountSeed", attach = inspect_static_attachment::callback())]
fn mount_seed_view() {}

#[compose]
fn static_attachment_tree(flag: mirui::core::reactive::Signal<bool>) {
    ui! {
        Column () {
            Switch (on: ${ flag.get() }, id: "mounted-switch") [
                MountSeed(7),
            ]
            View (id: "mounted-container") [
                MountSeed(9),
            ]
            Row ()
        }
    };
}

#[test]
fn static_attachments_initialize_once_before_reactive_values() {
    let mut app = mirui::app::App::headless(32, 32);
    app.with_default_widgets()
        .with_widget(mount_seed_view::view());
    app.add_resource(MountProbe::default());
    let root = app.spawn_root().id();
    let flag = mirui::core::reactive::Signal::new(true);
    app.compose(root, static_attachment_tree(flag));

    let probe = app.world.resource::<MountProbe>().unwrap();
    assert_eq!(probe.count, 2);
    let first = probe.entries[0].unwrap();
    assert_eq!((first.seed, first.on), (7, Some(false)));
    let switch = first.entity;
    assert!(
        app.world
            .get::<mirui::ui::widgets::Switch>(switch)
            .unwrap()
            .on
    );
    let second = probe.entries[1].unwrap();
    assert_eq!((second.seed, second.on), (9, None));
    let container = second.entity;
    assert!(!app.world.has::<mirui::ui::HitTarget>(container));
    assert!(!app.world.has::<mirui::ui::InteractionFeedback>(container));
    let parent = app.world.get::<Parent>(switch).unwrap().0;
    assert_eq!(first.parent, Some(parent));
    assert_eq!(second.parent, Some(parent));
    assert!(first.registered && second.registered);
    assert_eq!(app.world.get::<Parent>(parent).unwrap().0, root);
    assert_eq!(
        app.world
            .get::<Children>(parent)
            .unwrap()
            .0
            .iter()
            .filter(|child| **child == switch)
            .count(),
        1
    );
}

#[derive(Debug, PartialEq, Eq)]
struct OrderedAttachment(u8);

fn ordered_attachment(order: &RefCell<Vec<u8>>, value: u8) -> OrderedAttachment {
    order.borrow_mut().push(value);
    OrderedAttachment(value)
}

#[test]
fn attachment_expressions_keep_source_order_and_same_type_last_wins() {
    let mut app = mirui::app::App::headless(64, 64);
    let root = app.spawn_root().id();
    let order = RefCell::new(Vec::new());

    app.compose(root, |cx| {
        ui! {
            View () [
                ordered_attachment(&order, 1),
                ordered_attachment(&order, 2),
            ]
        }
    });
    let world = &app.world;

    assert_eq!(&*order.borrow(), &[1, 2]);
    let entities: Vec<_> = world.query::<OrderedAttachment>().collect();
    assert_eq!(entities.len(), 1);
    assert_eq!(
        world.get::<OrderedAttachment>(entities[0]),
        Some(&OrderedAttachment(2))
    );
}

#[mirui::component]
struct RowTag(u8);

#[compose]
fn indexed_rows(items: mirui::core::reactive::Signal<Vec<u8>>) -> Entity {
    ui! {
        Column () {
            walk ${ items.get() } with item {
                View () [
                    RowTag(item),
                ] {
                    View ()
                }
            }
        }
    }
}

#[compose]
fn keyed_rows(items: mirui::core::reactive::Signal<Vec<u8>>) -> Entity {
    ui! {
        Column () {
            walk ${ items.get() } with item by item {
                lowercase_row (item)
            }
        }
    }
}

#[compose]
fn lowercase_row(value: u8) -> Entity {
    ui! {
        View () [
            RowTag(value),
        ] {
            View ()
        }
    }
}

#[test]
fn reactive_row_roots_connect_once_and_keep_nested_children() {
    for keyed in [false, true] {
        let mut app = mirui::app::App::headless(32, 32);
        let root = app.spawn_root().id();
        let items = mirui::core::reactive::Signal::new(vec![1_u8, 2]);
        let list = if keyed {
            app.compose(root, keyed_rows(items.clone()))
        } else {
            app.compose(root, indexed_rows(items.clone()))
        };
        let original = app.world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(original.len(), 2);
        items.set(if keyed { vec![2, 1, 3] } else { vec![1, 2, 3] });
        mirui::core::reactive::flush_signal_dirty(&mut app.world);
        let rows = app.world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(rows.len(), 3);
        let expected = if keyed {
            [original[1], original[0]]
        } else {
            [original[0], original[1]]
        };
        assert_eq!(&rows[..2], &expected);
        for (index, entity) in rows.iter().enumerate() {
            assert!(!rows[..index].contains(entity));
            assert_eq!(app.world.get::<Parent>(*entity).unwrap().0, list);
            let nested = &app.world.get::<Children>(*entity).unwrap().0;
            assert_eq!(nested.len(), 1);
            assert_eq!(app.world.get::<Parent>(nested[0]).unwrap().0, *entity);
        }
        assert_eq!(app.world.get::<RowTag>(rows[2]).unwrap().0, 3);
        let removed = rows[2];
        items.set(vec![1, 2]);
        mirui::core::reactive::flush_signal_dirty(&mut app.world);
        assert_eq!(app.world.get::<Children>(list).unwrap().0, original);
        assert!(!app.world.is_alive(removed));
    }
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
    let mut app = mirui::app::App::headless(64, 64);
    let root = app.spawn_root().id();

    let column = app.compose(root, parent_with_lowercase_children());
    let world = &app.world;

    let children = &world
        .get::<Children>(column)
        .expect("Column has children")
        .0;
    assert_eq!(children.len(), 2);
    assert_eq!(world.get::<Parent>(children[0]).unwrap().0, column);
    assert_eq!(world.get::<Parent>(children[1]).unwrap().0, column);
    assert_eq!(
        world
            .get::<Text>(children[0])
            .unwrap()
            .resolve(world.resource()),
        "first"
    );
    assert_eq!(
        world
            .get::<Text>(children[1])
            .unwrap()
            .resolve(world.resource()),
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
        .resolve(world.resource())
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
    let mut app = mirui::app::App::headless(64, 64);
    app.with_widget(ui!(compose ParameterizedNicheCard));
    let root = app.spawn_root().id();

    app.compose(root, |cx| {
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
    });
    let world = &app.world;

    let first = tagged_entity(world, 1);
    let second = tagged_entity(world, 2);
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
    assert_eq!(single_text_child(world, first_body), "first fallback");
    assert_eq!(single_text_child(world, second_body), "second fill");
    assert_wrapper_style(world, first_body, Fixed::from_int(3), Fixed::from_int(5));
    assert_wrapper_style(world, second_body, Fixed::from_int(7), Fixed::from_int(11));
}
