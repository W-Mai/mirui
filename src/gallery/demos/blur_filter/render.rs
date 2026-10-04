use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::render::scene::resolver::SliceResolver;

#[crate::component]
#[derive(Default)]
pub struct BlurFilter;

#[crate::view(component = BlurFilter, name = "BlurFilter", priority = 60)]
fn blur_filter_render(scratch: res!(crate::gallery::SceneReplayWorkspace), paint: paint!()) {
    let rect = paint.rect();
    let transform =
        crate::gallery::fit_logical_canvas(rect, paint.transform(), LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let output_scale = paint.output_scale();
    let _ = scratch.with_prepared_surface(rect, output_scale, |rgba| {
        paint.replay_transformed_with_rgba(SCENE, &SliceResolver::new(&[], &[]), transform, rgba);
    });
}

pub use blur_filter_render::view as blur_filter_view;
