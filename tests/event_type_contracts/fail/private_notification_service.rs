use mirui::__macro_support::PendingNotifications;

fn main() {
    let mut world = mirui::ecs::World::new();
    let _ = world.remove_resource::<std::rc::Rc<PendingNotifications>>();
}
