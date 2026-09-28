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

    // PersistencePlugin must register before widgets spawn so the
    // Registry sits in the World when on_suspend / on_quit fire.
    let count = Signal::new(0i32);
    let plugin = PersistencePlugin::new(pick_storage())
        .signal("count", count.clone())
        .autosave_every_ms(2000);
    app.add_plugin(plugin);

    app.compose(parent, |cx| build_widgets(cx, count));
}
