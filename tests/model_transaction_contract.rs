use mirui::app::App;
use mirui::core::reactive::{Effect, flush_signal_dirty};
use mirui::model;
use std::cell::Cell;
use std::rc::Rc;

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
