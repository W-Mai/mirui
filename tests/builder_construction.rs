use std::cell::Cell;
use std::rc::Rc;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

use mirui::ecs::{Entity, World};
use mirui::input::event::{BubbleControl, gesture::GestureEvent};
use mirui::prelude::*;
use mirui::ui::property::prop;
use mirui::ui::widgets::{Button, Slider, Text};
use mirui::ui::{Children, HitTarget, InteractionFeedback, NamedId, Parent, Style, Widget};

#[model]
struct Controls {
    #[observe]
    level: Fixed,
    #[observe]
    hits: u32,
}

#[model]
impl Controls {
    fn set_level(&mut self, value: Fixed) {
        self.level = value;
    }

    fn reset(&mut self) {
        self.level = Fixed::ZERO;
        self.hits += 1;
    }
}

#[event]
fn reset(controls: model!(Controls)) {
    controls.reset();
}

#[component]
struct Mounted(Rc<Cell<u8>>);

#[mirui::event]
fn inspect_mount(
    entity: entity!(),
    mounted: query!(Mounted, Parent, Text),
    children: query!(Children),
    ids: res!(mirui::ui::IdMap),
) {
    let (mounted, parent, _) = mounted.get(entity).unwrap();
    assert!(children.get(parent.0).unwrap().0.contains(&entity));
    assert_eq!(ids.get("final"), Some(entity));
    assert_eq!(ids.get("overwritten"), None);
    let count = &mounted.0;
    count.set(count.get() + 1);
}

#[mirui::view(component = Mounted, attach = inspect_mount::callback())]
fn mounted_view() {}

#[test]
fn static_tuples_finalize_once_before_properties_and_children() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets().with_widget(mounted_view::view());
    let root = app.spawn_root().id();
    let mounts = Rc::new(Cell::new(0));
    let observed = mounts.clone();
    let (panel, child) = app.compose(root, |tree| {
        tree.children(
            (
                Slider::build(0, 100).bind_property::<prop::SliderValue>(move || {
                    assert_eq!(observed.get(), 1);
                    Fixed::from_int(17)
                }),
                NamedId("overwritten"),
                (
                    Text::label("level"),
                    Mounted(mounts.clone()),
                    NamedId("final"),
                ),
            ),
            |column| {
                assert_eq!(mounts.get(), 1);
                column.spawn(Style::default())
            },
        )
    });
    assert_eq!(mounts.get(), 1);
    assert_eq!(app.world.get::<Children>(root).unwrap().0, [panel]);
    assert_eq!(app.world.get::<Children>(panel).unwrap().0, [child]);
    assert_eq!(app.world.get::<Parent>(child).unwrap().0, panel);
    assert!(app.world.has::<Widget>(child));
    assert!(app.world.has::<Style>(child));
    assert!(app.world.get::<Children>(child).unwrap().0.is_empty());
    assert!(!app.world.has::<HitTarget>(child));
    assert!(!app.world.has::<InteractionFeedback>(child));
    assert_eq!(
        app.world.get::<Slider>(panel).unwrap().value,
        Fixed::from_int(17)
    );
}

fn tap(entity: Entity) -> GestureEvent {
    GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: entity,
    }
}

fn dispatch(world: &mut World, event: &GestureEvent, now_ms: u32) -> BubbleControl {
    mirui::__macro_support::dispatch_entity_gesture(world, event, now_ms).unwrap()
}

#[test]
fn typed_builders_and_completed_nodes_share_reactive_events() {
    let mut app = App::headless(100, 100);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let controls = app
        .add_model(Controls {
            level: Fixed::from_int(12),
            hits: 0,
        })
        .finish();
    let read = controls.clone();
    let (slider, button, node) = app.compose(root, |tree| {
        let slider = tree.spawn(
            Slider::build(0, 100)
                .bind_property::<prop::SliderValue>(move || read.level())
                .fill_color(mirui::ui::ColorToken::Primary)
                .style(Style::default()),
        );
        let button = tree.spawn((
            Button::build().on_event(Tap::EVENT, reset::callback(controls.clone())),
            Text::label("Reset"),
        ));
        let node = tree.spawn(Style::default());
        tree.on_event(node, Tap::EVENT, reset::callback(controls.clone()));
        (slider, button, node)
    });
    assert_eq!(
        app.world.get::<Slider>(slider).unwrap().value,
        Fixed::from_int(12)
    );
    controls.set_level(Fixed::from_int(37));
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Slider>(slider).unwrap().value,
        Fixed::from_int(37)
    );
    assert_eq!(
        dispatch(&mut app.world, &tap(button), 1),
        BubbleControl::Prevent
    );
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.get::<Slider>(slider).unwrap().value, Fixed::ZERO);
    dispatch(&mut app.world, &tap(node), 2);
    assert_eq!(controls.hits(), 2);

    let allocations = tracked_allocations(|| {
        for step in 1..=64 {
            controls.set_level(Fixed::from_int(step));
            mirui::core::reactive::flush_signal_dirty(&mut app.world);
            assert_eq!(
                app.world.get::<Slider>(slider).unwrap().value,
                Fixed::from_int(step)
            );
            dispatch(&mut app.world, &tap(button), step as u32 + 2);
            mirui::core::reactive::flush_signal_dirty(&mut app.world);
            assert_eq!(app.world.get::<Slider>(slider).unwrap().value, Fixed::ZERO);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(controls.hits(), 66);
}

#[test]
fn pending_properties_and_events_add_no_heap_container() {
    let mut app = App::headless(16, 16);
    let controls = app
        .add_model(Controls {
            level: Fixed::ONE,
            hits: 0,
        })
        .finish();
    let callback = reset::callback(controls.clone());
    let allocations = tracked_allocations(|| {
        let pending = Slider::build(0, 100)
            .bind_property::<prop::SliderValue>(move || controls.level())
            .on_event(Tap::EVENT, callback)
            .fill_color(mirui::ui::ColorToken::Primary)
            .style(Style::default());
        std::hint::black_box(pending);
    });
    assert_eq!(allocations, 0);
}
