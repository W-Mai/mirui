//! `timer!` macro shapes. Each one installs the produced struct into
//! a fresh World and checks the resulting `Timer` carries the right
//! lifecycle variant — that's the only contract the sugar promises.
//!
//! Run via `cargo xtask test` (passes `--features std` to expose the
//! mock clock); plain `cargo test --workspace` will compile-skip it.

#![cfg(feature = "std")]

use mirui::core::timer::{Timer, TimerMode};
use mirui::ecs::World;
use mirui::ecs::time::{MonoClock, mock};
use mirui_macros::timer;

fn nop(_world: &mut World, _entity: mirui::ecs::Entity) {}

timer!(MyAfter, after: 100, |w, e| nop(w, e));
timer!(MyEvery, every: 250, |w, e| nop(w, e));
timer!(MyRepeat, repeat: 3 every: 50, |w, e| nop(w, e));
timer!(MyUntil, until: 999 every: 30, |w, e| nop(w, e));

fn fresh() -> World {
    let mut w = World::new();
    w.insert_resource(MonoClock::new(mock::clock_fn));
    w
}

#[test]
fn after_schedule_parses() {
    let _g = mock::lock();
    let mut w = fresh();
    let e = MyAfter::install(&mut w);
    let t = w.get::<Timer>(e).expect("Timer installed");
    assert_eq!(t.mode(), TimerMode::After);
}

#[test]
fn every_schedule_parses() {
    let _g = mock::lock();
    let mut w = fresh();
    let e = MyEvery::install(&mut w);
    assert_eq!(w.get::<Timer>(e).unwrap().mode(), TimerMode::Every);
}

#[test]
fn repeat_schedule_carries_count() {
    let _g = mock::lock();
    let mut w = fresh();
    let e = MyRepeat::install(&mut w);
    assert_eq!(
        w.get::<Timer>(e).unwrap().mode(),
        TimerMode::Repeat { remaining: 3 },
    );
}

#[test]
fn until_schedule_carries_deadline() {
    let _g = mock::lock();
    let mut w = fresh();
    let e = MyUntil::install(&mut w);
    assert_eq!(
        w.get::<Timer>(e).unwrap().mode(),
        TimerMode::Until { deadline_ms: 999 },
    );
}
