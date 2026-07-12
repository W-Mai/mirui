#![allow(clippy::needless_update)]

extern crate alloc;

use alloc::vec;

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::scene::resolver::SliceResolver;
use crate::render::scene::{
    GradientStop, GradientUnits, LineCap, LineJoin, LinearGradient, Paint, RadialGradient,
    ResourceRef, Scene, SceneOp, SpreadMode,
};
use crate::types::{Point, Transform};
use crate::ui::widgets::Text;

#[derive(Default)]
pub struct RenderShowcase;

fn star_path(cx: Fixed, cy: Fixed, r: Fixed) -> Path {
    let mut pts = [Point::ZERO; 5];
    for (i, p) in pts.iter_mut().enumerate() {
        let a = Fixed::from_int(-90 + i as i32 * 72);
        *p = Point {
            x: cx + Fixed::cos_deg(a) * r,
            y: cy + Fixed::sin_deg(a) * r,
        };
    }
    let mut path = Path::new();
    path.move_to(pts[0]);
    for i in [2, 4, 1, 3] {
        path.line_to(pts[i]);
    }
    path.close();
    path
}

fn circle_path(cx: Fixed, cy: Fixed, r: Fixed) -> Path {
    let k = r * Fixed::from_f32(0.552_284_8);
    let mut path = Path::new();
    path.move_to(Point { x: cx + r, y: cy });
    path.cubic_to(
        Point {
            x: cx + r,
            y: cy + k,
        },
        Point {
            x: cx + k,
            y: cy + r,
        },
        Point { x: cx, y: cy + r },
    );
    path.cubic_to(
        Point {
            x: cx - k,
            y: cy + r,
        },
        Point {
            x: cx - r,
            y: cy + k,
        },
        Point { x: cx - r, y: cy },
    );
    path.cubic_to(
        Point {
            x: cx - r,
            y: cy - k,
        },
        Point {
            x: cx - k,
            y: cy - r,
        },
        Point { x: cx, y: cy - r },
    );
    path.cubic_to(
        Point {
            x: cx + k,
            y: cy - r,
        },
        Point {
            x: cx + r,
            y: cy - k,
        },
        Point { x: cx + r, y: cy },
    );
    path.close();
    path
}

fn unit_point(x: f32, y: f32) -> mirx::Point {
    Point {
        x: Fixed::from_f32(x),
        y: Fixed::from_f32(y),
    }
    .into()
}

fn unit(v: f32) -> mirx::Fixed {
    Fixed::from_f32(v).into()
}

fn linear_paint() -> Paint {
    Paint::LinearGradient(LinearGradient {
        start: unit_point(0.0, 0.0),
        end: unit_point(1.0, 1.0),
        stops: vec![
            GradientStop {
                offset: unit(0.0),
                color: Color::rgb(50, 120, 255).into(),
            },
            GradientStop {
                offset: unit(0.55),
                color: Color::rgb(125, 90, 255).into(),
            },
            GradientStop {
                offset: unit(1.0),
                color: Color::rgb(255, 70, 90).into(),
            },
        ],
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

fn radial_paint() -> Paint {
    Paint::RadialGradient(RadialGradient {
        center: unit_point(0.42, 0.38),
        radius: unit(0.7),
        focal: unit_point(0.42, 0.38),
        focal_radius: unit(0.0),
        stops: vec![
            GradientStop {
                offset: unit(0.0),
                color: Color::rgb(255, 255, 255).into(),
            },
            GradientStop {
                offset: unit(0.42),
                color: Color::rgb(90, 190, 255).into(),
            },
            GradientStop {
                offset: unit(1.0),
                color: Color::rgb(20, 70, 190).into(),
            },
        ],
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: Transform::IDENTITY.into(),
    })
}

fn panel_bg_op(s: &mut Scene, x: i32, y: i32, w: i32, h: i32) {
    let p = Path::rounded_rect(
        Fixed::from_int(x),
        Fixed::from_int(y),
        Fixed::from_int(w),
        Fixed::from_int(h),
        Fixed::from_int(10),
    );
    s.push(SceneOp::FillPath {
        path: p,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(30, 34, 44).into()),
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    });
}

fn build_scene() -> Scene {
    let mut s = Scene::new();

    s.push(SceneOp::FillRect {
        area: Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(640),
            Fixed::from_int(660),
        ),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(22, 24, 32),
        radius: Fixed::ZERO,
        opa: 255,
    });

    panel_bg_op(&mut s, 16, 16, 300, 216);
    panel_bg_op(&mut s, 324, 16, 300, 216);
    panel_bg_op(&mut s, 16, 248, 300, 216);
    panel_bg_op(&mut s, 324, 248, 300, 216);
    panel_bg_op(&mut s, 16, 480, 300, 164);
    panel_bg_op(&mut s, 324, 480, 300, 164);

    let grad_rect = Path::rounded_rect(
        Fixed::from_int(36),
        Fixed::from_int(40),
        Fixed::from_int(260),
        Fixed::from_int(120),
        Fixed::from_int(14),
    );
    s.push(SceneOp::FillPath {
        path: grad_rect.clone(),
        transform: Transform::IDENTITY,
        paint: linear_paint(),
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    });
    s.push(SceneOp::StrokePath {
        path: grad_rect,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(255, 255, 255).into()),
        width: Fixed::from_int(3),
        opa: 220,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: vec![
            Fixed::from_int(10),
            Fixed::from_int(6),
            Fixed::from_int(4),
            Fixed::from_int(6),
        ],
    });

    let rad_circle = circle_path(
        Fixed::from_int(166),
        Fixed::from_int(188),
        Fixed::from_int(28),
    );
    s.push(SceneOp::FillPath {
        path: rad_circle,
        transform: Transform::IDENTITY,
        paint: radial_paint(),
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    });

    let clip_circle = circle_path(
        Fixed::from_int(474),
        Fixed::from_int(124),
        Fixed::from_int(88),
    );
    s.push(SceneOp::PushClip {
        path: clip_circle.clone(),
        transform: Transform::IDENTITY,
        fill_rule: FillRule::EvenOdd,
    });
    for (i, c) in [
        Color::rgb(255, 90, 110),
        Color::rgb(255, 190, 80),
        Color::rgb(80, 210, 160),
        Color::rgb(80, 145, 255),
    ]
    .iter()
    .enumerate()
    {
        s.push(SceneOp::FillRect {
            area: Rect::new(
                Fixed::from_int(382),
                Fixed::from_int(40) + Fixed::from_int(i as i32 * 42),
                Fixed::from_int(184),
                Fixed::from_int(38),
            ),
            transform: Transform::IDENTITY,
            quad: None,
            color: *c,
            radius: Fixed::from_int(8),
            opa: 255,
        });
    }
    s.push(SceneOp::PopClip);
    s.push(SceneOp::StrokePath {
        path: clip_circle,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(230, 235, 245).into()),
        width: Fixed::from_int(2),
        opa: 220,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: vec![],
    });

    s.push(SceneOp::GroupBegin {
        transform: None,
        opacity: Some(255),
        clip: None,
        mask: None,
        filter: Some(ResourceRef::Token("blur:4:4".into())),
        disjoint_hint: true,
    });
    s.push(SceneOp::FillRect {
        area: Rect::new(
            Fixed::from_int(36),
            Fixed::from_int(272),
            Fixed::from_int(80),
            Fixed::from_int(80),
        ),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(255, 90, 110),
        radius: Fixed::from_int(8),
        opa: 255,
    });
    s.push(SceneOp::FillRect {
        area: Rect::new(
            Fixed::from_int(96),
            Fixed::from_int(296),
            Fixed::from_int(80),
            Fixed::from_int(80),
        ),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(80, 210, 160),
        radius: Fixed::from_int(8),
        opa: 255,
    });
    s.push(SceneOp::FillRect {
        area: Rect::new(
            Fixed::from_int(176),
            Fixed::from_int(272),
            Fixed::from_int(80),
            Fixed::from_int(80),
        ),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(255, 190, 80),
        radius: Fixed::from_int(8),
        opa: 255,
    });
    let blur_circle = circle_path(
        Fixed::from_int(116),
        Fixed::from_int(312),
        Fixed::from_int(44),
    );
    s.push(SceneOp::FillPath {
        path: blur_circle,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(145, 255, 120).into()),
        opa: 230,
        fill_rule: FillRule::EvenOdd,
    });
    s.push(SceneOp::GroupEnd);

    let sharp_circle = circle_path(
        Fixed::from_int(236),
        Fixed::from_int(312),
        Fixed::from_int(44),
    );
    s.push(SceneOp::FillPath {
        path: sharp_circle,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(145, 255, 120).into()),
        opa: 230,
        fill_rule: FillRule::EvenOdd,
    });

    let star_eo = star_path(
        Fixed::from_int(390),
        Fixed::from_int(124),
        Fixed::from_int(80),
    );
    s.push(SceneOp::FillPath {
        path: star_eo,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(255, 120, 180).into()),
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    });

    let star_nz = star_path(
        Fixed::from_int(560),
        Fixed::from_int(124),
        Fixed::from_int(80),
    );
    s.push(SceneOp::FillPath {
        path: star_nz,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(120, 220, 255).into()),
        opa: 255,
        fill_rule: FillRule::NonZero,
    });

    let mut y = Fixed::from_int(510);
    let styles: &[(LineCap, LineJoin)] = &[
        (LineCap::Butt, LineJoin::Miter),
        (LineCap::Round, LineJoin::Round),
        (LineCap::Square, LineJoin::Bevel),
    ];
    for (cap, join) in styles {
        let mut p = Path::new();
        p.move_to(Point {
            x: Fixed::from_int(48),
            y,
        });
        p.line_to(Point {
            x: Fixed::from_int(120),
            y: y - Fixed::from_int(24),
        });
        p.line_to(Point {
            x: Fixed::from_int(192),
            y,
        });
        p.line_to(Point {
            x: Fixed::from_int(264),
            y: y - Fixed::from_int(24),
        });
        s.push(SceneOp::StrokePath {
            path: p,
            transform: Transform::IDENTITY,
            paint: Paint::Color(Color::rgb(255, 220, 120).into()),
            width: Fixed::from_int(8),
            opa: 255,
            line_cap: *cap,
            line_join: *join,
            miter_limit: Fixed::from_int(4),
            dash: vec![],
        });
        y += Fixed::from_int(36);
    }

    let rotor = Path::rounded_rect(
        Fixed::from_int(-50),
        Fixed::from_int(-16),
        Fixed::from_int(100),
        Fixed::from_int(32),
        Fixed::from_int(8),
    );
    let center = Transform::translate(Fixed::from_int(474), Fixed::from_int(560));
    for i in 0..6 {
        let deg = Fixed::from_int(i * 30);
        let tf = center.compose(&Transform::rotate_deg(deg));
        let paint = Paint::Color(Color::rgb(80 + i as u8 * 20, 160, 255 - i as u8 * 20).into());
        s.push(SceneOp::FillPath {
            path: rotor.clone(),
            transform: tf,
            paint,
            opa: 220,
            fill_rule: FillRule::EvenOdd,
        });
    }

    let ring = circle_path(
        Fixed::from_int(474),
        Fixed::from_int(560),
        Fixed::from_int(56),
    );
    s.push(SceneOp::StrokePath {
        path: ring,
        transform: Transform::IDENTITY,
        paint: Paint::Color(Color::rgb(255, 255, 255).into()),
        width: Fixed::from_int(2),
        opa: 180,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: vec![Fixed::from_int(6), Fixed::from_int(4)],
    });

    s
}

fn showcase_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let scene = build_scene();
    let resolver = SliceResolver::new(&[], &[]);
    let _ = scene.replay(renderer, ctx.clip, &resolver);
}

pub fn showcase_view() -> View {
    View::new("RenderShowcase", 60, showcase_render).with_filter::<RenderShowcase>()
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (grow: 1.0, align: AlignItems::Center) {
            RenderShowcase (width: 640, height: 660)
            Text (
                "gradient · clip · blur · fill-rule · stroke-style · transform",
                width: 640,
                height: 20,
                text_color: ColorToken::OnSurface
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
    app.with_widget(showcase_view());
    app.compose(parent, build_widgets);
}
