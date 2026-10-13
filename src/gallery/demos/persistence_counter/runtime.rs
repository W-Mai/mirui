#[cfg(all(feature = "std", feature = "persistence"))]
use super::composition::build_widgets;
#[cfg(all(feature = "std", feature = "persistence"))]
use super::storage::pick_storage;
#[cfg(all(feature = "std", feature = "persistence"))]
use crate::core::persistence::PersistencePlugin;
#[cfg(all(feature = "std", feature = "persistence"))]
use crate::prelude::*;

#[cfg(all(feature = "std", feature = "persistence"))]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);

    let count = Signal::new(0i32);
    match pick_storage() {
        Ok(storage) => {
            app.add_plugin(PersistencePlugin::new(storage).autosave(2000));
            if let Err(error) = app.persistence().register_signal("count", count.clone()) {
                crate::warn!("counter persistence registration failed: {error}");
            }
        }
        Err(error) => crate::warn!("counter persistence unavailable: {error}"),
    }

    app.compose(parent, build_widgets(count));
}
