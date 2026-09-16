#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::SceneOp;
use crate::render::scene::resolver::SliceResolver;
use crate::ui::widgets::Text;

#[derive(Default)]
pub struct BlurFilter;

const SCENE: &[SceneOp] = scene! {
    rect 0 0 480 240 0 18 20 28 255 255;

    group filter "blur:3:3" disjoint;
    rect 28 58 118 58 14 255 80 110 255 220;
    rect 82 104 92 60 14 90 210 255 255 220;
    rect 130 44 68 100 14 255 200 80 255 220;
    fill_path {
        M 156 128;
        C 156 152 136 172 112 172;
        C 88 172 68 152 68 128;
        C 68 104 88 84 112 84;
        C 136 84 156 104 156 128;
        Z
    } 145 255 120 255 230 fill_rule evenodd;
    endgroup;

    rect 270 58 118 58 14 255 80 110 255 220;
    rect 324 104 92 60 14 90 210 255 255 220;
    rect 372 44 68 100 14 255 200 80 255 220;
    fill_path {
        M 398 128;
        C 398 152 378 172 354 172;
        C 330 172 310 152 310 128;
        C 310 104 330 84 354 84;
        C 378 84 398 104 398 128;
        Z
    } 145 255 120 255 230 fill_rule evenodd;
};

fn blur_filter_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let scratch = world
        .resource::<crate::gallery::SceneReplayWorkspace>()
        .expect("Blur Filter setup installs scene RGBA storage");
    let _ = scratch.with_prepared_surface(*rect, renderer.output_scale(), |rgba| {
        ctx.replay_with_rgba(renderer, SCENE, &SliceResolver::new(&[], &[]), rgba)
    });
}

pub fn blur_filter_view() -> View {
    View::new("BlurFilter", 60, blur_filter_render).with_filter::<BlurFilter>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::FlexEnd,
            padding: Padding::all(10)
        ) {
            BlurFilter (position: Position::Absolute, left: 0, top: 0, width: 480, height: 240)
            Text ("blur:3:3", width: 120, height: 22, text_color: ColorToken::OnSurface)
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let scale = app.viewport().scale();
    crate::gallery::SceneReplayWorkspace::install(&mut app.world, Rect::new(0, 0, 480, 240), scale)
        .expect("Blur Filter scene workspace size is representable");
    app.with_widget(blur_filter_view());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::scene::ResourceRef;
    use alloc::borrow::Cow;

    #[test]
    fn scene_paths_borrow_static_commands() {
        let mut paths = 0;
        for op in SCENE {
            match op {
                SceneOp::FillPath { path, .. } => {
                    assert!(path.is_borrowed());
                    paths += 1;
                }
                SceneOp::GroupBegin {
                    filter: Some(ResourceRef::Token(token)),
                    ..
                } => assert!(matches!(token, Cow::Borrowed(_))),
                _ => {}
            }
        }
        assert_eq!(paths, 2);
    }
}
