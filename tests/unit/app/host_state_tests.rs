use super::*;
use crate::input::event::contract::Event;
use crate::input::event::lifecycle::{HostFocusChanged, HostVisibilityChanged};

#[derive(Default)]
struct Count(u32);

#[crate::event]
fn notice(count: res!(mut Count)) {
    count.0 += 1;
}

#[test]
fn prepared_host_notifications_reuse_storage() {
    let mut app = App::headless(8, 8);
    app.add_resource(Count::default());
    app.on_event(HostFocusChanged::EVENT, notice::callback())
        .on_event(HostVisibilityChanged::EVENT, notice::callback());
    app.notify_raw_input(&InputEvent::HostFocus { focused: false });
    app.notify_raw_input(&InputEvent::HostVisibility { visible: false });
    app.world.resource_mut::<Count>().unwrap().0 = 0;
    let allocations = crate::test_support::tracked_allocations(|| {
        for index in 0..64 {
            app.notify_raw_input(&InputEvent::HostFocus {
                focused: index % 2 == 0,
            });
            app.notify_raw_input(&InputEvent::HostVisibility {
                visible: index % 2 == 0,
            });
        }
    });
    assert_eq!(app.world.resource::<Count>().unwrap().0, 128);
    assert_eq!(allocations, 0);
}
