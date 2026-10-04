use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::render::scene::resolver::SliceResolver;

#[crate::component]
#[derive(Default)]
pub struct RenderShowcase;

#[crate::view(component = RenderShowcase, name = "RenderShowcase", priority = 60)]
fn showcase_render(scratch: res!(crate::gallery::SceneReplayWorkspace), paint: paint!()) {
    let rect = paint.rect();
    let transform =
        crate::gallery::fit_logical_canvas(rect, paint.transform(), LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let resolver = SliceResolver::new(&[], &[]);
    let output_scale = paint.output_scale();
    let _ = scratch.with_prepared_surface(rect, output_scale, |rgba| {
        paint.replay_transformed_with_rgba(SCENE, &resolver, transform, rgba)
    });
}

pub use showcase_render::view as showcase_view;
