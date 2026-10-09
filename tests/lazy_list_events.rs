use mirui::input::event::scroll::ScrollOffset;
use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::{BindItem, lazy_list_system};
use mirui::ui::widgets::{LazyList, LazyListPool, Text};
use mirui::ui::{Children, Hidden, HitTarget, InteractionFeedback};

#[model]
struct Counts {
    calls: [u32; 4],
}

#[model]
impl Counts {
    fn record(&mut self, slot: usize, change: &BindItem) {
        self.calls[slot] += change.index + 1;
    }

    fn calls(&self) -> [u32; 4] {
        self.calls
    }
}

#[event]
fn record(counts: model!(Counts), slot: usize, change: event!(BindItem)) {
    counts.record(slot, change);
}

#[compose]
fn callback_forms(counts: model!(Counts)) {
    ui! {
        Column () {
            LazyList (item_count: 10_u32, item_height: Fixed::from_int(20), pool_size: 2_u8) {
                walk 0..2 with _i {
                    Text ("", height: 20)
                }
            } on BindItem { counts.record(0, event); }
            LazyList (item_count: 10_u32, item_height: Fixed::from_int(20), pool_size: 2_u8) {
                walk 0..2 with _i {
                    Text ("", height: 20)
                }
            } on BindItem(|change: event!(BindItem)| {
                counts.record(1, change);
            })
            View () [
                LazyList::new(10, 20, 2),
            ] {
                walk 0..2 with _i {
                    Text ("", height: 20)
                }
            } on LazyList::BindItem(record::callback(counts.clone(), 2_usize))
        }
    };
}

#[test]
fn callback_forms_share_model_identity_without_input_or_business_storage() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets()
        .with_event_capacity(EventCapacity::new());
    let counts = app.add_model(Counts { calls: [0; 4] });
    let root = app.spawn_root().id();
    app.compose(root, callback_forms(counts.clone()));
    app.compose(root, |tree| {
        let (list, ()) = tree.children(LazyList::new(10, 20, 2), |rows| {
            rows.spawn(Text::from(""));
            rows.spawn(Text::from(""));
        });
        tree.on_event(
            list,
            LazyList::BindItem,
            record::callback(counts.clone(), 3_usize),
        );
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(counts.calls(), [3; 4]);
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(counts.calls(), [3; 4]);
    for list in app.world.query::<LazyList>().collect() {
        assert!(!app.world.has::<HitTarget>(list));
        assert!(!app.world.has::<InteractionFeedback>(list));
    }
}

#[derive(Default)]
struct Trace(Vec<(Entity, u32)>);

#[event]
fn write_row(
    change: event!(BindItem),
    list: entity!(),
    pools: query!(LazyListPool),
    rows: query!(mut Text, mut Style),
    trace: res!(mut Trace),
) {
    assert_eq!(pools.get(list).unwrap().bound_indices, [0, 1, 2]);
    let (text, style) = rows.get(change.item).unwrap();
    text.set_content(["A", "B", "C"][change.index as usize]);
    style.set_bg_color(ColorToken::Primary);
    trace.0.push((list, change.index));
}

type TestApp = App<mirui::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn list_app(count: u32) -> (TestApp, Entity) {
    let mut app = App::headless(100, 100);
    app.with_default_widgets().add_resource(Trace::default());
    let root = app.spawn_root().id();
    let list = app.compose(root, |cx| {
        ui! {
            LazyList (item_count: count, item_height: Fixed::from_int(20), pool_size: 3_u8) {
                walk 0..3 with _i {
                    Text ("", height: 20, position: Position::Absolute)
                }
            }
        }
    });
    (app, list)
}

#[test]
fn callbacks_observe_complete_pool_and_update_text_and_style_before_layout() {
    let (mut app, list) = list_app(10);
    let root = app.root.unwrap();
    app.compose(root, |tree| {
        tree.on_event(list, LazyList::BindItem, write_row::callback())
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().0,
        [(list, 0), (list, 1), (list, 2)]
    );
    for (index, row) in app
        .world
        .get::<Children>(list)
        .unwrap()
        .0
        .iter()
        .enumerate()
    {
        assert_eq!(
            app.world.get::<Text>(*row).unwrap().resolve(&app.world),
            ["A", "B", "C"][index]
        );
        assert_eq!(
            app.world.get::<Style>(*row).unwrap().bg_color,
            Some(ColorToken::Primary.into())
        );
        assert_eq!(
            app.world.get::<Style>(*row).unwrap().layout.top,
            Dimension::px(20 * index as i32)
        );
    }
}

#[test]
fn unobserved_lists_manage_slots_and_late_listeners_do_not_replay() {
    let (mut app, list) = list_app(2);
    lazy_list_system(&mut app.world).unwrap();
    let rows = app.world.get::<Children>(list).unwrap().0.clone();
    assert!(!app.world.has::<Hidden>(rows[0]));
    assert!(app.world.has::<Hidden>(rows[2]));
    assert_eq!(
        app.world.get::<LazyListPool>(list).unwrap().bound_indices,
        [0, 1, u32::MAX]
    );
    let counts = app.add_model(Counts { calls: [0; 4] });
    let root = app.root.unwrap();
    app.compose(root, |tree| {
        tree.on_event(
            list,
            LazyList::BindItem,
            record::callback(counts.clone(), 0_usize),
        )
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(counts.calls(), [0; 4]);
    app.world.get_mut::<LazyList>(list).unwrap().item_count = 10;
    app.world.insert(
        list,
        ScrollOffset {
            x: Fixed::ZERO,
            y: Fixed::from_int(60),
        },
    );
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(counts.calls(), [15, 0, 0, 0]);
    assert!(!app.world.has::<Hidden>(rows[2]));
}

#[event]
fn replace_pool(
    change: event!(BindItem),
    list: entity!(),
    commands: commands!(),
    trace: res!(mut Trace),
) {
    trace.0.push((list, change.index));
    commands.insert(list, LazyListPool::new(Vec::new()));
}

#[test]
fn replaced_pool_is_not_overwritten_or_notified_from_an_old_snapshot() {
    let (mut app, list) = list_app(10);
    let root = app.root.unwrap();
    app.with_event_capacity(
        EventCapacity::new().commands(CommandCapacity::new().insert::<LazyListPool>(1)),
    );
    app.compose(root, |tree| {
        tree.on_event(list, LazyList::BindItem, replace_pool::callback())
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().0, [(list, 0)]);
    assert!(
        app.world
            .get::<LazyListPool>(list)
            .unwrap()
            .items
            .is_empty()
    );
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().0, [(list, 0)]);
}

#[event]
fn retire_rows(
    change: event!(BindItem),
    list: entity!(),
    pools: query!(LazyListPool),
    trace: res!(mut Trace),
    commands: commands!(),
) {
    trace.0.push((list, change.index));
    commands.despawn(pools.get(list).unwrap().items[1]);
    commands.insert(list, LazyList::new(2, 20, 3));
}

#[test]
fn callback_deletion_and_data_shrink_skip_remaining_invalid_items() {
    let (mut app, list) = list_app(10);
    app.with_event_capacity(
        EventCapacity::new().commands(CommandCapacity::new().despawn(1).insert::<LazyList>(1)),
    );
    let root = app.root.unwrap();
    app.compose(root, |tree| {
        tree.on_event(list, LazyList::BindItem, retire_rows::callback())
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().0, [(list, 0)]);
    assert_eq!(app.world.get::<LazyList>(list).unwrap().item_count, 2);
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().0,
        [(list, 0), (list, 1)]
    );
    let pool = app.world.get::<LazyListPool>(list).unwrap();
    assert!(!app.world.is_alive(pool.items[1]));
    assert!(app.world.is_alive(pool.items[2]));
    assert_eq!(pool.bound_indices, [0, u32::MAX, 1]);
}

#[test]
fn failed_commands_preserve_updates_without_retrying_the_same_binding() {
    let (mut app, list) = list_app(10);
    app.with_event_capacity(EventCapacity::new().commands(CommandCapacity::new()));
    let root = app.root.unwrap();
    app.compose(root, |tree| {
        tree.on_event(list, LazyList::BindItem, replace_pool::callback())
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().0,
        [(list, 0), (list, 1), (list, 2)]
    );
    assert_eq!(
        app.world.get::<LazyListPool>(list).unwrap().bound_indices,
        [0, 1, 2]
    );
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().0.len(), 3);
}

struct RowReady(u32);

impl Event for RowReady {
    type Route = mirui::input::event::contract::EntityNotification;
    type Source = mirui::input::event::contract::WidgetSource<LazyList>;
    type Options = ();
}

#[event]
fn announce(
    change: event!(BindItem),
    list: entity!(),
    trace: res!(mut Trace),
    commands: commands!(),
) {
    trace.0.push((list, change.index));
    commands.emit(list, RowReady(change.index));
}

#[event]
fn receive(change: event!(RowReady), list: entity!(), trace: res!(mut Trace)) {
    assert!(trace.0.len() >= 6);
    trace.0.push((list, 20 + change.0));
}

#[event]
fn second_listener(change: event!(BindItem), list: entity!(), trace: res!(mut Trace)) {
    trace.0.push((list, 10 + change.index));
}

#[test]
fn listener_order_and_business_notifications_finish_after_the_binding_batch() {
    let (mut app, list) = list_app(10);
    app.with_event_capacity(
        EventCapacity::new()
            .notifications::<RowReady>(3)
            .commands(CommandCapacity::new().emit::<RowReady>(1))
            .flush_limit(3),
    );
    let root = app.root.unwrap();
    app.compose(root, |tree| {
        tree.on_event(list, LazyList::BindItem, announce::callback());
        tree.on_event(list, LazyList::BindItem, second_listener::callback());
        tree.on_event(list, RowReady::EVENT, receive::callback());
    });
    lazy_list_system(&mut app.world).unwrap();
    let values: Vec<_> = app
        .world
        .resource::<Trace>()
        .unwrap()
        .0
        .iter()
        .map(|(_, index)| *index)
        .collect();
    assert_eq!(values, [0, 10, 1, 11, 2, 12, 20, 21, 22]);
}

#[test]
fn generic_node_installation_rejects_a_missing_list_source() {
    let mut app = App::headless(10, 10);
    let counts = app.add_model(Counts { calls: [0; 4] });
    let root = app.spawn_root().id();
    app.compose(root, |tree| {
        tree.on_event(
            root,
            LazyList::BindItem,
            record::callback(counts.clone(), 0_usize),
        );
    });
    app.world.insert(root, LazyList::new(1, 20, 1));
    app.compose(root, |tree| {
        tree.spawn(Text::from(""));
    });
    lazy_list_system(&mut app.world).unwrap();
    assert_eq!(counts.calls(), [0; 4]);
}
