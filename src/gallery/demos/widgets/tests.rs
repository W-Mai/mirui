use super::DEFAULT_VIEW;
use super::binding::{row_binder, slider_to_progress_system};
use super::composition::build_widgets;
use super::state::{
    ACCENT, FormProgress, FormSlider, ThemeCycle, cycle, dark_with_accent, light_with_accent,
};
use crate::ecs::SystemScheduler;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::widgets::{Button, Checkbox, Image, ProgressBar, Slider, Text};
use crate::ui::{Children, IdMap, UiScope};

#[test]
fn theme_cycle_timer_captures_model_and_applies_deferred_theme_change() {
    let _guard = crate::core::time::mock::install();
    crate::core::time::mock::set_ms(0);
    let mut app = App::headless(32, 32);
    app.world
        .insert_resource(crate::ecs::MonoClock::from_time_source());
    app.world.insert_resource(dark_with_accent());
    app.add_system(crate::core::timer::timer_system::system());
    let state = app.add_model(ThemeCycle::default());
    app.add_timer(cycle::timer(state));

    app.systems.run_all(&mut app.world).unwrap();
    crate::core::time::mock::set_ms(3_000);
    app.systems.run_all(&mut app.world).unwrap();

    assert_eq!(
        app.world
            .resource::<crate::ui::Theme>()
            .unwrap()
            .resolve(ACCENT),
        light_with_accent().resolve(ACCENT)
    );
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
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    drop(cx);
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
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    drop(cx);

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

    row_binder(&mut world, row, 7);

    let text = world.get::<Text>(label).expect("bound label");
    assert_eq!(text.resolve(&world), "Row 7");
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
    app.compose(root, |cx| build_widgets(cx, 320, 480));
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
