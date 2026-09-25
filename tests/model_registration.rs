use mirui::app::App;
use mirui::core::model::{BindType, SharedValue};
use mirui::model;

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
