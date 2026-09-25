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

#[derive(Clone, Copy)]
struct ChangeSet(u8);

impl ChangeSet {
    const NONE: Self = Self(0);
    const VISUAL: Self = Self(1);
    const PERSISTENCE: Self = Self(2);

    fn contains(self, mask: Self) -> bool {
        self.0 & mask.0 == mask.0
    }
}

#[model(change = ChangeSet, watch(visual = ChangeSet::VISUAL, persistence = ChangeSet::PERSISTENCE))]
struct VisualCounter {
    #[observe]
    active: bool,
    pixels: u32,
}

#[derive(Clone, Copy)]
struct Note(u32);

#[derive(Clone, Copy)]
struct Audit(u32);

#[model]
pub struct EffectCounter {
    #[observe]
    count: u32,
    notes: [Option<Note>; 2],
    audits: [Option<Audit>; 1],
}

#[model]
impl EffectCounter {
    fn emit(&mut self, count: u32) {
        self.count = count;
        self.notes[0] = Some(Note(count));
        self.audits[0] = Some(Audit(count));
    }

    #[effects]
    fn take_notes(&mut self) -> [Option<Note>; 2] {
        std::mem::take(&mut self.notes)
    }

    #[effects]
    fn take_audits(&mut self) -> [Option<Audit>; 1] {
        std::mem::take(&mut self.audits)
    }
}

#[model]
impl VisualCounter {
    fn change_pixels(&mut self, pixels: u32) -> ChangeSet {
        if self.pixels == pixels {
            ChangeSet::NONE
        } else {
            self.pixels = pixels;
            ChangeSet::VISUAL
        }
    }

    fn set_active(&mut self, active: bool) -> ChangeSet {
        self.active = active;
        ChangeSet::NONE
    }

    fn request_persistence(&mut self) -> ChangeSet {
        ChangeSet::PERSISTENCE
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

#[test]
fn named_revision_follows_change_mask_without_suppressing_fields() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(VisualCounter {
        active: false,
        pixels: 0,
    });
    let visual_runs = Rc::new(Cell::new(0));
    let field_runs = Rc::new(Cell::new(0));
    let visual = model.clone();
    let visual_runs_in_effect = visual_runs.clone();
    let _visual_effect = Effect::new(move || {
        let _ = visual.visual_revision();
        visual_runs_in_effect.set(visual_runs_in_effect.get() + 1);
    });
    let field = model.clone();
    let field_runs_in_effect = field_runs.clone();
    let _field_effect = Effect::new(move || {
        let _ = field.active();
        field_runs_in_effect.set(field_runs_in_effect.get() + 1);
    });

    assert_eq!(model.visual_revision(), 0);
    assert_eq!(model.persistence_revision(), 0);
    model.set_active(true);
    flush_signal_dirty(&mut app.world);
    assert_eq!((visual_runs.get(), field_runs.get()), (1, 2));
    assert_eq!(model.visual_revision(), 0);

    model.change_pixels(1);
    flush_signal_dirty(&mut app.world);
    assert_eq!((visual_runs.get(), field_runs.get()), (2, 2));
    assert_eq!(model.visual_revision(), 1);
    assert_eq!(model.persistence_revision(), 0);

    model.change_pixels(1);
    flush_signal_dirty(&mut app.world);
    assert_eq!((visual_runs.get(), field_runs.get()), (2, 2));

    model.request_persistence();
    flush_signal_dirty(&mut app.world);
    assert_eq!(model.visual_revision(), 1);
    assert_eq!(model.persistence_revision(), 1);
    assert_eq!((visual_runs.get(), field_runs.get()), (2, 2));
}

#[test]
fn named_revision_notification_reuses_its_registered_storage() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(VisualCounter {
        active: false,
        pixels: 0,
    });
    let observed = model.clone();
    let _effect = Effect::new(move || {
        let _ = observed.visual_revision();
    });
    model.change_pixels(1);
    flush_signal_dirty(&mut app.world);
    for pixels in 2..130 {
        assert_eq!(
            tracked_allocations(|| {
                model.change_pixels(pixels);
            }),
            0
        );
        flush_signal_dirty(&mut app.world);
    }
    assert_eq!(model.visual_revision(), 129);
}

#[test]
fn typed_effects_are_drained_once_after_model_borrow_is_released() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(EffectCounter {
        count: 0,
        notes: [None; 2],
        audits: [None; 1],
    });
    model.emit(1);

    let note_total = Rc::new(Cell::new(0));
    let audit_total = Rc::new(Cell::new(0));
    let note_total_in_handler = note_total.clone();
    let observed = model.clone();
    app.on_effect(&model, move |note: Note| {
        assert_eq!(observed.count(), note.0);
        note_total_in_handler.set(note_total_in_handler.get() + note.0);
    })
    .unwrap();
    let audit_total_in_handler = audit_total.clone();
    app.on_effect(&model, move |audit: Audit| {
        audit_total_in_handler.set(audit_total_in_handler.get() + audit.0);
    })
    .unwrap();
    assert_eq!(note_total.get(), 0);
    assert_eq!(audit_total.get(), 0);

    model.emit(2);
    assert_eq!(note_total.get(), 2);
    assert_eq!(audit_total.get(), 2);
    model.emit(3);
    assert_eq!(note_total.get(), 5);
    assert_eq!(audit_total.get(), 5);
    assert_eq!(
        app.on_effect(&model, |_note: Note| {}),
        Err(mirui::core::model::EffectRegistrationError::AlreadyRegistered)
    );
}

#[test]
fn registered_effect_delivery_does_not_allocate_per_command() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(EffectCounter {
        count: 0,
        notes: [None; 2],
        audits: [None; 1],
    });
    let total = Rc::new(Cell::new(0));
    let in_handler = total.clone();
    app.on_effect(&model, move |note: Note| {
        in_handler.set(in_handler.get() + note.0);
    })
    .unwrap();
    model.emit(1);
    assert_eq!(tracked_allocations(|| model.emit(2)), 0);
    assert_eq!(total.get(), 3);
}

#[test]
fn effect_callback_failure_does_not_poison_committed_model() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(EffectCounter {
        count: 0,
        notes: [None; 2],
        audits: [None; 1],
    });
    let panic_once = Rc::new(Cell::new(true));
    let panic_in_handler = panic_once.clone();
    app.on_effect(&model, move |_note: Note| {
        if panic_in_handler.replace(false) {
            panic!("effect callback failed");
        }
    })
    .unwrap();

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.emit(1)));
    assert!(failure.is_err());
    assert_eq!(model.count(), 1);
    model.emit(2);
    assert_eq!(model.count(), 2);
}

#[test]
fn effect_consumers_are_bound_to_model_instances() {
    let mut app = App::headless(32, 32);
    let first = app.add_model(EffectCounter {
        count: 0,
        notes: [None; 2],
        audits: [None; 1],
    });
    let second = app.add_model(EffectCounter {
        count: 0,
        notes: [None; 2],
        audits: [None; 1],
    });
    let first_total = Rc::new(Cell::new(0));
    let second_total = Rc::new(Cell::new(0));
    let first_in_handler = first_total.clone();
    app.on_effect(&first, move |note: Note| {
        first_in_handler.set(first_in_handler.get() + note.0);
    })
    .unwrap();
    let second_in_handler = second_total.clone();
    app.on_effect(&second, move |note: Note| {
        second_in_handler.set(second_in_handler.get() + note.0);
    })
    .unwrap();

    first.emit(3);
    second.emit(7);
    assert_eq!((first_total.get(), second_total.get()), (3, 7));
}
