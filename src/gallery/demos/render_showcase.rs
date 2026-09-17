#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::scene::SceneOp;
use crate::render::scene::resolver::SliceResolver;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const LOGICAL_WIDTH: i32 = 640;
const LOGICAL_HEIGHT: i32 = 660;

#[derive(Default)]
pub struct RenderShowcase;

const SCENE: &[SceneOp] = scene! {
    rect 0 0 640 660 0 22 24 32 255 255;

    fill_path { M 16 16; L 316 16; L 316 232; L 16 232; Z } 30 34 44 255 255;
    fill_path { M 324 16; L 624 16; L 624 232; L 324 232; Z } 30 34 44 255 255;
    fill_path { M 16 248; L 316 248; L 316 464; L 16 464; Z } 30 34 44 255 255;
    fill_path { M 324 248; L 624 248; L 624 464; L 324 464; Z } 30 34 44 255 255;
    fill_path { M 16 480; L 316 480; L 316 644; L 16 644; Z } 30 34 44 255 255;
    fill_path { M 324 480; L 624 480; L 624 644; L 324 644; Z } 30 34 44 255 255;

    fill_path {
        M 50 40;
        C 50 32 58 26 66 26;
        C 74 26 82 32 82 40;
        C 82 48 74 54 66 54;
        C 58 54 50 48 50 40;
        Z;
        M 166 26;
        C 224 26 268 70 268 128;
        C 268 186 224 230 166 230;
        C 108 230 64 186 64 128;
        C 64 70 108 26 166 26;
        Z
    } { linear 0 0 1 1 stops [0 50 120 255 255 0.55 125 90 255 255 1 255 70 90 255] spread pad units object_bbox } 255 fill_rule evenodd;
    stroke_path {
        M 50 40;
        C 50 32 58 26 66 26;
        C 74 26 82 32 82 40;
        C 82 48 74 54 66 54;
        C 58 54 50 48 50 40;
        Z;
        M 166 26;
        C 224 26 268 70 268 128;
        C 268 186 224 230 166 230;
        C 108 230 64 186 64 128;
        C 64 70 108 26 166 26;
        Z
    } 3 255 255 255 255 220 cap round join round miter 4 dash [10 6 4 6];

    fill_path {
        M 194 188;
        C 194 172 182 160 166 160;
        C 150 160 138 172 138 188;
        C 138 204 150 216 166 216;
        C 182 216 194 204 194 188;
        Z
    } { radial 0.42 0.38 0.7 0.42 0.38 0 stops [0 255 255 255 255 0.42 90 190 255 255 1 20 70 190 255] spread pad units object_bbox } 255 fill_rule evenodd;

    push_clip {
        M 562 124;
        C 562 79 527 44 482 44;
        C 437 44 402 79 402 124;
        C 402 169 437 204 482 204;
        C 527 204 562 169 562 124;
        Z
    } fill_rule evenodd;
    rect 382 40 184 38 8 255 90 110 255 255;
    rect 382 82 184 38 8 255 190 80 255 255;
    rect 382 124 184 38 8 80 210 160 255 255;
    rect 382 166 184 38 8 80 145 255 255 255;
    pop_clip;
    stroke_path {
        M 562 124;
        C 562 79 527 44 482 44;
        C 437 44 402 79 402 124;
        C 402 169 437 204 482 204;
        C 527 204 562 169 562 124;
        Z
    } 2 230 235 245 255 220 cap round join round miter 4 dash [];

    group filter "blur:4:4" disjoint;
    rect 36 272 80 80 8 255 90 110 255 255;
    rect 96 296 80 80 8 80 210 160 255 255;
    rect 176 272 80 80 8 255 190 80 255 255;
    fill_path {
        M 160 312;
        C 160 288 140 268 116 268;
        C 92 268 72 288 72 312;
        C 72 336 92 356 116 356;
        C 140 356 160 336 160 312;
        Z
    } 145 255 120 255 230 fill_rule evenodd;
    endgroup;

    rect 344 272 80 80 8 255 90 110 255 255;
    rect 404 296 80 80 8 80 210 160 255 255;
    rect 484 272 80 80 8 255 190 80 255 255;
    fill_path {
        M 468 312;
        C 468 288 448 268 424 268;
        C 400 268 380 288 380 312;
        C 380 336 400 356 424 356;
        C 448 356 468 336 468 312;
        Z
    } 145 255 120 255 230 fill_rule evenodd;

    fill_path {
        M 390 44;
        L 414 104;
        L 366 104;
        Z;
        M 390 204;
        L 366 144;
        L 414 144;
        Z;
        M 330 124;
        L 370 100;
        L 370 148;
        Z;
        M 450 124;
        L 410 100;
        L 410 148;
        Z
    } 255 120 180 255 255 fill_rule evenodd;

    fill_path {
        M 560 44;
        L 584 104;
        L 536 104;
        Z;
        M 560 204;
        L 536 144;
        L 584 144;
        Z;
        M 500 124;
        L 540 100;
        L 540 148;
        Z;
        M 620 124;
        L 580 100;
        L 580 148;
        Z;
        M 560 44;
        L 584 104;
        L 560 204;
        L 536 144;
        Z
    } 120 220 255 255 255 fill_rule nonzero;

    stroke_path { M 48 510; L 120 486; L 192 510; L 264 486 } 8 255 220 120 255 255 cap butt join miter miter 4 dash [];
    stroke_path { M 48 546; L 120 522; L 192 546; L 264 522 } 8 255 220 120 255 255 cap round join round miter 4 dash [];
    stroke_path { M 48 582; L 120 558; L 192 582; L 264 558 } 8 255 220 120 255 255 cap square join bevel miter 4 dash [];

    group translate 474 560 rotate 0;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 100 160 235 255 220 fill_rule evenodd;
    endgroup;
    group translate 474 560 rotate 30;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 120 160 215 255 220 fill_rule evenodd;
    endgroup;
    group translate 474 560 rotate 60;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 140 160 195 255 220 fill_rule evenodd;
    endgroup;
    group translate 474 560 rotate 90;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 160 160 175 255 220 fill_rule evenodd;
    endgroup;
    group translate 474 560 rotate 120;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 180 160 155 255 220 fill_rule evenodd;
    endgroup;
    group translate 474 560 rotate 150;
    fill_path { M -50 -16; L 50 -16; L 50 16; L -50 16; Z } 200 160 135 255 220 fill_rule evenodd;
    endgroup;

    stroke_path {
        M 530 560;
        C 530 529 505 504 474 504;
        C 443 504 418 529 418 560;
        C 418 591 443 616 474 616;
        C 505 616 530 591 530 560;
        Z
    } 2 255 255 255 255 180 cap round join round miter 4 dash [6 4]
};

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
                "RENDER SHOWCASE",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "gradient · clip · blur · fill-rule · stroke · transform",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            RenderShowcase (
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
    let scale = app.viewport().scale();
    let (width, height) = app.viewport().logical_size();
    crate::gallery::SceneReplayWorkspace::install(
        &mut app.world,
        Rect::new(0, 0, width, height),
        scale,
    )
    .expect("Render Showcase scene workspace size is representable");
    app.with_widget(showcase_view());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Transform;

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
