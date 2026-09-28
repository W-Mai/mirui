use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::render::scene::resolver::SliceResolver;
use crate::ui::view::{View, ViewCtx};

#[crate::component]
#[derive(Default)]
pub struct RenderShowcase;

fn showcase_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let transform =
        crate::gallery::fit_logical_canvas(*rect, ctx.transform, LOGICAL_WIDTH, LOGICAL_HEIGHT);
    let resolver = SliceResolver::new(&[], &[]);
    let scratch = world
        .resource::<crate::gallery::SceneReplayWorkspace>()
        .expect("Render Showcase setup installs scene RGBA storage");
    let _ = scratch.with_prepared_surface(*rect, renderer.output_scale(), |rgba| {
        ctx.replay_transformed_with_rgba(renderer, SCENE, &resolver, transform, rgba)
    });
}

pub fn showcase_view() -> View {
    View::new("RenderShowcase", 60, showcase_render).with_filter::<RenderShowcase>()
}
