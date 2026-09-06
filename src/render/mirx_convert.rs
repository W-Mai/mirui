use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::render::command::CompositeMode;
use crate::render::path::{Path, PathCmd};
use crate::render::raster::FillRule;
use crate::render::scene::{ResourceRef, Scene, SceneOp};
use crate::types::{Color, Fixed, Point, Rect, Transform, fixed::storage};

impl From<mirx::Fixed> for Fixed {
    fn from(v: mirx::Fixed) -> Self {
        storage::from_le_bytes(v.to_le_bytes())
    }
}

impl From<Fixed> for mirx::Fixed {
    fn from(v: Fixed) -> Self {
        mirx::Fixed::from_le_bytes(storage::to_le_bytes(v))
    }
}

impl From<mirx::Point> for Point {
    fn from(p: mirx::Point) -> Self {
        Self {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl From<Point> for mirx::Point {
    fn from(p: Point) -> Self {
        Self {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

impl From<mirx::Rect> for Rect {
    fn from(r: mirx::Rect) -> Self {
        Self {
            x: r.x.into(),
            y: r.y.into(),
            w: r.w.into(),
            h: r.h.into(),
        }
    }
}

impl From<Rect> for mirx::Rect {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x.into(),
            y: r.y.into(),
            w: r.w.into(),
            h: r.h.into(),
        }
    }
}

impl From<mirx::Transform> for Transform {
    fn from(t: mirx::Transform) -> Self {
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

impl From<Transform> for mirx::Transform {
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

impl From<mirx::Color> for Color {
    fn from(c: mirx::Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

impl From<Color> for mirx::Color {
    fn from(c: Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

impl From<mirx::PathCmd> for PathCmd {
    fn from(c: mirx::PathCmd) -> Self {
        match c {
            mirx::PathCmd::MoveTo(p) => Self::MoveTo(p.into()),
            mirx::PathCmd::LineTo(p) => Self::LineTo(p.into()),
            mirx::PathCmd::QuadTo { ctrl, end } => Self::QuadTo {
                ctrl: ctrl.into(),
                end: end.into(),
            },
            mirx::PathCmd::CubicTo { ctrl1, ctrl2, end } => Self::CubicTo {
                ctrl1: ctrl1.into(),
                ctrl2: ctrl2.into(),
                end: end.into(),
            },
            mirx::PathCmd::Close => Self::Close,
        }
    }
}

impl From<PathCmd> for mirx::PathCmd {
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

impl From<mirx::Path> for Path {
    fn from(p: mirx::Path) -> Self {
        let cmds: Vec<PathCmd> = p.cmds.into_iter().map(Into::into).collect();
        Self {
            cmds: Cow::Owned(cmds),
        }
    }
}

impl From<Path> for mirx::Path {
    fn from(p: Path) -> Self {
        let cmds: Vec<mirx::PathCmd> = p.cmds.into_owned().into_iter().map(Into::into).collect();
        Self::from_cmds(cmds)
    }
}

impl From<mirx::ResourceRef> for ResourceRef {
    fn from(r: mirx::ResourceRef) -> Self {
        match r {
            mirx::ResourceRef::Token(s) => Self::Token(Cow::Owned(s)),
            mirx::ResourceRef::Index(i) => Self::Index(i),
            mirx::ResourceRef::Inline(p) => Self::Inline(p.into()),
        }
    }
}

impl From<ResourceRef> for mirx::ResourceRef {
    fn from(r: ResourceRef) -> Self {
        match r {
            ResourceRef::Token(s) => Self::Token(s.into_owned()),
            ResourceRef::Index(i) => Self::Index(i),
            ResourceRef::Inline(p) => Self::Inline(p.into()),
        }
    }
}

impl From<mirx::FillRule> for FillRule {
    fn from(r: mirx::FillRule) -> Self {
        match r {
            mirx::FillRule::EvenOdd => Self::EvenOdd,
            mirx::FillRule::NonZero => Self::NonZero,
        }
    }
}

impl From<FillRule> for mirx::FillRule {
    fn from(r: FillRule) -> Self {
        match r {
            FillRule::EvenOdd => Self::EvenOdd,
            FillRule::NonZero => Self::NonZero,
        }
    }
}

impl From<mirx::CompositeMode> for CompositeMode {
    fn from(m: mirx::CompositeMode) -> Self {
        match m {
            mirx::CompositeMode::SourceOver => Self::SourceOver,
            mirx::CompositeMode::Add => Self::Add,
            mirx::CompositeMode::Screen => Self::Screen,
            mirx::CompositeMode::Multiply => Self::Multiply,
            mirx::CompositeMode::Darken => Self::Darken,
            mirx::CompositeMode::Lighten => Self::Lighten,
            mirx::CompositeMode::Difference => Self::Difference,
        }
    }
}

impl From<CompositeMode> for mirx::CompositeMode {
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

impl From<mirx::SceneOp> for SceneOp {
    fn from(op: mirx::SceneOp) -> Self {
        match op {
            mirx::SceneOp::GroupBegin {
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
            mirx::SceneOp::GroupEnd => Self::GroupEnd,
            mirx::SceneOp::PushClip {
                path,
                transform,
                fill_rule,
            } => Self::PushClip {
                path: path.into(),
                transform: transform.into(),
                fill_rule: fill_rule.into(),
            },
            mirx::SceneOp::PopClip => Self::PopClip,
            mirx::SceneOp::FillPath {
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
            mirx::SceneOp::StrokePath {
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
            mirx::SceneOp::FillRect {
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
            mirx::SceneOp::Border {
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
            mirx::SceneOp::Label {
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
            mirx::SceneOp::Line {
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
            mirx::SceneOp::Arc {
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
            mirx::SceneOp::Blit {
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

impl From<SceneOp> for mirx::SceneOp {
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

impl From<mirx::Scene> for Scene {
    fn from(s: mirx::Scene) -> Self {
        let ops: Vec<SceneOp> = s.ops.into_iter().map(Into::into).collect();
        Self { ops }
    }
}

impl From<Scene> for mirx::Scene {
    fn from(s: Scene) -> Self {
        let ops: Vec<mirx::SceneOp> = s.ops.into_iter().map(Into::into).collect();
        Self::from_ops(ops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_conversion_preserves_every_endpoint_bit() {
        for bits in [i32::MIN, -1, 0, 1, i32::MAX] {
            let wire = mirx::Fixed::from_le_bytes(bits.to_le_bytes());
            let runtime = Fixed::from(wire);
            assert_eq!(mirx::Fixed::from(runtime).to_le_bytes(), bits.to_le_bytes());
        }
    }
}
