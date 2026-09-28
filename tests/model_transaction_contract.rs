use mirui::app::App;
use mirui::core::reactive::{Effect, flush_signal_dirty};
use mirui::model;
use std::cell::Cell;
use std::rc::Rc;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[model]
struct TransactionCounter {
    #[observe]
    value: u32,
}

#[model]
impl TransactionCounter {
    fn add_twice(&mut self, amount: u32) {
        self.add_raw(amount);
        self.add_raw(amount);
    }

    fn add_raw(&mut self, amount: u32) {
        self.value += amount;
    }
}

#[model]
struct SnapshotFailure {
    #[observe]
    value: u32,
}

#[model]
impl SnapshotFailure {
    #[observe]
    fn checked(&self) -> u32 {
        assert_ne!(self.value, 13, "observer snapshot failed");
        self.value
    }

    fn set(&mut self, value: u32) {
        self.value = value;
    }
}

#[derive(Clone, Copy)]
struct Note;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FallibleChange(u8);

impl FallibleChange {
    const VISUAL: Self = Self(1);

    fn contains(self, mask: Self) -> bool {
        self.0 & mask.0 == mask.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rejected {
    ReservedValue,
}

#[derive(Clone, Copy)]
struct AcceptedValue(u32);

#[model(change = FallibleChange, watch(visual = FallibleChange::VISUAL))]
struct FallibleCounter {
    #[observe]
    value: u32,
    accepted: Option<AcceptedValue>,
}

#[model]
impl FallibleCounter {
    fn set_checked(&mut self, value: u32) -> Result<FallibleChange, Rejected> {
        if value == 13 {
            return Err(Rejected::ReservedValue);
        }
        self.value = value;
        self.accepted = Some(AcceptedValue(value));
        Ok(FallibleChange::VISUAL)
    }

    fn reject_with_pending_effect(&mut self) -> Result<FallibleChange, Rejected> {
        self.accepted = Some(AcceptedValue(13));
        Err(Rejected::ReservedValue)
    }

    fn panic_during_update(&mut self) -> Result<FallibleChange, Rejected> {
        panic!("fallible update failed")
    }

    #[effects]
    fn take_accepted(&mut self) -> [Option<AcceptedValue>; 1] {
        [self.accepted.take()]
    }
}

#[model]
struct ExtractionFailure {
    #[observe]
    value: u32,
    event: Option<Note>,
}

#[model]
impl ExtractionFailure {
    fn set(&mut self, value: u32) {
        self.value = value;
        self.event = Some(Note);
    }

    #[effects]
    fn take_event(&mut self) -> [Option<Note>; 1] {
        assert_ne!(self.value, 13, "effect extraction failed");
        [self.event.take()]
    }
}

#[test]
fn raw_helpers_inside_one_command_publish_once() {
    let mut app = App::headless(64, 64);
    let model = app.add_model(TransactionCounter { value: 0 });
    let runs = Rc::new(Cell::new(0));
    let observed = model.clone();
    let counted = runs.clone();
    let _effect = Effect::new(move || {
        let _ = observed.value();
        counted.set(counted.get() + 1);
    });

    model.add_twice(2);
    flush_signal_dirty(&mut app.world);

    assert_eq!(model.value(), 4);
    assert_eq!(runs.get(), 2);
}

#[test]
fn sequential_commands_on_distinct_models_keep_instance_boundaries() {
    let mut app = App::headless(64, 64);
    let first = app.add_model(TransactionCounter { value: 1 });
    let second = app.add_model(TransactionCounter { value: 10 });
    let first_runs = Rc::new(Cell::new(0));
    let second_runs = Rc::new(Cell::new(0));
    let first_observer = first.clone();
    let second_observer = second.clone();
    let first_counted = first_runs.clone();
    let second_counted = second_runs.clone();
    let _first_effect = Effect::new(move || {
        let _ = first_observer.value();
        first_counted.set(first_counted.get() + 1);
    });
    let _second_effect = Effect::new(move || {
        let _ = second_observer.value();
        second_counted.set(second_counted.get() + 1);
    });

    first.add_twice(2);
    flush_signal_dirty(&mut app.world);
    assert_eq!((first.value(), second.value()), (5, 10));
    assert_eq!((first_runs.get(), second_runs.get()), (2, 1));

    second.add_twice(3);
    flush_signal_dirty(&mut app.world);
    assert_eq!((first.value(), second.value()), (5, 16));
    assert_eq!((first_runs.get(), second_runs.get()), (2, 2));
}

#[test]
fn observer_snapshot_failure_poisoned_before_commit() {
    let mut app = App::headless(64, 64);
    let model = app.add_model(SnapshotFailure { value: 0 });

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.set(13)));
    assert!(failure.is_err());
    let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.checked()));
    assert!(next.is_err());
}

#[test]
fn effect_extraction_failure_poisoned_before_commit() {
    let mut app = App::headless(64, 64);
    let model = app.add_model(ExtractionFailure {
        value: 0,
        event: None,
    });

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.set(13)));
    assert!(failure.is_err());
    let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.value()));
    assert!(next.is_err());
}

#[test]
fn rejected_update_does_not_publish_or_leak_effects() {
    let mut app = App::headless(64, 64);
    let model = app.add_model(FallibleCounter {
        value: 1,
        accepted: None,
    });
    let observer_runs = Rc::new(Cell::new(0));
    let observed = model.clone();
    let counted = Rc::clone(&observer_runs);
    let _effect = Effect::new(move || {
        let _ = observed.value();
        counted.set(counted.get() + 1);
    });
    let accepted_total = Rc::new(Cell::new(0));
    let recorded = Rc::clone(&accepted_total);
    app.on_effect(&model, move |value: AcceptedValue| {
        recorded.set(recorded.get() + value.0);
    })
    .unwrap();

    assert_eq!(
        model.reject_with_pending_effect(),
        Err(Rejected::ReservedValue)
    );
    flush_signal_dirty(&mut app.world);
    assert_eq!(model.value(), 1);
    assert_eq!(model.visual_revision(), 0);
    assert_eq!(observer_runs.get(), 1);
    assert_eq!(accepted_total.get(), 0);

    let mut accepted = None;
    assert_eq!(
        tracked_allocations(|| accepted = Some(model.set_checked(7))),
        0
    );
    assert_eq!(accepted, Some(Ok(FallibleChange::VISUAL)));
    flush_signal_dirty(&mut app.world);
    assert_eq!(model.value(), 7);
    assert_eq!(model.visual_revision(), 1);
    assert_eq!(observer_runs.get(), 2);
    assert_eq!(accepted_total.get(), 7);
}

#[test]
fn panicking_fallible_update_poisoned_the_model() {
    let mut app = App::headless(64, 64);
    let model = app.add_model(FallibleCounter {
        value: 1,
        accepted: None,
    });

    let failure =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.panic_during_update()));
    assert!(failure.is_err());
    let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.value()));
    assert!(next.is_err());
}
