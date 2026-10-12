use mirui::{app::App, model};
use std::{cell::Cell, rc::Rc};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

type Result<T> = core::result::Result<T, mirui::core::model::EffectRegistrationError>;

struct Sound {
    text: String,
    drops: Rc<Cell<usize>>,
}

impl Drop for Sound {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct BatchSound(Sound);

#[model]
struct Player {
    #[observe]
    count: usize,
    sound: Option<Sound>,
    batch: [Option<BatchSound>; 3],
}

#[model]
impl Player {
    fn emit(&mut self, sound: Option<Sound>, batch: [Option<BatchSound>; 3]) {
        self.count += usize::from(sound.is_some()) + batch.iter().flatten().count();
        self.sound = sound;
        self.batch = batch;
    }

    #[effects]
    fn take_sound(&mut self) -> Option<Sound> {
        self.sound.take()
    }

    #[effects]
    fn take_batch(&mut self) -> [Option<BatchSound>; 3] {
        core::mem::take(&mut self.batch)
    }
}

fn empty_player() -> Player {
    Player {
        count: 0,
        sound: None,
        batch: [None, None, None],
    }
}

#[test]
fn single_and_batch_effects_borrow_noncopy_payloads_after_state_publication() {
    let mut app = App::headless(1, 1);
    let player = app.add_model(empty_player()).finish();
    let drops = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let source = player.clone();
    let received = calls.clone();
    let registration: Result<()> = player.on_effect(move |sound: &Sound| {
        assert_eq!(source.count(), 3);
        assert_eq!(sound.text, "one");
        assert_eq!(received.replace(1), 0);
    });
    registration.unwrap();
    let source = player.clone();
    let received = calls.clone();
    player
        .on_effect(move |sound: &BatchSound| {
            assert_eq!(source.count(), 3);
            let call = received.get();
            assert_eq!(sound.0.text, if call == 1 { "two" } else { "three" });
            received.set(call + 1);
        })
        .unwrap();
    let payload = |text: &str| Sound {
        text: text.into(),
        drops: drops.clone(),
    };
    let single = payload("one");
    let batch = [
        Some(BatchSound(payload("two"))),
        None,
        Some(BatchSound(payload("three"))),
    ];
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            player.emit(Some(single), batch);
            player.emit(None, [None, None, None]);
        }),
        0
    );
    assert_eq!(calls.get(), 3);
    assert_eq!(drops.get(), 3);
}

#[test]
fn unhandled_optional_effect_is_dropped_and_not_replayed_on_registration() {
    let mut app = App::headless(1, 1);
    let player = app.add_model(empty_player()).finish();
    let drops = Rc::new(Cell::new(0));
    player.emit(
        Some(Sound {
            text: "unhandled".into(),
            drops: drops.clone(),
        }),
        [None, None, None],
    );
    assert_eq!(drops.get(), 1);
    player
        .on_effect(|_: &Sound| panic!("unhandled event was retained"))
        .unwrap();
    player.emit(None, [None, None, None]);
}
