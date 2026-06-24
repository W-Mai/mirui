//! Replay an owned `SceneOp` stream back through a live `Renderer`.

use alloc::vec::Vec;

use super::{ResourceRef, SceneOp};
use crate::render::command::DrawCommand;
use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::types::{Rect, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    UnbalancedGroup,
    UnresolvedFont,
    UnresolvedTexture,
    UnsupportedFillRule,
}

/// Resolves a persisted `ResourceRef` back to a live borrow for the duration
/// of one draw call.
pub trait SceneResolver {
    fn font(&self, r: &ResourceRef) -> Option<&Font>;
    fn texture(&self, r: &ResourceRef) -> Option<&Texture<'_>>;
}

pub fn replay_scene(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
) -> Result<(), ReplayError> {
    let mut stack: Vec<Transform> = alloc::vec![Transform::IDENTITY];
    for op in ops {
        let top = *stack.last().ok_or(ReplayError::UnbalancedGroup)?;
        match op {
            SceneOp::GroupBegin { transform, .. } => {
                let next = match transform {
                    Some(t) => top.compose(t),
                    None => top,
                };
                stack.push(next);
            }
            SceneOp::GroupEnd => {
                if stack.len() <= 1 {
                    return Err(ReplayError::UnbalancedGroup);
                }
                stack.pop();
            }
            SceneOp::FillRect {
                area,
                transform,
                quad,
                color,
                radius,
                opa,
            } => renderer.draw(
                &DrawCommand::Fill {
                    area: *area,
                    transform: top.compose(transform),
                    quad: *quad,
                    color: *color,
                    radius: *radius,
                    opa: *opa,
                },
                clip,
            ),
            SceneOp::Border {
                area,
                transform,
                quad,
                color,
                width,
                radius,
                opa,
            } => renderer.draw(
                &DrawCommand::Border {
                    area: *area,
                    transform: top.compose(transform),
                    quad: *quad,
                    color: *color,
                    width: *width,
                    radius: *radius,
                    opa: *opa,
                },
                clip,
            ),
            SceneOp::Label {
                font,
                pos,
                transform,
                color,
                opa,
                text,
            } => {
                let font = resolver.font(font).ok_or(ReplayError::UnresolvedFont)?;
                renderer.draw(
                    &DrawCommand::Label {
                        pos: *pos,
                        transform: top.compose(transform),
                        text: text.as_bytes(),
                        font,
                        color: *color,
                        opa: *opa,
                    },
                    clip,
                );
            }
            SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => renderer.draw(
                &DrawCommand::Line {
                    p1: *p1,
                    p2: *p2,
                    transform: top.compose(transform),
                    color: *color,
                    width: *width,
                    opa: *opa,
                },
                clip,
            ),
            SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => renderer.draw(
                &DrawCommand::Arc {
                    center: *center,
                    transform: top.compose(transform),
                    radius: *radius,
                    start_angle: *start_angle,
                    end_angle: *end_angle,
                    color: *color,
                    width: *width,
                    opa: *opa,
                },
                clip,
            ),
            SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
            } => {
                let texture = resolver
                    .texture(texture)
                    .ok_or(ReplayError::UnresolvedTexture)?;
                renderer.draw(
                    &DrawCommand::Blit {
                        pos: *pos,
                        size: *size,
                        transform: top.compose(transform),
                        quad: *quad,
                        texture,
                        opa: *opa,
                    },
                    clip,
                );
            }
            SceneOp::FillPath {
                path,
                transform,
                color,
                opa,
                fill_rule,
            } => {
                if !matches!(fill_rule, FillRule::EvenOdd) {
                    return Err(ReplayError::UnsupportedFillRule);
                }
                let p = Path {
                    cmds: path.to_vec(),
                };
                renderer.draw(
                    &DrawCommand::FillPath {
                        path: &p,
                        transform: top.compose(transform),
                        color: *color,
                        opa: *opa,
                    },
                    clip,
                );
            }
        }
    }
    if stack.len() != 1 {
        return Err(ReplayError::UnbalancedGroup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::command::DrawCommand;
    use crate::types::{Color, Fixed, Point, Rect};
    use alloc::vec;

    struct CaptureRenderer {
        transforms: Vec<Transform>,
    }
    impl Renderer for CaptureRenderer {
        fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::Fill { transform, .. } = cmd {
                self.transforms.push(*transform);
            }
        }
        fn flush(&mut self) {}
    }

    struct NoResolver;
    impl SceneResolver for NoResolver {
        fn font(&self, _: &ResourceRef) -> Option<&Font> {
            None
        }
        fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
            None
        }
    }

    fn rect() -> Rect {
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(4),
            h: Fixed::from_int(4),
        }
    }

    fn fill(transform: Transform) -> SceneOp {
        SceneOp::FillRect {
            area: rect(),
            transform,
            quad: None,
            color: Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            radius: Fixed::ZERO,
            opa: 255,
        }
    }

    #[test]
    fn group_transform_composes_onto_child() {
        let group_tf = Transform::translate(Fixed::from_int(10), Fixed::ZERO);
        let child_tf = Transform::translate(Fixed::ZERO, Fixed::from_int(5));
        let ops = vec![
            SceneOp::GroupBegin {
                transform: Some(group_tf),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(child_tf),
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.transforms, vec![group_tf.compose(&child_tf)]);
    }

    #[test]
    fn op_outside_group_keeps_own_transform() {
        let child_tf = Transform::translate(Fixed::from_int(2), Fixed::from_int(3));
        let ops = vec![fill(child_tf)];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.transforms, vec![Transform::IDENTITY.compose(&child_tf)]);
    }

    #[test]
    fn unresolved_texture_errors() {
        let ops = vec![SceneOp::Blit {
            texture: ResourceRef::Index(0),
            pos: Point::ZERO,
            size: Point::ZERO,
            transform: Transform::IDENTITY,
            quad: None,
            opa: 255,
        }];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut r, &rect(), &NoResolver),
            Err(ReplayError::UnresolvedTexture)
        );
    }

    #[test]
    fn unbalanced_group_end_errors() {
        let ops = vec![SceneOp::GroupEnd];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut r, &rect(), &NoResolver),
            Err(ReplayError::UnbalancedGroup)
        );
    }

    #[test]
    fn nonzero_fill_rule_is_rejected() {
        let ops = vec![SceneOp::FillPath {
            path: alloc::borrow::Cow::Borrowed(&[]),
            transform: Transform::IDENTITY,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            opa: 0,
            fill_rule: FillRule::NonZero,
        }];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut r, &rect(), &NoResolver),
            Err(ReplayError::UnsupportedFillRule)
        );
    }
}
