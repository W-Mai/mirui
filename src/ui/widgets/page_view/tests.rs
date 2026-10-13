use super::*;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Details,
    Rejected,
    Other(u8),
}

#[crate::model]
struct SelectionModel {
    #[observe]
    selected: Route,
}

#[crate::model]
impl SelectionModel {
    fn select(&mut self, page: Route) {
        self.selected = page;
    }
}

#[crate::event]
fn make_page(event: event!(CreatePage<Route>), target: entity!()) -> Entity {
    match *event.route() {
        Route::Home => ui! { Text("home", grow: 1.0) },
        Route::Details => ui! { Text("details", grow: 1.0) },
        Route::Rejected => {
            ui! { Text("discarded") };
            target
        }
        Route::Other(_) => ui! { Text("other", grow: 1.0) },
    }
}

#[crate::compose]
fn panel(selection: model!(SelectionModel)) -> Entity {
    ui! {
        PageView(page: ${ selection.selected() }, grow: 1.0)
        on CreatePage(make_page::callback())
    }
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn setup() -> (TestApp, Entity, SelectionModelHandle) {
    let mut app = App::headless(120, 80);
    app.with_default_widgets();
    app.with_widget(view::<Route>());
    let root = app.spawn_root().id();
    let selection = app
        .add_model(SelectionModel {
            selected: Route::Home,
        })
        .finish();
    let page = app.compose(root, panel(selection.clone()));
    app.render().unwrap();
    (app, page, selection)
}

fn mounted(world: &World, container: Entity) -> Entity {
    world
        .get::<PageState<Route>>(container)
        .unwrap()
        .mounted
        .unwrap()
}

#[test]
fn selection_reuses_equal_values_and_retires_only_the_previous_subtree() {
    let (mut app, container, selection) = setup();
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    let home = mounted(&app.world, container);
    selection.select(Route::Home);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert_eq!(mounted(&app.world, container), home);
    selection.select(Route::Details);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    let details = mounted(&app.world, container);
    assert_ne!(details, home);
    assert!(!app.world.is_alive(home));
    assert!(app.world.is_alive(root));
    assert!(app.world.is_alive(container));
    assert!(mount::attached(&app.world, container, details));
    app.world
        .get_mut::<PageView<Route>>(container)
        .unwrap()
        .page = None;
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert!(!app.world.is_alive(details));
    assert_eq!(app.world.query::<Text>().iter().count(), 0);
    assert_eq!(selection.selected(), Route::Details);
}

mod animation;

#[test]
fn rejected_construction_keeps_the_page_and_does_not_retry_each_frame() {
    let (mut app, container, selection) = setup();
    let home = mounted(&app.world, container);
    let count = app.world.live_entity_count();
    selection.select(Route::Rejected);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert_eq!(mounted(&app.world, container), home);
    assert_eq!(app.world.live_entity_count(), count);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..5 {
                crate::ui::delivery::flush(&mut app.world).unwrap();
            }
        }),
        0
    );
    selection.select(Route::Details);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert!(!app.world.is_alive(home));
}

#[crate::event]
fn redirect_page(event: event!(CreatePage<Route>), selection: model!(SelectionModel)) -> Entity {
    if *event.route() == Route::Home {
        selection.select(Route::Details);
    }
    ui! { Text("page") }
}

#[crate::compose]
fn redirected(selection: model!(SelectionModel)) -> Entity {
    ui! {
        PageView(page: ${ selection.selected() }, grow: 1.0)
        on CreatePage(redirect_page::callback(selection))
    }
}

#[test]
fn factory_mutations_are_reconciled_without_retaining_an_obsolete_page() {
    let mut app = App::headless(120, 80);
    app.with_default_widgets();
    app.with_widget(view::<Route>());
    let root = app.spawn_root().id();
    let selection = app
        .add_model(SelectionModel {
            selected: Route::Home,
        })
        .finish();
    let container = app.compose(root, redirected(selection.clone()));
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert_eq!(selection.selected(), Route::Details);
    assert!(
        app.world
            .get::<PageState<Route>>(container)
            .unwrap()
            .attempted
            .page
            == Some(Route::Details)
    );
    assert_eq!(app.world.get::<Children>(container).unwrap().0.len(), 1);
    assert_eq!(app.world.query::<Text>().iter().count(), 1);
}

#[test]
fn three_callback_forms_and_builder_use_one_construction_contract() {
    #[crate::compose]
    fn forms() -> (Entity, Entity) {
        let block = ui! {
            PageView(page: Route::Home) on CreatePage {
                assert!(*event.route() == Route::Home);
                ui! { Text("block") }
            }
        };
        let inline = ui! {
            PageView(page: Route::Details)
            on CreatePage(|event: event!(CreatePage<Route>)| {
                assert!(*event.route() == Route::Details);
                ui! { Text("inline") }
            })
        };
        (block, inline)
    }
    let (mut app, named, _) = setup();
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    let (block, inline) = app.compose(root, forms());
    let builder = app.compose(root, |tree| {
        let page = tree.spawn(PageView::new(Route::Home));
        tree.on_event(page, PageView::CreatePage, make_page::callback());
        page
    });
    crate::ui::delivery::flush(&mut app.world).unwrap();
    for page in [named, block, inline, builder] {
        assert!(app.world.is_alive(mounted(&app.world, page)));
        assert!(!app.world.has::<crate::ui::HitTarget>(page));
    }
}

#[crate::event]
fn remove_container(
    _event: event!(CreatePage<Route>),
    target: entity!(),
    commands: commands!(),
) -> Entity {
    let child = ui! { Text("temporary") };
    commands.despawn(target);
    child
}

#[test]
fn removing_the_container_during_construction_cleans_its_entire_tree() {
    let (mut app, container, _) = setup();
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    app.compose(root, |tree| {
        tree.on_event(
            container,
            PageView::<Route>::CreatePage,
            remove_container::callback(),
        );
    });
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert!(!app.world.is_alive(container));
    assert!(app.world.is_alive(root));
    assert_eq!(app.world.query::<Text>().iter().count(), 0);
}

#[test]
fn removing_a_mounted_page_and_replacing_a_provider_rebuild_once() {
    let (mut app, container, _) = setup();
    let home = mounted(&app.world, container);
    crate::ui::despawn_subtree(&mut app.world, home);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    let replacement = mounted(&app.world, container);
    assert_ne!(home, replacement);
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    app.compose(root, |tree| {
        tree.on_event(container, PageView::CreatePage, make_page::callback());
    });
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert!(!app.world.is_alive(replacement));
    let current = mounted(&app.world, container);
    crate::ui::delivery::flush(&mut app.world).unwrap();
    assert_eq!(current, mounted(&app.world, container));
    assert_eq!(app.world.query::<Text>().iter().count(), 1);
}
