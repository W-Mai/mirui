use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::prelude::*;
use crate::render::scene::resolver::SliceResolver;
use crate::ui::view::{PaintCtx, View};

#[crate::component]
#[derive(Default)]
pub struct RenderShowcase;

fn showcase_render(world: &World, _entity: Entity, paint: &mut PaintCtx<'_>) {
    let rect = paint.rect();
    let transform =
        crate::gallery::fit_logical_canvas(rect, paint.transform(), LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let resolver = SliceResolver::new(&[], &[]);
    let scratch = world
        .resource::<crate::gallery::SceneReplayWorkspace>()
        .expect("Render Showcase setup installs scene RGBA storage");
    let output_scale = paint.output_scale();
    let _ = scratch.with_prepared_surface(rect, output_scale, |rgba| {
        paint.replay_transformed_with_rgba(SCENE, &resolver, transform, rgba)
    });
}

pub fn showcase_view() -> View {
    View::new("RenderShowcase", 60, showcase_render).with_filter::<RenderShowcase>()
}
