use mirui::prelude::*;
use std::cell::Cell;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

struct Retained<'a>(&'a Cell<u8>);
struct Executed;

impl Drop for Retained<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[compose]
fn deferred(_input: Retained<'_>, entered: &Cell<bool>) -> u8 {
    res!().insert(Executed);
    entered.set(true);
    7
}

#[compose]
fn passthrough<T>(value: T) -> T {
    value
}

#[compose]
fn patterned<const N: usize>((mut value, add): (u8, bool), ref bytes: [u8; N]) -> (u8, u8) {
    if add {
        value += 1;
    }
    (value, bytes[0])
}

#[compose]
fn checked(value: Result<u8, &'static str>) -> Result<u8, &'static str> {
    let value = value?;
    if value == 0 {
        return Ok(0);
    }
    Ok(value + 1)
}

#[compose]
fn borrowed<'a>(value: &'a str) -> &'a str {
    value
}

#[compose]
fn conditional(
    #[cfg(any())] _absent: MissingType,
    #[cfg(test)] value: u8,
    #[cfg_attr(test, cfg(any()), allow(unused_variables))] _removed: MissingType,
    #[cfg_attr(test, cfg_attr(test, cfg(any())))] _nested: MissingType,
    #[cfg_attr(test, allow(unused_variables))] _kept: u8,
) -> u8 {
    value
}

struct Panel<T>(T);

impl<T> Panel<T> {
    #[compose]
    fn value(value: Self) -> T {
        value.0
    }
}

struct Badge(u8);

impl Badge {
    #[compose]
    fn value(value: Self) -> u8 {
        Self::decode(value)
    }

    fn decode(value: Self) -> u8 {
        value.0
    }
}

#[test]
fn preparation_retains_unused_arguments_until_execution_or_drop() {
    let entered = Cell::new(false);
    let drops = Cell::new(0);
    let composition = deferred(Retained(&drops), &entered);
    assert!(!entered.get());
    assert_eq!(drops.get(), 0);

    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, composition), 7);
    assert!(entered.get());
    assert_eq!(drops.get(), 1);
    assert!(app.world.resource::<Executed>().is_some());

    entered.set(false);
    let abandoned = deferred(Retained(&drops), &entered);
    assert_eq!(drops.get(), 1);
    drop(abandoned);
    assert!(!entered.get());
    assert_eq!(drops.get(), 2);
}

#[test]
fn parameter_patterns_generics_and_control_flow_keep_their_results() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, patterned((4, true), [8, 9])), (5, 8));
    assert_eq!(app.compose(root, checked(Ok(0))), Ok(0));
    assert_eq!(app.compose(root, checked(Ok(4))), Ok(5));
    assert_eq!(app.compose(root, checked(Err("input"))), Err("input"));
    let moved = String::from("owned");
    assert_eq!(app.compose(root, passthrough(moved)), "owned");
}

#[test]
fn outputs_can_borrow_arguments_but_not_the_construction_scope() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    let source = String::from("input");
    let output = app.compose(root, borrowed(&source));
    assert_eq!(output.as_ptr(), source.as_ptr());
    app.compose(root, passthrough(1_u8));
    assert_eq!(output, "input");
}

#[test]
fn preparing_and_executing_warm_factories_add_no_heap_allocations() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    app.compose(root, passthrough(0_u8));
    let allocations = tracked_allocations(|| {
        for value in 0..64_u8 {
            assert_eq!(app.compose(root, passthrough(value)), value);
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn execution_retains_the_authored_type_scope() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, Panel::value(Panel(9_u8))), 9);
    assert_eq!(app.compose(root, Badge::value(Badge(4))), 4);
}

#[test]
fn conditional_parameters_and_forwarded_arguments_have_the_same_presence() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, conditional(7, 9)), 7);
}
