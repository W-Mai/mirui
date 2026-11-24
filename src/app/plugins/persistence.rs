use crate::app::{App, RendererFactory, plugin::Plugin};
use crate::core::persistence::{PersistencePlugin, flush, flush_due};
use crate::core::storage::Storage;
use crate::ecs::World;
use crate::surface::Surface;

impl<B, F, S> Plugin<B, F> for PersistencePlugin<S>
where
    B: Surface,
    F: RendererFactory<B>,
    S: Storage + 'static,
{
    fn build(&mut self, app: &mut App<B, F>) {
        self.install(&mut app.world);
    }

    fn post_render(&mut self, world: &mut World, _render_nanos: Option<u64>) {
        flush_due(world);
    }

    fn on_suspend(&mut self, world: &mut World) {
        flush(world);
    }

    fn on_quit(&mut self, world: &mut World) {
        flush(world);
    }
}
