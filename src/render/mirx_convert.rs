use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::render::command::CompositeMode;
use crate::render::path::{Path, PathCmd};
use crate::render::raster::FillRule;
use crate::render::scene::{ResourceRef, Scene, SceneOp};
use crate::types::{Color, Fixed, Point, Rect, Transform, fixed::storage};

impl From<mirx::types::Fixed> for Fixed {
    fn from(v: mirx::types::Fixed) -> Self {
        storage::from_le_bytes(v.to_le_bytes())
    }
}

impl From<Fixed> for mirx::types::Fixed {
    fn from(v: Fixed) -> Self {
        mirx::types::Fixed::from_le_bytes(storage::to_le_bytes(v))
    }
}

impl From<mirx::types::Point> for Point {
    fn from(p: mirx::types::Point) -> Self {
        Self {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl From<Point> for mirx::types::Point {
    fn from(p: Point) -> Self {
        Self {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl From<mirx::types::Rect> for Rect {
    fn from(r: mirx::types::Rect) -> Self {
        Self {
            x: r.x.into(),
            y: r.y.into(),
            w: r.w.into(),
            h: r.h.into(),
        }
    }
}

impl From<Rect> for mirx::types::Rect {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x.into(),
            y: r.y.into(),
            w: r.w.into(),
            h: r.h.into(),
        }
    }
}

impl From<mirx::types::Transform> for Transform {
    fn from(t: mirx::types::Transform) -> Self {
        Self {
            m00: t.m00.into(),
            m01: t.m01.into(),
            tx: t.tx.into(),
            m10: t.m10.into(),
            m11: t.m11.into(),
            ty: t.ty.into(),
        }
    }
}

impl From<Transform> for mirx::types::Transform {
    fn from(t: Transform) -> Self {
        Self {
            m00: t.m00.into(),
            m01: t.m01.into(),
            tx: t.tx.into(),
            m10: t.m10.into(),
            m11: t.m11.into(),
            ty: t.ty.into(),
        }
    }
}

impl From<mirx::types::Color> for Color {
    fn from(c: mirx::types::Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

impl From<Color> for mirx::types::Color {
    fn from(c: Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

impl From<mirx::scene::PathCmd> for PathCmd {
    fn from(c: mirx::scene::PathCmd) -> Self {
        match c {
            mirx::scene::PathCmd::MoveTo(p) => Self::MoveTo(p.into()),
            mirx::scene::PathCmd::LineTo(p) => Self::LineTo(p.into()),
            mirx::scene::PathCmd::QuadTo { ctrl, end } => Self::QuadTo {
                ctrl: ctrl.into(),
                end: end.into(),
            },
            mirx::scene::PathCmd::CubicTo { ctrl1, ctrl2, end } => Self::CubicTo {
                ctrl1: ctrl1.into(),
                ctrl2: ctrl2.into(),
                end: end.into(),
            },
            mirx::scene::PathCmd::Close => Self::Close,
        }
    }
}

impl From<PathCmd> for mirx::scene::PathCmd {
    fn from(c: PathCmd) -> Self {
        match c {
            PathCmd::MoveTo(p) => Self::MoveTo(p.into()),
            PathCmd::LineTo(p) => Self::LineTo(p.into()),
            PathCmd::QuadTo { ctrl, end } => Self::QuadTo {
                ctrl: ctrl.into(),
                end: end.into(),
            },
            PathCmd::CubicTo { ctrl1, ctrl2, end } => Self::CubicTo {
                ctrl1: ctrl1.into(),
                ctrl2: ctrl2.into(),
                end: end.into(),
            },
            PathCmd::Close => Self::Close,
        }
    }
}

impl From<mirx::scene::Path> for Path {
    fn from(p: mirx::scene::Path) -> Self {
        let cmds: Vec<PathCmd> = p.cmds.into_iter().map(Into::into).collect();
        Self {
            cmds: Cow::Owned(cmds),
        }
    }
}

impl From<Path> for mirx::scene::Path {
    fn from(p: Path) -> Self {
        let cmds: Vec<mirx::scene::PathCmd> =
            p.cmds.into_owned().into_iter().map(Into::into).collect();
        Self::from_cmds(cmds)
    }
}

impl From<mirx::scene::ResourceRef> for ResourceRef {
    fn from(r: mirx::scene::ResourceRef) -> Self {
        match r {
            mirx::scene::ResourceRef::Token(s) => Self::Token(Cow::Owned(s)),
            mirx::scene::ResourceRef::Index(i) => Self::Index(i),
            mirx::scene::ResourceRef::Inline(p) => Self::Inline(p.into()),
        }
    }
}

impl From<ResourceRef> for mirx::scene::ResourceRef {
    fn from(r: ResourceRef) -> Self {
        match r {
            ResourceRef::Token(s) => Self::Token(s.into_owned()),
            ResourceRef::Index(i) => Self::Index(i),
            ResourceRef::Inline(p) => Self::Inline(p.into()),
        }
    }
}

impl From<mirx::scene::FillRule> for FillRule {
    fn from(r: mirx::scene::FillRule) -> Self {
        match r {
            mirx::scene::FillRule::EvenOdd => Self::EvenOdd,
            mirx::scene::FillRule::NonZero => Self::NonZero,
        }
    }
}

impl From<FillRule> for mirx::scene::FillRule {
    fn from(r: FillRule) -> Self {
        match r {
            FillRule::EvenOdd => Self::EvenOdd,
            FillRule::NonZero => Self::NonZero,
        }
    }
}

impl From<mirx::scene::CompositeMode> for CompositeMode {
    fn from(m: mirx::scene::CompositeMode) -> Self {
        match m {
            mirx::scene::CompositeMode::SourceOver => Self::SourceOver,
            mirx::scene::CompositeMode::Add => Self::Add,
            mirx::scene::CompositeMode::Screen => Self::Screen,
            mirx::scene::CompositeMode::Multiply => Self::Multiply,
            mirx::scene::CompositeMode::Darken => Self::Darken,
            mirx::scene::CompositeMode::Lighten => Self::Lighten,
            mirx::scene::CompositeMode::Difference => Self::Difference,
        }
    }
}

impl From<CompositeMode> for mirx::scene::CompositeMode {
    fn from(m: CompositeMode) -> Self {
        match m {
            CompositeMode::SourceOver => Self::SourceOver,
            CompositeMode::Add => Self::Add,
            CompositeMode::Screen => Self::Screen,
            CompositeMode::Multiply => Self::Multiply,
            CompositeMode::Darken => Self::Darken,
            CompositeMode::Lighten => Self::Lighten,
            CompositeMode::Difference => Self::Difference,
        }
    }
}

impl From<mirx::scene::SceneOp> for SceneOp {
    fn from(op: mirx::scene::SceneOp) -> Self {
        match op {
            mirx::scene::SceneOp::GroupBegin {
                transform,
                opacity,
                clip,
                mask,
                filter,
                disjoint_hint,
            } => Self::GroupBegin {
                transform: transform.map(Into::into),
                opacity,
                clip: clip.map(Into::into),
                mask: mask.map(Into::into),
                filter: filter.map(Into::into),
                disjoint_hint,
            },
            mirx::scene::SceneOp::GroupEnd => Self::GroupEnd,
            mirx::scene::SceneOp::PushClip {
                path,
                transform,
                fill_rule,
            } => Self::PushClip {
                path: path.into(),
                transform: transform.into(),
                fill_rule: fill_rule.into(),
            },
            mirx::scene::SceneOp::PopClip => Self::PopClip,
            mirx::scene::SceneOp::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
            } => Self::FillPath {
                path: path.into(),
                transform: transform.into(),
                paint,
                opa,
                fill_rule: fill_rule.into(),
            },
            mirx::scene::SceneOp::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => Self::StrokePath {
                path: path.into(),
                transform: transform.into(),
                paint,
                width: width.into(),
                opa,
                line_cap,
                line_join,
                miter_limit: miter_limit.into(),
                dash: dash
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect::<alloc::vec::Vec<_>>()
                    .into(),
            },
            mirx::scene::SceneOp::FillRect {
                area,
                transform,
                quad,
                color,
                radius,
                opa,
            } => Self::FillRect {
                area: area.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                color: color.into(),
                radius: radius.into(),
                opa,
            },
            mirx::scene::SceneOp::Border {
                area,
                transform,
                quad,
                color,
                width,
                radius,
                opa,
            } => Self::Border {
                area: area.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                color: color.into(),
                width: width.into(),
                radius: radius.into(),
                opa,
            },
            mirx::scene::SceneOp::Label {
                font,
                pos,
                transform,
                color,
                opa,
                text,
            } => Self::Label {
                font: font.into(),
                pos: pos.into(),
                transform: transform.into(),
                color: color.into(),
                opa,
                text: Cow::Owned(text),
            },
            mirx::scene::SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => Self::Line {
                p1: p1.into(),
                p2: p2.into(),
                transform: transform.into(),
                color: color.into(),
                width: width.into(),
                opa,
            },
            mirx::scene::SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => Self::Arc {
                center: center.into(),
                transform: transform.into(),
                radius: radius.into(),
                start_angle: start_angle.into(),
                end_angle: end_angle.into(),
                color: color.into(),
                width: width.into(),
                opa,
            },
            mirx::scene::SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
                radius,
                composite,
            } => Self::Blit {
                texture: texture.into(),
                pos: pos.into(),
                size: size.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                opa,
                radius: radius.into(),
                composite: composite.into(),
            },
        }
    }
}

impl From<SceneOp> for mirx::scene::SceneOp {
    fn from(op: SceneOp) -> Self {
        match op {
            SceneOp::GroupBegin {
                transform,
                opacity,
                clip,
                mask,
                filter,
                disjoint_hint,
            } => Self::GroupBegin {
                transform: transform.map(Into::into),
                opacity,
                clip: clip.map(Into::into),
                mask: mask.map(Into::into),
                filter: filter.map(Into::into),
                disjoint_hint,
            },
            SceneOp::GroupEnd => Self::GroupEnd,
            SceneOp::PushClip {
                path,
                transform,
                fill_rule,
            } => Self::PushClip {
                path: path.into(),
                transform: transform.into(),
                fill_rule: fill_rule.into(),
            },
            SceneOp::PopClip => Self::PopClip,
            SceneOp::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
            } => Self::FillPath {
                path: path.into(),
                transform: transform.into(),
                paint,
                opa,
                fill_rule: fill_rule.into(),
            },
            SceneOp::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => Self::StrokePath {
                path: path.into(),
                transform: transform.into(),
                paint,
                width: width.into(),
                opa,
                line_cap,
                line_join,
                miter_limit: miter_limit.into(),
                dash: dash
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect::<alloc::vec::Vec<_>>()
                    .into(),
            },
            SceneOp::FillRect {
                area,
                transform,
                quad,
                color,
                radius,
                opa,
            } => Self::FillRect {
                area: area.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                color: color.into(),
                radius: radius.into(),
                opa,
            },
            SceneOp::Border {
                area,
                transform,
                quad,
                color,
                width,
                radius,
                opa,
            } => Self::Border {
                area: area.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                color: color.into(),
                width: width.into(),
                radius: radius.into(),
                opa,
            },
            SceneOp::Label {
                font,
                pos,
                transform,
                color,
                opa,
                text,
            } => Self::Label {
                font: font.into(),
                pos: pos.into(),
                transform: transform.into(),
                color: color.into(),
                opa,
                text: text.into_owned(),
            },
            SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => Self::Line {
                p1: p1.into(),
                p2: p2.into(),
                transform: transform.into(),
                color: color.into(),
                width: width.into(),
                opa,
            },
            SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => Self::Arc {
                center: center.into(),
                transform: transform.into(),
                radius: radius.into(),
                start_angle: start_angle.into(),
                end_angle: end_angle.into(),
                color: color.into(),
                width: width.into(),
                opa,
            },
            SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
                radius,
                composite,
            } => Self::Blit {
                texture: texture.into(),
                pos: pos.into(),
                size: size.into(),
                transform: transform.into(),
                quad: quad.map(|q| [q[0].into(), q[1].into(), q[2].into(), q[3].into()]),
                opa,
                radius: radius.into(),
                composite: composite.into(),
            },
        }
    }
}

impl From<mirx::scene::Scene> for Scene {
    fn from(s: mirx::scene::Scene) -> Self {
        let ops: Vec<SceneOp> = s.ops.into_iter().map(Into::into).collect();
        Self { ops }
    }
}

impl From<Scene> for mirx::scene::Scene {
    fn from(s: Scene) -> Self {
        let ops: Vec<mirx::scene::SceneOp> = s.ops.into_iter().map(Into::into).collect();
        Self::from_ops(ops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_conversion_preserves_every_endpoint_bit() {
        for bits in [i32::MIN, -1, 0, 1, i32::MAX] {
            let wire = mirx::types::Fixed::from_le_bytes(bits.to_le_bytes());
            let runtime = Fixed::from(wire);
            assert_eq!(
                mirx::types::Fixed::from(runtime).to_le_bytes(),
                bits.to_le_bytes()
            );
        }
    }
}
