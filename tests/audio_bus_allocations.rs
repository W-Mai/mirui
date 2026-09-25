#![cfg(feature = "audio")]

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::audio::{AudioBus, CueId};
use tracking_allocator::tracked_allocations;

#[test]
fn control_and_queue_updates_do_not_allocate() {
    let mut empty = AudioBus::<0>::new();
    let mut single = AudioBus::<1>::new();
    let mut bounded = AudioBus::<4>::new();
    let cue = CueId::new(1);

    let allocations = tracked_allocations(|| {
        for step in 0..20_000 {
            let muted = step % 2 == 0;
            assert!(!empty.set_muted(muted));
            assert!(!empty.set_master_gain(step as u8));
            single.set_muted(muted);
            single.set_master_gain(step as u8);
            bounded.play(cue);
            bounded.set_muted(muted);
            bounded.set_master_gain(step as u8);
        }
    });

    assert_eq!(allocations, 0);
    assert!(!empty.is_muted());
    assert_eq!(empty.master_gain(), 220);
    assert_eq!(bounded.master_gain(), 19_999u32 as u8);
}
