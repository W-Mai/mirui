use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::resolver::SliceResolver;

#[crate::component]
#[derive(Default)]
pub struct BlurFilter;

fn blur_filter_render(world: &World, _entity: Entity, paint: &mut PaintCtx<'_>) {
    let rect = paint.rect();
    let transform =
        crate::gallery::fit_logical_canvas(rect, paint.transform(), LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let scratch = world
        .resource::<crate::gallery::SceneReplayWorkspace>()
        .expect("Blur Filter setup installs scene RGBA storage");
    let output_scale = paint.output_scale();
    let _ = scratch.with_prepared_surface(rect, output_scale, |rgba| {
        paint.replay_transformed_with_rgba(SCENE, &SliceResolver::new(&[], &[]), transform, rgba);
    });
}

pub fn blur_filter_view() -> View {
    View::new("BlurFilter", 60, blur_filter_render).with_filter::<BlurFilter>()
}
