use mirui::ecs::World;

fn main() {
    let mut world = World::new();
    let _ = mirui::core::timer::timer_system(&mut world);
    let _ = mirui::ui::state::hover_system(&mut world);
    let _ = mirui::ui::state::press_system(&mut world);
    let _ = mirui::ui::widgets::lazy_list::lazy_list_system(&mut world);
    let _ = mirui::ui::widgets::tab_pages::tab_pages_system(&mut world);
    let _ = mirui::input::event::sim::sim_input_system(&mut world);
    let _ = mirui::input::event::sim::sim_timeline_system(&mut world);
    let _ = mirui::input::event::scroll::system::scroll_inertia_system(&mut world);
    let _ = mirui::input::feedback::cursor::cursor_feedback_system(&mut world);
    let _ = mirui::input::event::scroll::scroll_inertia_system(&mut world);
}
