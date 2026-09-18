#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::SceneOp;
use crate::render::scene::resolver::SliceResolver;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const LOGICAL_WIDTH: i32 = 480;
const LOGICAL_HEIGHT: i32 = 240;

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

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 6,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "BLUR FILTER",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "isolated group · caller-owned RGBA workspace",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            BlurFilter (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(crate::gallery::SceneReplayWorkspacePlugin);
    app.with_widget(blur_filter_view());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::scene::ResourceRef;
    use crate::types::Transform;
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

    #[test]
    fn scene_canvas_stays_inside_phone_bounds() {
        let rect = Rect::new(8, 72, 304, 480);
        let transform = crate::gallery::fit_logical_canvas(
            rect,
            Transform::IDENTITY,
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
        );
        let top_left = transform.apply_point(Point::ZERO);
        let bottom_right = transform.apply_point(Point::new(LOGICAL_WIDTH, LOGICAL_HEIGHT));
        assert!(top_left.x >= rect.x && top_left.y >= rect.y);
        assert!(bottom_right.x <= rect.x + rect.w);
        assert!(bottom_right.y <= rect.y + rect.h);
    }
}
