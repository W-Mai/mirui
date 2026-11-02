use mirui::prelude::*;
use std::{cell::Cell, rc::Rc};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[test]
fn static_inputs_add_no_framework_allocations() {
    let allocations = tracked_allocations(|| {
        let value = Prop::from(42_u32);
        let forwarded = value.clone();
        assert_eq!(forwarded.get(), 42);
        assert!(!Prop::<bool>::default().get());
        assert_eq!(Prop::from(ColorToken::Primary).get(), ColorToken::Primary);
    });
    assert_eq!(allocations, 0);
    assert_eq!(Prop::<String>::from("Nearby").get(), "Nearby");
}

#[test]
fn getters_are_lazy_and_forwarding_shares_one_allocation() {
    let calls = Rc::new(Cell::new(0));
    let input_calls = calls.clone();
    let mut input = None;
    let allocations = tracked_allocations(|| {
        input = Some(Prop::new(move || {
            input_calls.set(input_calls.get() + 1);
            input_calls.get()
        }));
    });
    assert_eq!(allocations, 1);
    assert_eq!(calls.get(), 0);
    let input = input.unwrap();
    let allocations = tracked_allocations(|| {
        let first = input.clone();
        let second = first.clone();
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
    });
    assert_eq!(allocations, 0);
    assert_eq!(calls.get(), 2);
}

#[test]
fn the_last_forwarded_input_releases_its_captures() {
    let captured = Rc::new(Cell::new(7));
    let retired = Rc::downgrade(&captured);
    let input = Prop::new(move || captured.get());
    let forwarded = input.clone();
    drop(input);
    assert!(retired.upgrade().is_some());
    assert_eq!(forwarded.get(), 7);
    drop(forwarded);
    assert!(retired.upgrade().is_none());
}
