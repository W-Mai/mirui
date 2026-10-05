use mirui::core::model::ModelHandle;
use mirui::{
    __macro_support::DeferredCommands,
    ecs::{CommandError, World},
};

fn raw(_: &mut World) -> Result<(), CommandError> {
    Ok(())
}

fn deferred(_: &mut World, _: DeferredCommands<'_>) -> Result<(), CommandError> {
    Ok(())
}

fn bound<H: ModelHandle>(model: &H) {
    let _ = mirui::__macro_support::bound_system("bound", 0, model, |_| Ok(()));
    let _ = mirui::__macro_support::bound_deferred_system("bound", 0, model, |_, _| Ok(()));
}

fn main() {
    let _ = mirui::__macro_support::static_system("raw", 0, raw);
    let _ = mirui::__macro_support::deferred_system("deferred", 0, deferred);
}
