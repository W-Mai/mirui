#![deny(warnings)]

use mirui::ecs::{System, SystemScheduler};

fn main() {
    let descriptors: [System; 10] = [
        mirui::core::timer::timer_system::system(),
        mirui::ui::state::hover_system::system(),
        mirui::ui::state::press_system::system(),
        mirui::ui::widgets::lazy_list::lazy_list_system::system(),
        mirui::ui::widgets::tab_pages::tab_pages_system::system(),
        mirui::input::event::sim::sim_input_system::system(),
        mirui::input::event::sim::sim_timeline_system::system(),
        mirui::input::event::scroll::system::scroll_inertia_system::system(),
        mirui::input::feedback::cursor::cursor_feedback_system::system(),
        mirui::input::event::scroll::scroll_inertia_system::system(),
    ];
    let mut scheduler = SystemScheduler::new();
    for descriptor in descriptors {
        scheduler.add(descriptor);
    }
    assert_eq!(scheduler.iter().count(), 10);
}
