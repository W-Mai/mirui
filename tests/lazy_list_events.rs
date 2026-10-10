use mirui::core::reactive::flush_signal_dirty;
use mirui::ecs::SystemScheduler;
use mirui::input::event::scroll::ScrollOffset;
use mirui::prelude::*;
use mirui::types::Rect;
use mirui::ui::widgets::lazy_list::{CreateCell, lazy_list_system};
use mirui::ui::widgets::{LazyList, ListItem, Text};
use mirui::ui::{Children, ComputedRect};

#[model]
struct Catalog {
    #[observe]
    revision: u32,
    #[observe]
    count: u32,
    creations: u32,
}

#[model]
impl Catalog {
    fn resize(&mut self, count: u32) {
        self.count = count;
        self.revision += 1;
    }
    fn created(&mut self) {
        self.creations += 1;
    }
    fn creations(&self) -> u32 {
        self.creations
    }
    fn label(&self, index: u32) -> bool {
        assert!(
            index < self.count,
            "inactive cells must not read expired data"
        );
        self.revision % 2 == 0
    }
}

#[compose]
fn cell(item: model!(ListItem), data: model!(Catalog)) -> Entity {
    ui! {
        Text (
            text: ${
                {
                    let _ = data.revision();
                    if data.label(item.index()) { "even" } else { "odd" }
                }
            }
        )
    }
}

#[event]
fn create(data: model!(Catalog), event: event!(CreateCell)) -> Entity {
    data.created();
    ui! {
        cell (event.item.clone(), data.clone())
    }
}

#[compose]
fn list(data: model!(Catalog)) -> Entity {
    ui! {
        LazyList (items: ${ data.count() }, item_extent: Fixed::from_int(40)) on CreateCell(create::callback(data.clone()))
    }
}

fn geometry(
    world: &mut mirui::ecs::World,
    scheduler: &mut SystemScheduler,
    list: Entity,
    offset: i32,
) {
    world.insert(list, ComputedRect(Rect::new(0, 0, 200, 80)));
    world.get_mut::<ScrollOffset>(list).unwrap().y = offset.into();
    scheduler.run_all(world).unwrap();
    flush_signal_dirty(world);
}

#[test]
fn data_shrink_suspends_expired_cells_before_property_evaluation_and_reuses_them() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    let data = app.add_model(Catalog {
        revision: 0,
        count: 100,
        creations: 0,
    });
    let root = app.spawn_root().id();
    let list = app.compose(root, list(data.clone()));
    geometry(&mut app.world, &mut scheduler, list, 800);
    let cells = app.world.get::<Children>(list).unwrap().0.clone();
    data.resize(0);
    flush_signal_dirty(&mut app.world);
    scheduler.run_all(&mut app.world).unwrap();
    assert_eq!(app.world.get::<LazyList>(list).unwrap().items.len(), 0);
    assert!(
        cells
            .iter()
            .all(|&cell| app.world.has::<mirui::ui::Hidden>(cell))
    );
    data.resize(100);
    flush_signal_dirty(&mut app.world);
    geometry(&mut app.world, &mut scheduler, list, 800);
    assert_eq!(data.creations() as usize, cells.len());
    assert_eq!(app.world.get::<Children>(list).unwrap().0, cells);
    for &cell in &cells {
        assert!(!app.world.has::<mirui::ui::Hidden>(cell));
        assert_eq!(
            app.world
                .get::<Text>(cell)
                .unwrap()
                .resolve(app.world.resource()),
            "even"
        );
    }
    data.resize(100);
    flush_signal_dirty(&mut app.world);
    for &cell in &cells {
        assert_eq!(
            app.world
                .get::<Text>(cell)
                .unwrap()
                .resolve(app.world.resource()),
            "odd"
        );
    }
}

#[test]
fn replacing_a_creation_callback_rebuilds_cells_and_rejects_an_existing_root() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    let data = app.add_model(Catalog {
        revision: 0,
        count: 100,
        creations: 0,
    });
    let root = app.spawn_root().id();
    let good = app.compose(root, list(data.clone()));
    geometry(&mut app.world, &mut scheduler, good, 0);
    assert_eq!(data.creations(), 4);
    let previous = app.world.get::<Children>(good).unwrap().0.clone();
    app.compose(root, |tree| {
        tree.on_event(good, LazyList::CreateCell, existing::callback(root))
    });
    geometry(&mut app.world, &mut scheduler, good, 0);
    assert_eq!(data.creations(), 4);
    assert!(app.world.get::<Children>(good).unwrap().0.is_empty());
    assert!(previous.iter().all(|&entity| !app.world.is_alive(entity)));
    let invalid = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(100, 40));
        tree.on_event(list, LazyList::CreateCell, existing::callback(root));
        list
    });
    let before = app.world.query::<mirui::ui::Widget>().iter().count();
    geometry(&mut app.world, &mut scheduler, invalid, 0);
    assert_eq!(
        app.world.query::<mirui::ui::Widget>().iter().count(),
        before
    );
    assert!(app.world.get::<Children>(invalid).unwrap().0.is_empty());
    assert!(app.world.is_alive(root));
}

#[test]
fn multiple_roots_are_disposed_instead_of_leaking_partial_cells() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    let root = app.spawn_root().id();
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(10, 40));
        tree.on_event(list, LazyList::CreateCell, invalid::callback());
        list
    });
    let before = app.world.query::<mirui::ui::Widget>().iter().count();
    geometry(&mut app.world, &mut scheduler, list, 0);
    assert_eq!(
        app.world.query::<mirui::ui::Widget>().iter().count(),
        before
    );
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
}

#[event]
fn existing(entity: Entity, _event: event!(CreateCell)) -> Entity {
    entity
}

#[event]
fn invalid(_event: event!(CreateCell)) -> Entity {
    ui! {
        Text ("orphan")
    };
    ui! {
        Text ("result")
    }
}

#[event]
fn rejected_commands(_event: event!(CreateCell), commands: commands!()) -> Entity {
    for _ in 0..5 {
        commands.remove_resource::<u32>();
    }
    ui! {
        Text ("discarded")
    }
}

#[test]
fn failed_command_submission_discards_the_new_cell() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    app.add_resource(7_u32);
    let root = app.spawn_root().id();
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(10, 40));
        tree.on_event(list, LazyList::CreateCell, rejected_commands::callback());
        list
    });
    let before = app.world.query::<mirui::ui::Widget>().iter().count();
    geometry(&mut app.world, &mut scheduler, list, 0);
    assert_eq!(
        app.world.query::<mirui::ui::Widget>().iter().count(),
        before
    );
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    assert_eq!(app.world.resource::<u32>(), Some(&7));
}

#[component]
struct Mounted;

#[event]
fn check_parent(cell: entity!(), parents: query!(mirui::ui::Parent), lists: query!(LazyList)) {
    let parent = parents.get(cell).unwrap().0;
    assert!(
        lists.get(parent).is_some(),
        "attachment must see the actual list parent"
    );
}

#[view(component = Mounted, attach = check_parent::callback())]
fn mounted_view() {}

#[event]
fn mounted(_event: event!(CreateCell)) -> Entity {
    ui! {
        Text ("mounted") [
            Mounted,
        ]
    }
}

#[event]
fn panicking(_event: event!(CreateCell)) -> Entity {
    ui! {
        Text ("discard on unwind")
    };
    panic!("construction stopped");
}

#[test]
fn cell_attachment_uses_the_real_parent_and_unwinding_cleans_new_roots() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets().with_widget(mounted_view::view());
    let mut scheduler = SystemScheduler::new();
    scheduler.add(lazy_list_system::system());
    let root = app.spawn_root().id();
    let good = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(10, 40));
        tree.on_event(list, LazyList::CreateCell, mounted::callback());
        list
    });
    geometry(&mut app.world, &mut scheduler, good, 0);
    assert_eq!(app.world.get::<Children>(good).unwrap().0.len(), 4);
    let failing = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(10, 40));
        tree.on_event(list, LazyList::CreateCell, panicking::callback());
        list
    });
    let count = app.world.query::<mirui::ui::Widget>().iter().count();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| geometry(
            &mut app.world,
            &mut scheduler,
            failing,
            0
        )))
        .is_err()
    );
    assert_eq!(app.world.query::<mirui::ui::Widget>().iter().count(), count);
    assert!(app.world.get::<Children>(failing).unwrap().0.is_empty());
}
