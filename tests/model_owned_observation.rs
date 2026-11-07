use mirui::app::App;
use mirui::core::reactive::{Effect, flush_signal_dirty};
use mirui::model;
use std::{cell::Cell, rc::Rc};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

#[model]
struct Document {
    #[observe]
    title: String,
    #[observe]
    bytes: Vec<u8>,
    #[observe]
    count: u32,
    notice: Option<Changed>,
}

#[derive(Clone, Copy)]
struct Changed;

#[model]
impl Document {
    fn title_to(&mut self, title: &str) {
        self.title.clear();
        self.title.push_str(title);
    }

    fn reject(&mut self, title: &str) -> Result<(), ()> {
        self.title_to(title);
        self.notice = Some(Changed);
        Err(())
    }

    fn change_back(&mut self) {
        self.title.push('!');
        self.title.pop();
    }

    fn increment(&mut self) {
        self.count += 1;
    }

    fn announce(&mut self) {
        self.notice = Some(Changed);
    }

    fn replace_bytes(&mut self, bytes: Vec<u8>) {
        self.bytes = bytes;
    }

    #[observe]
    fn caption(&self) -> String {
        format!("{}: {}", self.title, self.count)
    }

    #[effects]
    fn take_notice(&mut self) -> Option<Changed> {
        self.notice.take()
    }
}

fn document() -> Document {
    Document {
        title: "one".into(),
        bytes: vec![1, 2],
        count: 0,
        notice: None,
    }
}

#[test]
fn owned_fields_and_derived_values_publish_exact_final_changes() {
    let mut app = App::headless(64, 64);
    let document = app.add_model(document()).finish();
    let title_runs = Rc::new(Cell::new(0));
    let caption_runs = Rc::new(Cell::new(0));
    let bytes_runs = Rc::new(Cell::new(0));
    let title = document.clone();
    let counted = title_runs.clone();
    let _title = Effect::new(move || {
        assert!(!title.title().is_empty());
        counted.set(counted.get() + 1);
    });
    let caption = document.clone();
    let counted = caption_runs.clone();
    let _caption = Effect::new(move || {
        assert!(!caption.caption().is_empty());
        counted.set(counted.get() + 1);
    });
    let bytes = document.clone();
    let counted = bytes_runs.clone();
    let _bytes = Effect::new(move || {
        assert!(!bytes.bytes().is_empty());
        counted.set(counted.get() + 1);
    });

    document.title_to("one");
    document.change_back();
    document.replace_bytes(vec![1, 2]);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (title_runs.get(), caption_runs.get(), bytes_runs.get()),
        (1, 1, 1)
    );

    document.title_to("two");
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (title_runs.get(), caption_runs.get(), bytes_runs.get()),
        (2, 2, 1)
    );
    document.increment();
    document.replace_bytes(vec![3]);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (title_runs.get(), caption_runs.get(), bytes_runs.get()),
        (2, 3, 2)
    );
}

#[test]
fn rejected_owned_changes_advance_the_baseline_without_later_notifications() {
    let mut app = App::headless(64, 64);
    let document = app.add_model(document()).finish();
    let runs = Rc::new(Cell::new(0));
    let observed = document.clone();
    let counted = runs.clone();
    let _effect = Effect::new(move || {
        let _ = observed.title();
        counted.set(counted.get() + 1);
    });
    let notices = Rc::new(Cell::new(0));
    let counted = notices.clone();
    document
        .on_effect(move |_: &Changed| counted.set(counted.get() + 1))
        .unwrap();

    assert_eq!(document.reject("rejected"), Err(()));
    flush_signal_dirty(&mut app.world);
    assert_eq!(document.title(), "rejected");
    assert_eq!((runs.get(), notices.get()), (1, 0));

    document.increment();
    flush_signal_dirty(&mut app.world);
    assert_eq!((runs.get(), notices.get()), (1, 0));
    document.title_to("accepted");
    flush_signal_dirty(&mut app.world);
    assert_eq!((runs.get(), notices.get()), (2, 0));
}

struct TrackedText {
    text: String,
    copies: Rc<Cell<u32>>,
}

impl Clone for TrackedText {
    fn clone(&self) -> Self {
        self.copies.set(self.copies.get() + 1);
        Self {
            text: self.text.clone(),
            copies: self.copies.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        source.copies.set(source.copies.get() + 1);
        self.text.clone_from(&source.text);
        self.copies.clone_from(&source.copies);
    }
}

impl PartialEq for TrackedText {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}
impl Eq for TrackedText {}

#[model]
struct Tracked {
    #[observe]
    value: TrackedText,
    #[observe]
    count: u32,
}

#[model]
impl Tracked {
    fn increment(&mut self) {
        self.count += 1;
    }
    fn change(&mut self, text: &str) {
        self.value.text.clear();
        self.value.text.push_str(text);
    }
}

#[test]
fn unchanged_owned_fields_are_not_cloned_and_changed_storage_is_reused() {
    let mut app = App::headless(64, 64);
    let copies = Rc::new(Cell::new(0));
    let model = app
        .add_model(Tracked {
            value: TrackedText {
                text: "initial".into(),
                copies: copies.clone(),
            },
            count: 0,
        })
        .finish();
    let initial = copies.get();
    model.increment();
    model.change("initial");
    assert_eq!(copies.get(), initial);

    model.change("a much longer title to warm the two string capacities");
    assert_eq!(copies.get(), initial + 1);
    let allocations = tracking_allocator::tracked_allocations(|| {
        for _ in 0..8 {
            model.change("short");
            model.change("another short title");
            model.increment();
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(copies.get(), initial + 17);
}

#[model]
struct Folder {
    document: mirui::model!(owned Document),
    #[observe]
    generation: u32,
}

#[model]
impl Folder {
    fn reject(&mut self) -> Result<(), ()> {
        self.document.title_to("not published");
        Err(())
    }

    fn rename(&mut self, title: &str) {
        self.document.title_to(title);
        self.document.announce();
        self.generation += 1;
    }
}

#[test]
fn owned_child_observations_share_commit_failure_and_effect_order() {
    let mut app = App::headless(64, 64);
    let folder = app
        .add_model(Folder {
            document: document().into(),
            generation: 0,
        })
        .finish();
    let document = folder.document();
    let runs = Rc::new(Cell::new(0));
    let observed = document.clone();
    let counted = runs.clone();
    let _effect = Effect::new(move || {
        let _ = observed.title();
        counted.set(counted.get() + 1);
    });
    assert_eq!(folder.reject(), Err(()));
    flush_signal_dirty(&mut app.world);
    assert_eq!(runs.get(), 1);

    let delivered = Rc::new(Cell::new(0));
    let counted = delivered.clone();
    let parent = folder.clone();
    let child = document.clone();
    document
        .on_effect(move |_: &Changed| {
            assert_eq!(parent.generation(), 1);
            assert_eq!(child.title(), "accepted");
            counted.set(counted.get() + 1);
        })
        .unwrap();
    folder.rename("accepted");
    flush_signal_dirty(&mut app.world);
    assert_eq!((runs.get(), delivered.get()), (2, 1));
}

#[model]
struct GenericValue<T: Clone + Eq + 'static> {
    #[observe]
    value: T,
}

#[model]
impl<T: Clone + Eq + 'static> GenericValue<T> {
    fn replace(&mut self, value: T) {
        self.value = value;
    }
}

#[test]
fn generic_observed_values_keep_owned_and_registered_accessors() {
    let owned: GenericValueOwned<String> = GenericValue {
        value: "first".into(),
    }
    .into();
    assert_eq!(owned.value(), "first");
    let mut app = App::headless(64, 64);
    let model = app
        .add_model(GenericValue {
            value: String::from("first"),
        })
        .finish();
    model.replace("next".into());
    assert_eq!(model.value(), "next");
}
