use super::DEFAULT_VIEW;
use super::binding::slider_to_progress_system;
use super::composition::build_widgets;
use super::state::{
    ACCENT, FormProgress, FormSlider, custom_theme, dark_with_accent, light_with_accent,
};
use crate::ecs::{SystemScheduler, World};
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::dirty::VisualDirty;
use crate::ui::widgets::{
    Button, Checkbox, Image, LazyList, ProgressBar, Slider, Switch, TabBar, Text,
};

#[test]
fn timeline_ignores_other_lists_and_controls() {
    #[compose]
    fn unrelated_controls() {
        ui! {
            Column () [
                crate::ui::Hidden,
            ] {
                TabBar (count: 3) {
                    Text ("Other list")
                    Text ("Other controls")
                    Text ("Other theme")
                }
                Switch ()
                Slider ()
                LazyList ()
            }
        };
    }

    let mut app = App::headless(320, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, unrelated_controls());
    assert!(app.compose(root, super::build_sim_timeline()).is_none());
    super::install(&mut app, root, true);
    assert!(
        app.world
            .resource::<crate::input::event::sim::SimTimeline>()
            .unwrap()
            .is_running()
    );

    let _guard = crate::core::time::mock::install();
    crate::core::time::mock::set_ms(0);
    app.add_resource(MonoClock::from_time_source());
    crate::ui::render_system::update_layout(
        &mut app.world,
        root,
        &crate::types::Viewport::new(320, 480, Fixed::ONE),
    );
    let mut scheduler = SystemScheduler::new();
    scheduler.add(crate::input::event::sim::sim_timeline_system::system());
    for now in (0..=1_000).step_by(50) {
        crate::core::time::mock::set_ms(now);
        scheduler.run_all(&mut app.world).unwrap();
    }
    let tabs = app.world.find_by_id("widgets_tabs").unwrap();
    assert_eq!(app.world.get::<TabBar>(tabs).unwrap().selected, 1);
}

#[test]
fn installed_workbench_cycles_themes_without_autoplay() {
    let _guard = crate::core::time::mock::install();
    crate::core::time::mock::set_ms(0);
    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::from_time_source());
    let root = app.spawn_root().id();
    super::install(&mut app, root, false);

    app.systems.run_all(&mut app.world).unwrap();
    for (now, expected) in [
        (0, dark_with_accent()),
        (3_000, light_with_accent()),
        (6_000, custom_theme()),
        (9_000, dark_with_accent()),
    ] {
        crate::core::time::mock::set_ms(now);
        app.systems.run_all(&mut app.world).unwrap();
        assert_eq!(
            app.world.resource::<Theme>().unwrap().resolve(ACCENT),
            expected.resolve(ACCENT)
        );
        assert!(
            app.world
                .resource::<crate::input::event::sim::SimTimeline>()
                .is_none()
        );
    }
}

#[test]
fn slider_progress_sync_reuses_stable_traversal() {
    let mut world = World::new();
    let slider = world.spawn_empty();
    world.insert(slider, FormSlider);
    let mut control = Slider::new(Fixed::ZERO, Fixed::from_int(100));
    control.value = Fixed::from_int(37);
    world.insert(slider, control);

    let progress = world.spawn_empty();
    world.insert(progress, FormProgress);
    world.insert(progress, ProgressBar::default());

    let mut scheduler = SystemScheduler::new();
    scheduler.add(slider_to_progress_system::system());
    scheduler.run_all(&mut world).unwrap();

    assert!((world.get::<ProgressBar>(progress).unwrap().value - 0.37).abs() < 0.001);
    assert!(world.get::<VisualDirty>(progress).is_some());

    world.remove::<VisualDirty>(progress);
    scheduler.run_all(&mut world).unwrap();
    assert!(!world.has::<VisualDirty>(progress));
}

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let world = app.world;
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
    assert_eq!(world.query::<Button>().collect().len(), 2);
    assert_eq!(world.query::<Checkbox>().collect().len(), 2);
    assert_eq!(world.query::<Image>().collect().len(), 1);
    assert!(!world.query::<ProgressBar>().collect().is_empty());
}

#[test]
fn list_binding_updates_content_without_dropping_paragraph_style() {
    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets().with_default_systems();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    crate::ui::render_system::update_layout(
        &mut app.world,
        parent,
        &crate::types::Viewport::new(DEFAULT_VIEW.0, DEFAULT_VIEW.1, Fixed::ONE),
    );
    let mut world = app.world;

    let list = world.find_by_id("widgets_list").expect("widgets list id");
    let row = world
        .get::<Children>(list)
        .and_then(|children| children.0.first().copied())
        .expect("pooled row");
    let label = world
        .get::<Children>(row)
        .and_then(|children| children.0.first().copied())
        .expect("row label");
    let paragraph = world
        .get::<Text>(label)
        .expect("label text")
        .paragraph()
        .clone();

    let mut scheduler = SystemScheduler::new();
    scheduler.add(crate::ui::widgets::lazy_list::lazy_list_system::system());
    scheduler.run_all(&mut world).unwrap();

    let text = world.get::<Text>(label).expect("bound label");
    assert_eq!(text.resolve(world.resource()), "Row 0");
    assert_eq!(text.paragraph(), &paragraph);
}

#[test]
fn portrait_layout_keeps_the_workbench_inside_the_viewport() {
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    let mut app = App::headless(320, 480);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets());
    app.set_root(root);
    update_layout(&mut app.world, root, &Viewport::new(320, 480, Fixed::ONE));

    let tabs = app.world.find_by_id("widgets_tabs").expect("tabs id");
    let list = app.world.find_by_id("widgets_list").expect("list id");
    let tabs_rect = app.world.get::<ComputedRect>(tabs).expect("tabs rect").0;
    let list_rect = app.world.get::<ComputedRect>(list).expect("list rect").0;

    assert_eq!(tabs_rect.x.to_int(), 16);
    assert_eq!(tabs_rect.w.to_int(), 288);
    assert_eq!(list_rect.x.to_int(), 16);
    assert_eq!(list_rect.w.to_int(), 288);
    assert!(list_rect.y + list_rect.h <= Fixed::from_int(464));
}
