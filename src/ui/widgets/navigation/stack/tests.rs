use super::*;
use crate::prelude::*;
use crate::ui::widgets::Text;
use crate::ui::{Children, Hidden};

fn animate(app: &mut TestApp, dt: u16) {
    app.world.insert_resource(crate::ecs::DeltaTimeMs(dt));
    transition::advance::<StackState<Route>>(&mut app.world).unwrap();
    settle(app);
    app.render().unwrap();
}

fn finish_pair(app: &mut TestApp, stack: Entity) {
    let destination = transition::running(&app.world, stack).unwrap().to;
    for _ in 0..240 {
        animate(app, 16);
        if transition::running(&app.world, stack).is_none_or(|pair| pair.to != destination) {
            return;
        }
    }
    panic!("page spring did not settle");
}

#[test]
fn animated_push_reverses_to_the_retained_root_and_pop_uses_the_opposite_direction() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, stack, path) = setup(0);
        app.world
            .get_mut::<NavigationStack<Route>>(stack)
            .unwrap()
            .transition = PageTransition::slide(config);
        let root = shown(&app.world, stack);
        path.push(Route::Details(1)).unwrap();
        app.render().unwrap();
        let first = shown(&app.world, stack);
        animate(&mut app, 80);
        path.pop().unwrap();
        settle(&mut app);
        finish_pair(&mut app, stack);
        assert_eq!(shown(&app.world, stack), root);
        assert!(!app.world.is_alive(first));
        path.push(Route::Details(2)).unwrap();
        app.render().unwrap();
        finish_pair(&mut app, stack);
        let second = shown(&app.world, stack);
        path.pop().unwrap();
        app.render().unwrap();
        let offset = app.world.get::<mount::PageOffset>(root).unwrap().0;
        assert_eq!(offset, -Fixed::ONE);
        assert!(app.world.is_alive(second));
        finish_pair(&mut app, stack);
        assert!(!app.world.is_alive(second));
        assert_eq!(shown(&app.world, stack), root);
    }
}

#[test]
fn animated_path_changes_keep_the_latest_goal_and_prune_departed_entries() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, stack, path) = setup(0);
        app.world
            .get_mut::<NavigationStack<Route>>(stack)
            .unwrap()
            .transition = PageTransition::slide(config);
        let root = shown(&app.world, stack);
        path.push(Route::Details(1)).unwrap();
        app.render().unwrap();
        let first = shown(&app.world, stack);
        for id in 2..10 {
            path.clear().unwrap();
            path.push(Route::Details(id)).unwrap();
            settle(&mut app);
            assert_eq!(app.world.query::<Text>().iter().count(), 2);
        }
        finish_pair(&mut app, stack);
        assert!(transition::running(&app.world, stack).is_some());
        finish_pair(&mut app, stack);
        assert!(!app.world.is_alive(first));
        assert_eq!(app.world.query::<Text>().iter().count(), 2);
        assert!(app.world.has::<Hidden>(root));
        assert_eq!(
            app.world
                .get::<StackState<Route>>(stack)
                .unwrap()
                .pages
                .len(),
            1
        );
    }
}

#[derive(Clone)]
enum Route {
    Details(u32),
    Rejected,
}

#[crate::event]
fn make_page(event: event!(CreatePage<Route>), target: entity!()) -> Entity {
    match event.route() {
        Route::Details(id) => ui! { Text(text: alloc::format!("Device {id}"), grow: 1.0) },
        Route::Rejected => target,
    }
}

#[crate::compose]
fn panel(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        NavigationStack(path: path, grow: 1.0) {
            Text("home", grow: 1.0)
        } on CreatePage(make_page::callback())
    }
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn setup(initial: usize) -> (TestApp, Entity, super::Path<Route>) {
    let mut app = App::headless(120, 80);
    app.with_default_widgets();
    app.with_widget(view::<Route>());
    let root = app.spawn_root().id();
    let path = app.add_model(NavigationPath::with_capacity(3)).finish();
    for id in 0..initial {
        path.push(Route::Details(id as u32)).unwrap();
    }
    let stack = app.compose(root, panel(path.clone()));
    app.render().unwrap();
    (app, stack, path)
}

fn shown(world: &World, stack: Entity) -> Entity {
    world.get::<StackState<Route>>(stack).unwrap().shown
}

fn settle(app: &mut TestApp) {
    crate::ui::delivery::flush(&mut app.world).unwrap();
}

#[test]
fn push_pop_keep_distinct_page_identity_and_restore_retained_content() {
    let (mut app, stack, path) = setup(0);
    let root = shown(&app.world, stack);
    path.push(Route::Details(7)).unwrap();
    settle(&mut app);
    let first = shown(&app.world, stack);
    let child = app.world.get::<Children>(first).unwrap().0[0];
    app.world
        .get_mut::<Text>(child)
        .unwrap()
        .set_content("edited");
    path.push(Route::Details(7)).unwrap();
    settle(&mut app);
    let second = shown(&app.world, stack);
    assert_ne!(first, second);
    assert!(app.world.has::<Hidden>(first));
    assert!(app.world.has::<Hidden>(root));
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(shown(&app.world, stack), first);
    assert!(!app.world.is_alive(second));
    assert!(!app.world.has::<Hidden>(first));
    assert_eq!(
        app.world
            .get::<Text>(child)
            .unwrap()
            .resolve(app.world.resource()),
        "edited"
    );
    path.clear().unwrap();
    settle(&mut app);
    assert_eq!(shown(&app.world, stack), root);
    assert!(!app.world.is_alive(first));
    assert!(
        app.world
            .get::<StackState<Route>>(stack)
            .unwrap()
            .pages
            .is_empty()
    );
}

#[test]
fn initially_deep_paths_construct_only_the_visible_destination() {
    let (mut app, stack, path) = setup(3);
    assert_eq!(
        app.world
            .get::<StackState<Route>>(stack)
            .unwrap()
            .pages
            .len(),
        1
    );
    assert_eq!(app.world.query::<Text>().iter().count(), 2);
    let leaf = shown(&app.world, stack);
    path.pop().unwrap();
    settle(&mut app);
    assert_ne!(shown(&app.world, stack), leaf);
    assert!(!app.world.is_alive(leaf));
    assert_eq!(app.world.query::<Text>().iter().count(), 2);
}

#[test]
fn rejected_destinations_keep_the_visible_page_without_repeated_construction() {
    let (mut app, stack, path) = setup(1);
    let previous = shown(&app.world, stack);
    path.push(Route::Rejected).unwrap();
    settle(&mut app);
    assert_eq!(shown(&app.world, stack), previous);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..5 {
                settle(&mut app);
            }
        }),
        0
    );
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(shown(&app.world, stack), previous);
}

#[test]
fn a_second_container_cannot_claim_a_live_path_and_can_claim_it_after_retirement() {
    let (mut app, first, path) = setup(1);
    let root = app.world.resource::<crate::ui::WidgetRoot>().unwrap().0;
    let second = app.compose(root, panel(path.clone()));
    settle(&mut app);
    assert!(app.world.get::<StackState<Route>>(first).unwrap().claimed);
    assert!(!app.world.get::<StackState<Route>>(second).unwrap().claimed);
    assert!(
        app.world
            .get::<StackState<Route>>(second)
            .unwrap()
            .pages
            .is_empty()
    );
    crate::ui::despawn_subtree(&mut app.world, first);
    settle(&mut app);
    assert!(app.world.get::<StackState<Route>>(second).unwrap().claimed);
    assert_eq!(
        app.world
            .get::<StackState<Route>>(second)
            .unwrap()
            .pages
            .len(),
        1
    );
}

#[test]
fn returning_to_a_cached_page_invalidates_old_input_leases() {
    let (mut app, stack, path) = setup(0);
    let root = shown(&app.world, stack);
    let lease = crate::core::reactive::retained::InputLease::capture(&app.world, root).unwrap();
    assert!(lease.valid(&app.world));
    path.push(Route::Details(1)).unwrap();
    settle(&mut app);
    assert!(!lease.valid(&app.world));
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(shown(&app.world, stack), root);
    assert!(!lease.valid(&app.world));
    assert!(
        crate::core::reactive::retained::InputLease::capture(&app.world, root)
            .unwrap()
            .valid(&app.world)
    );
}

#[test]
fn returning_restores_valid_focus_without_overriding_another_focused_area() {
    use crate::input::event::focus::{FocusState, Focusable};
    let (mut app, stack, path) = setup(0);
    let root = shown(&app.world, stack);
    let child = app.world.get::<Children>(root).unwrap().0[0];
    app.world.insert(child, Focusable);
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(child);
    path.push(Route::Details(1)).unwrap();
    settle(&mut app);
    assert_eq!(app.world.resource::<FocusState>().unwrap().focused, None);
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(child)
    );
    path.push(Route::Details(2)).unwrap();
    settle(&mut app);
    let outside = app.world.spawn_empty();
    app.world.insert(outside, Focusable);
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(outside);
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(outside)
    );
}

#[test]
fn full_capacity_replacement_reuses_prepared_page_metadata() {
    let (mut app, stack, path) = setup(0);
    for id in 0..3 {
        path.push(Route::Details(id)).unwrap();
        settle(&mut app);
    }
    let capacity = app
        .world
        .get::<StackState<Route>>(stack)
        .unwrap()
        .pages
        .capacity();
    path.clear().unwrap();
    path.push(Route::Details(9)).unwrap();
    settle(&mut app);
    let state = app.world.get::<StackState<Route>>(stack).unwrap();
    assert_eq!(state.pages.capacity(), capacity);
    assert_eq!(state.pages.len(), 1);
}

#[test]
fn deleted_pages_are_reconstructed_without_reusing_dead_cache_entries() {
    let (mut app, stack, path) = setup(1);
    let first = shown(&app.world, stack);
    crate::ui::despawn_subtree(&mut app.world, first);
    settle(&mut app);
    let replacement = shown(&app.world, stack);
    assert_ne!(first, replacement);
    assert!(app.world.is_alive(replacement));
    assert_eq!(
        app.world
            .get::<StackState<Route>>(stack)
            .unwrap()
            .pages
            .len(),
        1
    );
    let root = app.world.get::<StackState<Route>>(stack).unwrap().root;
    crate::ui::despawn_subtree(&mut app.world, root);
    settle(&mut app);
    assert!(!app.world.is_alive(replacement));
    path.clear().unwrap();
    settle(&mut app);
    assert!(app.world.is_alive(shown(&app.world, stack)));
}

#[crate::compose]
fn reentrant_panel(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        NavigationStack(path: path.clone()) {
            Text("home")
        } on CreatePage {
            if matches!(event.route(), Route::Details(0)) {
                path.push(Route::Details(1)).unwrap();
            }
            ui! { Text("page") }
        }
    }
}

#[test]
fn factory_navigation_retires_obsolete_construction_before_mounting_the_latest_entry() {
    let mut app = App::headless(120, 80);
    app.with_default_widgets();
    app.with_widget(view::<Route>());
    let root = app.spawn_root().id();
    let path = app.add_model(NavigationPath::with_capacity(3)).finish();
    path.push(Route::Details(0)).unwrap();
    let stack = app.compose(root, reentrant_panel(path.clone()));
    settle(&mut app);
    assert_eq!(path.len(), 2);
    assert_eq!(
        app.world
            .get::<StackState<Route>>(stack)
            .unwrap()
            .pages
            .len(),
        1
    );
    assert_eq!(app.world.query::<Text>().iter().count(), 2);
    path.pop().unwrap();
    settle(&mut app);
    assert_eq!(path.len(), 2);
    assert_eq!(app.world.query::<Text>().iter().count(), 2);
}
