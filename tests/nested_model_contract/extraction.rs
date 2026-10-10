use super::*;

#[model]
struct Finalizer {
    child: model!(owned Counter),
    #[observe]
    version: u16,
    finish: bool,
}

#[model]
impl Finalizer {
    fn run(&mut self) {
        self.child.emit(9);
        self.finish = true;
    }

    #[effects]
    fn take_completion(&mut self) -> [Option<Note>; 1] {
        if !core::mem::take(&mut self.finish) {
            return [None];
        }
        self.version += 1;
        self.child = Counter::new(1).into();
        [Some(Note(self.version))]
    }
}

#[test]
fn extraction_finishes_before_after_snapshots_and_replacement_detection() {
    let mut app = App::headless(16, 16);
    let root = app.add_model(Finalizer {
        child: Counter::new(1).into(),
        version: 0,
        finish: false,
    });
    let child = root.child();
    let observed = root.clone();
    let (_effect, runs) = count_reads(move || {
        let _ = observed.version();
    });
    let child_calls = Rc::new(Cell::new(0));
    let recorded = Rc::clone(&child_calls);
    child
        .on_effect(move |_note: &Note| recorded.set(recorded.get() + 1))
        .unwrap();
    let seen = Rc::new(Cell::new(None));
    let recorded = Rc::clone(&seen);
    let final_state = root.clone();
    root.on_effect(move |note: &Note| {
        recorded.set(Some((
            note.0,
            final_state.version(),
            final_state.child().value(),
        )));
    })
    .unwrap();

    root.run();
    flush_signal_dirty(&mut app.world);
    assert_eq!(seen.get(), Some((1, 1, 1)));
    assert_eq!(runs.get(), 2);
    assert_eq!(child_calls.get(), 0);
    assert_eq!((child.visual_revision(), child.save_revision()), (1, 1));
}
