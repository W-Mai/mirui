//! Bridge `InputEvent::AppSuspend` / `AppResume` from backends through
//! `SuspendRequest` so `App::tick` flips `App::suspend()` / `App::resume()`
//! the next time it drains the resource.
//!
//! Host focus and visibility notifications do not request suspension.

use crate::app::lifecycle::SuspendRequest;
use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::input::event::input::InputEvent;
use crate::surface::Surface;

/// **Inserts**
/// - resource: `SuspendRequest` (only on lifecycle events; otherwise none)
/// - hooks:    `on_event` (swallows `AppSuspend` / `AppResume`)
#[derive(Default)]
pub struct AutoSuspendPlugin;

impl<B, F> Plugin<B, F> for AutoSuspendPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        match event {
            InputEvent::AppSuspend => {
                world.insert_resource(SuspendRequest::Suspend);
                true
            }
            InputEvent::AppResume => {
                world.insert_resource(SuspendRequest::Resume);
                true
            }
            _ => false,
        }
    }
}
