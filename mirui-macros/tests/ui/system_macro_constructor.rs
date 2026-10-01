use mirui::core::model::ModelHandle;
use mirui::{__macro_support::DeferredCommands, ecs::World};

fn raw(_: &mut World) {}

fn deferred(_: &mut World, _: DeferredCommands<'_>) {}

fn bound<H: ModelHandle>(model: &H) {
    let _ = mirui::__macro_support::bound_system("bound", 0, model, |_| {});
    let _ = mirui::__macro_support::bound_deferred_system("bound", 0, model, |_, _| {});
}

fn main() {
    let _ = mirui::__macro_support::static_system("raw", 0, raw);
    let _ = mirui::__macro_support::deferred_system("deferred", 0, deferred);
}
