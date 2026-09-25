use mirui::app::App;
use mirui::core::model::{BindType, SharedValue};
use mirui::core::reactive::{Effect, flush_signal_dirty};
use mirui::model;
use std::cell::Cell;
use std::rc::Rc;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[model]
struct Counter {
    value: u32,
}

#[model]
impl Counter {
    fn add(&mut self, amount: u32) {
        self.value += amount;
    }

    fn value(&self) -> u32 {
        self.value
    }
}

#[model]
struct GenericValue<T: Clone + 'static> {
    value: T,
}

#[model]
impl<T: Clone + 'static> GenericValue<T> {
    fn replace(&mut self, value: T) {
        self.value = value;
    }

    fn value(&self) -> T {
        self.value.clone()
    }
}

#[model]
struct WhereValue<T>
where
    T: Copy + 'static,
{
    value: T,
}

#[model]
impl<T> WhereValue<T>
where
    T: Copy + 'static,
{
    fn replace(&mut self, value: T) {
        self.value = value;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Idle,
    Running,
}

#[model]
pub struct ObservedCounter {
    #[observe]
    count: u32,
    #[observe]
    mode: Mode,
    untouched: u32,
}

#[model]
impl ObservedCounter {
    #[observe]
    fn is_even(&self) -> bool {
        self.count % 2 == 0
    }

    fn set_count(&mut self, count: u32) {
        self.count = count;
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    fn set_untouched(&mut self, untouched: u32) {
        self.untouched = untouched;
    }
}

#[model]
struct GenericObserved<T: Copy + Eq + 'static> {
    #[observe]
    value: T,
}

#[model]
impl<T: Copy + Eq + 'static> GenericObserved<T> {
    #[observe]
    fn derived(&self) -> T {
        self.value
    }

    fn replace(&mut self, value: T) {
        self.value = value;
    }
}

#[test]
fn generated_methods_target_the_registered_instance() {
    let mut app = App::headless(32, 32);
    let left = app.add_model(Counter { value: 1 });
    let right = app.add_model(Counter { value: 10 });
    let same_left = left.clone();

    left.add(2);
    same_left.add(3);
    right.add(5);

    assert_eq!(left.value(), 6);
    assert_eq!(same_left.value(), 6);
    assert_eq!(right.value(), 15);
}

#[test]
fn generic_model_methods_keep_their_type_arguments() {
    let mut app = App::headless(32, 32);
    let value = app.add_model(GenericValue { value: 3_u16 });
    value.replace(9);
    assert_eq!(value.value(), 9);

    let where_value = app.add_model(WhereValue { value: 1_u8 });
    where_value.replace(2);
}

#[test]
fn registered_method_calls_do_not_allocate_after_registration() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 });
    let allocations = tracked_allocations(|| {
        for _ in 0..20_000 {
            counter.add(1);
            let _ = counter.value();
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(counter.value(), 20_000);
}

#[test]
fn binding_types_share_handles_without_cloning_models() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 4 });
    let allocations = tracked_allocations(|| {
        let bound: <Counter as BindType>::Shared = counter.share();
        let optional: <Option<Counter> as BindType>::Shared = Some(bound.share());
        optional.as_ref().unwrap().add(2);
    });

    assert_eq!(allocations, 0);
    assert_eq!(counter.value(), 6);
}

#[test]
fn observed_fields_notify_only_on_their_own_changes() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(ObservedCounter {
        count: 1,
        mode: Mode::Idle,
        untouched: 0,
    });
    let seen = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let observed = model.clone();
    let seen_in_effect = seen.clone();
    let runs_in_effect = runs.clone();
    let _effect = Effect::new(move || {
        seen_in_effect.set(observed.count());
        runs_in_effect.set(runs_in_effect.get() + 1);
    });
    assert_eq!(seen.get(), 1);
    assert_eq!(runs.get(), 1);

    model.set_untouched(9);
    model.set_mode(Mode::Running);
    model.set_count(1);
    flush_signal_dirty(&mut app.world);
    assert_eq!(runs.get(), 1);
    assert_eq!(model.mode(), Mode::Running);

    model.set_count(2);
    flush_signal_dirty(&mut app.world);
    assert_eq!(seen.get(), 2);
    assert_eq!(runs.get(), 2);
}

#[test]
fn generic_observed_fields_keep_their_value_type() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(GenericObserved { value: 3_u16 });
    assert_eq!(model.value(), 3);
    assert_eq!(model.derived(), 3);
    model.replace(7);
    assert_eq!(model.value(), 7);
    assert_eq!(model.derived(), 7);
}

#[test]
fn observed_update_and_notification_do_not_allocate_after_registration() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(ObservedCounter {
        count: 0,
        mode: Mode::Idle,
        untouched: 0,
    });
    let observer = model.clone();
    let _effect = Effect::new(move || {
        let _ = observer.count();
    });
    model.set_count(1);
    flush_signal_dirty(&mut app.world);
    for count in 2..130 {
        assert_eq!(tracked_allocations(|| model.set_count(count)), 0);
        flush_signal_dirty(&mut app.world);
    }
    assert_eq!(model.count(), 129);
}

#[test]
fn same_type_model_instances_keep_observers_separate() {
    let mut app = App::headless(32, 32);
    let first = app.add_model(ObservedCounter {
        count: 1,
        mode: Mode::Idle,
        untouched: 0,
    });
    let second = app.add_model(ObservedCounter {
        count: 7,
        mode: Mode::Idle,
        untouched: 0,
    });
    let first_runs = Rc::new(Cell::new(0));
    let second_runs = Rc::new(Cell::new(0));
    let first_observer = first.clone();
    let first_runs_in_effect = first_runs.clone();
    let _first_effect = Effect::new(move || {
        let _ = first_observer.count();
        first_runs_in_effect.set(first_runs_in_effect.get() + 1);
    });
    let second_observer = second.clone();
    let second_runs_in_effect = second_runs.clone();
    let _second_effect = Effect::new(move || {
        let _ = second_observer.count();
        second_runs_in_effect.set(second_runs_in_effect.get() + 1);
    });

    first.set_count(2);
    flush_signal_dirty(&mut app.world);
    assert_eq!((first_runs.get(), second_runs.get()), (2, 1));

    second.set_count(8);
    flush_signal_dirty(&mut app.world);
    assert_eq!((first_runs.get(), second_runs.get()), (2, 2));
}

#[test]
fn derived_observer_only_notifies_when_its_result_changes() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(ObservedCounter {
        count: 1,
        mode: Mode::Idle,
        untouched: 0,
    });
    let seen = Rc::new(Cell::new(false));
    let runs = Rc::new(Cell::new(0));
    let observed = model.clone();
    let seen_in_effect = seen.clone();
    let runs_in_effect = runs.clone();
    let _effect = Effect::new(move || {
        seen_in_effect.set(observed.is_even());
        runs_in_effect.set(runs_in_effect.get() + 1);
    });
    assert_eq!((seen.get(), runs.get()), (false, 1));

    model.set_count(3);
    flush_signal_dirty(&mut app.world);
    assert_eq!((seen.get(), runs.get()), (false, 1));

    model.set_count(4);
    flush_signal_dirty(&mut app.world);
    assert_eq!((seen.get(), runs.get()), (true, 2));
}
