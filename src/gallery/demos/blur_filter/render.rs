use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::resolver::SliceResolver;

#[derive(Default)]
pub struct BlurFilter;

fn blur_filter_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let transform =
        crate::gallery::fit_logical_canvas(*rect, ctx.transform, LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let scratch = world
        .resource::<crate::gallery::SceneReplayWorkspace>()
        .expect("Blur Filter setup installs scene RGBA storage");
    let _ = scratch.with_prepared_surface(*rect, renderer.output_scale(), |rgba| {
        ctx.replay_transformed_with_rgba(
            renderer,
            SCENE,
            &SliceResolver::new(&[], &[]),
            transform,
            rgba,
        )
    });
}

pub fn blur_filter_view() -> View {
    View::new("BlurFilter", 60, blur_filter_render).with_filter::<BlurFilter>()
}
