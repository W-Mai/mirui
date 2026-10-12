use mirui::app::App;
use mirui::render::DrawCommand;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::ui::Style;
use mirui::ui::layout::LayoutStyle;
use mirui::{component, model, system, view};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[model]
struct Counter {
    #[observe]
    value: u8,
}

#[model]
impl Counter {
    fn advance(&mut self, amount: u8) {
        self.value += amount;
    }
}

#[model]
struct Step {
    #[observe]
    amount: u8,
}

#[model]
impl Step {
    fn set_amount(&mut self, amount: u8) {
        self.amount = amount;
    }
}

#[system]
fn advance(counter: model!(Counter), step: model!(Step)) {
    counter.advance(step.amount());
}

#[component]
struct Meter {
    counter: model!(Counter),
    slot: usize,
}

struct PaintProbe(Rc<Cell<[usize; 2]>>);

#[view(component = Meter)]
fn meter_view(
    meter: component!(),
    counter: model!(component.counter, watch(value)),
    probe: res!(PaintProbe),
    paint: paint!(),
) {
    let mut counts = probe.0.get();
    counts[meter.slot] += 1;
    probe.0.set(counts);
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color: Color::rgb(counter.value, 0, 0),
        radius: Fixed::ZERO,
        opa: 255,
    });
    paint.set_background_handled(true);
}

struct StaticCalls(Rc<Cell<usize>>);

#[system]
fn count_static(calls: res!(StaticCalls)) {
    calls.0.set(calls.0.get() + 1);
}

#[component]
struct Driver;

#[view(component = Driver, systems = [count_static::system()])]
fn driver_view() {}

#[component]
struct OtherDriver;

#[view(component = OtherDriver)]
fn other_driver_view() {}

#[test]
fn owned_systems_keep_each_binding_and_repaint_once_without_warm_allocations() {
    let pixels = Rc::new(Cell::new([[0_u8; 4]; 2]));
    let painted = Rc::new(Cell::new([0_usize; 2]));
    let pixels_on_flush = pixels.clone();
    let backend = FramebufSurface::new(8, 16, move |buffer, _| {
        pixels_on_flush.set(core::array::from_fn(|slot| {
            let offset = ((slot * 8 + 4) * 8 + 4) * 4;
            buffer[offset..offset + 4].try_into().unwrap()
        }));
    });
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.add_resource(PaintProbe(painted.clone()));
    let first = app.add_model(Counter { value: 0 }).finish();
    let second = app.add_model(Counter { value: 10 }).finish();
    let step = app.add_model(Step { amount: 1 }).finish();
    app.with_widget(meter_view::view().with_systems([
        advance::system(first.clone(), step.clone()),
        advance::system(second.clone(), step.clone()),
        advance::system(first.clone(), step.clone()),
    ]));
    let root = app.spawn_root().id();
    app.compose(root, |tree| {
        for (slot, counter) in [first.clone(), second.clone()].into_iter().enumerate() {
            tree.spawn((
                Style {
                    layout: LayoutStyle {
                        width: Dimension::px(8),
                        height: Dimension::px(8),
                        ..LayoutStyle::default()
                    },
                    ..Style::default()
                },
                Meter { counter, slot },
            ));
        }
    });

    for frame in 1..=2 {
        assert!(!app.tick());
        assert_eq!((first.value(), second.value()), (frame * 2, 10 + frame));
        assert_eq!(
            pixels.get(),
            [[frame * 2, 0, 0, 255], [10 + frame, 0, 0, 255]]
        );
        assert_eq!(painted.get(), [usize::from(frame); 2]);
    }

    assert_eq!(tracked_allocations(|| assert!(!app.tick())), 0);
    assert_eq!((first.value(), second.value()), (6, 13));
    assert_eq!(pixels.get(), [[6, 0, 0, 255], [13, 0, 0, 255]]);
    assert_eq!(painted.get(), [3, 3]);

    step.set_amount(0);
    assert!(!app.tick());
    assert_eq!(painted.get(), [3, 3]);
}

#[test]
fn same_source_batches_replace_the_whole_list_without_requiring_widget_entities() {
    let calls = Rc::new(Cell::new(0));
    let mut app = App::headless(8, 8);
    app.add_resource(StaticCalls(calls.clone()));
    let first = app.add_model(Counter { value: 0 }).finish();
    let second = app.add_model(Counter { value: 7 }).finish();
    let step = app.add_model(Step { amount: 3 }).finish();
    app.with_widgets([
        driver_view::view()
            .with_systems([
                advance::system(first.clone(), step.clone()),
                advance::system(second.clone(), step.clone()),
            ])
            .with_systems([advance::system(first.clone(), step.clone())]),
        other_driver_view::view().with_systems([advance::system(second.clone(), step)]),
    ]);

    assert!(!app.tick());
    assert_eq!((first.value(), second.value()), (3, 10));
    assert_eq!(calls.get(), 0);
    assert!(!app.tick());
    assert_eq!((first.value(), second.value()), (6, 13));
    assert_eq!(calls.get(), 0);
}

#[test]
fn static_templates_remain_reusable_between_apps() {
    let first_calls = Rc::new(Cell::new(0));
    let second_calls = Rc::new(Cell::new(0));
    let mut first = App::headless(8, 8);
    first.add_resource(StaticCalls(first_calls.clone()));
    first.with_widget(driver_view::view());
    let mut second = App::headless(8, 8);
    second.add_resource(StaticCalls(second_calls.clone()));
    second.with_widgets([driver_view::view(), other_driver_view::view()]);

    assert!(!first.tick());
    assert!(!second.tick());
    assert!(!second.tick());
    assert_eq!((first_calls.get(), second_calls.get()), (1, 2));
}

#[test]
fn bound_sources_reject_mismatched_models_and_target_apps_before_execution() {
    let mut first = App::headless(8, 8);
    let counter = first.add_model(Counter { value: 0 }).finish();
    let step = first.add_model(Step { amount: 3 }).finish();
    let mut second = App::headless(8, 8);
    let foreign_step = second.add_model(Step { amount: 7 }).finish();

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            advance::system(counter.clone(), foreign_step);
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            second.with_widget(
                other_driver_view::view()
                    .with_systems([advance::system(counter.clone(), step.clone())]),
            );
        }))
        .is_err()
    );
    assert_eq!(counter.value(), 0);

    first.with_widget(
        other_driver_view::view().with_systems([advance::system(counter.clone(), step)]),
    );
    assert!(!first.tick());
    assert_eq!(counter.value(), 3);
}

#[test]
fn sources_run_only_after_installation_and_do_not_extend_model_lifetime() {
    let mut app = App::headless(8, 8);
    let counter = app.add_model(Counter { value: 0 }).finish();
    let step = app.add_model(Step { amount: 1 }).finish();
    let uninstalled =
        other_driver_view::view().with_systems([advance::system(counter.clone(), step.clone())]);
    assert!(!app.tick());
    assert_eq!(counter.value(), 0);
    drop(uninstalled);
    assert!(!app.tick());
    assert_eq!(counter.value(), 0);

    let pending =
        other_driver_view::view().with_systems([advance::system(counter.clone(), step.clone())]);
    app.with_widget(
        other_driver_view::view().with_systems([advance::system(counter.clone(), step)]),
    );
    assert!(!app.tick());
    assert_eq!(counter.value(), 1);
    drop(app);
    assert!(catch_unwind(AssertUnwindSafe(|| counter.value())).is_err());
    drop(pending);
}
