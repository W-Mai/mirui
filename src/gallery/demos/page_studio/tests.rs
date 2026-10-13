use super::*;
use crate::core::model::BindType;
use crate::ecs::DeltaTimeMs;
use crate::input::event::input::{InputEvent, PointerKind};
use crate::input::event::scroll::ScrollOffset;
use crate::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
use crate::ui::ComputedRect;
use crate::ui::widgets::navigation::{NavigationPath, NavigationStack, PageTransition, transition};
use crate::ui::widgets::page_view::PageView;
use crate::ui::widgets::{FollowBounds, Slider, Switch, Text};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;
type Path = <NavigationPath<Route> as BindType>::Shared;
type Controls = <Studio as BindType>::Shared;

fn fixture() -> (TestApp, Path, Controls) {
    let mut app = App::headless(960, 680);
    app.with_default_widgets().with_default_systems();
    app.add_system(sim_input_system::system());
    app.add_resource(DeltaTimeMs(16));
    app.add_plugin(crate::gallery::play::font::PlayFontPlugin);
    app.with_widget(navigation::view::<Route>())
        .with_widget(page_view::view::<Scene>());
    app.with_offscreen_pool_budget(960 * 680 * 8);
    let root = app.spawn_root().id();
    let path = app.add_model(NavigationPath::with_capacity(8)).finish();
    let controls = app.add_model(Studio::default()).finish();
    app.compose(root, build_widgets(path.clone(), controls.clone()));
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    (app, path, controls)
}

#[test]
fn gallery_setup_registers_the_showcase_palettes() {
    let mut app = App::headless(960, 680);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    for theme in [
        crate::gallery::showcase_theme::DARK_ID,
        crate::gallery::showcase_theme::LIGHT_ID,
    ] {
        app.set_theme(theme).unwrap();
        app.prepare_text_layout().unwrap();
        app.render().unwrap();
    }
}

fn click(app: &mut TestApp, entity: Entity) {
    let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
    let x = rect.x + rect.w / Fixed::from_int(2);
    let y = rect.y + rect.h / Fixed::from_int(2);
    app.add_resource(SimulatedInput::new(alloc::vec![
        SimCommand::at(
            0,
            InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y
            }
        ),
        SimCommand::at(
            0,
            InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x,
                y
            }
        ),
    ]));
    assert!(!app.tick());
}

fn click_id(app: &mut TestApp, id: &'static str) {
    let entity = app.world.find_by_id(id).unwrap();
    click(app, entity);
}

fn text_entity(app: &TestApp, label: &str) -> Entity {
    app.world
        .query::<Text>()
        .iter()
        .find(|(_, text)| text.resolve(app.world.resource()) == label)
        .unwrap()
        .0
}

fn finish(app: &mut TestApp) {
    for _ in 0..240 {
        assert!(!app.tick());
        let running = ["pages_stack", "pages_scene"]
            .iter()
            .any(|id| transition::running(&app.world, app.world.find_by_id(id).unwrap()).is_some());
        if !running {
            return;
        }
    }
    panic!("page transition did not settle");
}

#[test]
fn following_selection_reuses_the_frame_and_applies_spring_presets() {
    let (mut app, _, controls) = fixture();
    let frame = app.world.find_by_id("pages_follow_frame").unwrap();
    for (preset, id) in [
        (0, "pages_follow_b"),
        (1, "pages_follow_c"),
        (2, "pages_follow_a"),
    ] {
        controls.set_spring(preset);
        let before = app.world.get::<ComputedRect>(frame).unwrap().0;
        click_id(&mut app, id);
        let target = app.world.find_by_id(id).unwrap();
        let bounds = app.world.get::<FollowBounds>(frame).unwrap();
        assert_eq!(bounds.target, target);
        assert!(bounds.spring == Some(model::spring_config(preset)));
        let target_rect = app.world.get::<ComputedRect>(target).unwrap().0;
        let expected = Rect::new(
            target_rect.x - Fixed::from_int(4),
            target_rect.y - Fixed::from_int(4),
            target_rect.w + Fixed::from_int(8),
            target_rect.h + Fixed::from_int(8),
        );
        assert_ne!(before, expected);
        assert_ne!(app.world.get::<ComputedRect>(frame).unwrap().0, expected);
        for _ in 0..160 {
            assert!(!app.tick());
        }
        assert_eq!(app.world.get::<ComputedRect>(frame).unwrap().0, expected);
        assert_eq!(app.world.find_by_id("pages_follow_frame"), Some(frame));
    }
}

#[test]
fn stack_retains_controls_and_scroll_then_releases_departed_pages() {
    let (mut app, path, controls) = fixture();
    controls.set_effect(0);
    app.render().unwrap();
    click_id(&mut app, "pages_explore");
    assert_eq!(path.len(), 1);
    let list = app.world.find_by_id("pages_collection").unwrap();
    app.world.get_mut::<ScrollOffset>(list).unwrap().y = Fixed::from_int(70);
    let saved_offset = *app.world.get::<ScrollOffset>(list).unwrap();
    path.push(Route::Place(1)).unwrap();
    app.render().unwrap();
    let switch = app.world.query::<Switch>().iter().next().unwrap().0;
    let slider = app.world.query::<Slider>().iter().next().unwrap().0;
    click(&mut app, switch);
    assert!(app.world.get::<Switch>(switch).unwrap().on);
    app.world.get_mut::<Slider>(slider).unwrap().value = Fixed::from_ratio(3, 4);
    let note_button = text_entity(&app, "Write a note  >");
    click(&mut app, note_button);
    assert_eq!(path.len(), 3);
    let note = text_entity(&app, "A note to keep.");
    click_id(&mut app, "pages_back");
    assert_eq!(path.len(), 2);
    assert!(!app.world.is_alive(note));
    assert!(app.world.get::<Switch>(switch).unwrap().on);
    assert_eq!(
        app.world.get::<Slider>(slider).unwrap().value,
        Fixed::from_ratio(3, 4)
    );
    click_id(&mut app, "pages_back");
    assert_eq!(*app.world.get::<ScrollOffset>(list).unwrap(), saved_offset);
    click_id(&mut app, "pages_home");
    assert!(path.is_empty());
    assert!(!app.world.is_alive(list));
    assert!(!app.world.is_alive(switch));
}

#[test]
fn reactive_transition_controls_apply_to_both_containers_without_restarting_motion() {
    let (mut app, path, controls) = fixture();
    let stack = app.world.find_by_id("pages_stack").unwrap();
    let page = app.world.find_by_id("pages_scene").unwrap();
    path.push(Route::Library).unwrap();
    app.render().unwrap();
    assert!(!app.tick());
    let before = transition::running(&app.world, stack).unwrap();
    controls.set_spring(2);
    controls.set_effect(2);
    app.render().unwrap();
    let expected = PageTransition::fade(crate::anim::BOUNCY);
    assert!(
        app.world
            .get::<NavigationStack<Route>>(stack)
            .unwrap()
            .transition
            == expected
    );
    assert!(app.world.get::<PageView<Scene>>(page).unwrap().transition == expected);
    let after = transition::running(&app.world, stack).unwrap();
    assert_eq!((before.from, before.to), (after.from, after.to));
    finish(&mut app);
    click_id(&mut app, "pages_cycle");
    controls.next();
    app.render().unwrap();
    finish(&mut app);
    assert!(controls.scene() == Scene::Night);
    assert!(app.world.is_alive(text_entity(&app, "Stay a little.")));
    assert!(
        !app.world
            .query::<Text>()
            .iter()
            .any(|(_, text)| { text.resolve(app.world.resource()) == "Drift away." })
    );
}
