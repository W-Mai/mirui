//! Replay an owned `SceneOp` stream back through a live `Renderer`.

use alloc::vec::Vec;

use super::bbox::{direct_children_bboxes, pairwise_disjoint};
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
    /// Mid-range group opacity over overlapping children with no
    /// `disjoint_hint`. Flat alpha-multiply would seam; offscreen
    /// compositing isn't available. Separate the children, or set the
    /// hint to flatten with a visible seam.
    GroupOpacityNeedsOffscreen,
}

#[derive(Clone, Copy)]
struct GroupFrame {
    transform: Transform,
    alpha: u8,
}

/// Resolves a persisted `ResourceRef` back to a live borrow for the duration
/// of one draw call.
pub trait SceneResolver {
    fn font(&self, r: &ResourceRef) -> Option<&Font>;
    fn texture(&self, r: &ResourceRef) -> Option<&Texture<'_>>;
}

fn mul_alpha(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16) / 255) as u8
}

/// Find the index of the matching `GroupEnd` for the `GroupBegin` at `start`.
fn matching_group_end(ops: &[SceneOp], start: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = start + 1;
    while i < ops.len() {
        match &ops[i] {
            SceneOp::GroupBegin { .. } => depth += 1,
            SceneOp::GroupEnd => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub fn replay_scene(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
) -> Result<(), ReplayError> {
    let mut stack: Vec<GroupFrame> = alloc::vec![GroupFrame {
        transform: Transform::IDENTITY,
        alpha: 255,
    }];
    let mut skip_until_depth: Option<usize> = None;

    let mut i = 0;
    while i < ops.len() {
        let op = &ops[i];

        if let Some(target_depth) = skip_until_depth {
            match op {
                SceneOp::GroupBegin { .. } => {
                    stack.push(*stack.last().unwrap());
                }
                SceneOp::GroupEnd => {
                    if stack.len() <= 1 {
                        return Err(ReplayError::UnbalancedGroup);
                    }
                    stack.pop();
                    if stack.len() == target_depth {
                        skip_until_depth = None;
                    }
                }
                _ => {}
            }
            i += 1;
            continue;
        }

        let top = *stack.last().ok_or(ReplayError::UnbalancedGroup)?;
        match op {
            SceneOp::GroupBegin {
                transform,
                opacity,
                disjoint_hint,
                ..
            } => {
                let composed = match transform {
                    Some(t) => top.transform.compose(t),
                    None => top.transform,
                };
                let next_alpha = match opacity {
                    None => top.alpha,
                    Some(255) => top.alpha,
                    Some(0) => {
                        skip_until_depth = Some(stack.len());
                        stack.push(GroupFrame {
                            transform: composed,
                            alpha: 0,
                        });
                        i += 1;
                        continue;
                    }
                    Some(n) => {
                        if !*disjoint_hint {
                            let end_idx =
                                matching_group_end(ops, i).ok_or(ReplayError::UnbalancedGroup)?;
                            let inner = &ops[i + 1..end_idx];
                            let bboxes = direct_children_bboxes(inner);
                            if !pairwise_disjoint(&bboxes) {
                                return Err(ReplayError::GroupOpacityNeedsOffscreen);
                            }
                        }
                        mul_alpha(top.alpha, *n)
                    }
                };
                stack.push(GroupFrame {
                    transform: composed,
                    alpha: next_alpha,
                });
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
                    transform: top.transform.compose(transform),
                    quad: *quad,
                    color: *color,
                    radius: *radius,
                    opa: mul_alpha(*opa, top.alpha),
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
                    transform: top.transform.compose(transform),
                    quad: *quad,
                    color: *color,
                    width: *width,
                    radius: *radius,
                    opa: mul_alpha(*opa, top.alpha),
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
                        transform: top.transform.compose(transform),
                        text: text.as_bytes(),
                        font,
                        color: *color,
                        opa: mul_alpha(*opa, top.alpha),
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
                    transform: top.transform.compose(transform),
                    color: *color,
                    width: *width,
                    opa: mul_alpha(*opa, top.alpha),
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
                    transform: top.transform.compose(transform),
                    radius: *radius,
                    start_angle: *start_angle,
                    end_angle: *end_angle,
                    color: *color,
                    width: *width,
                    opa: mul_alpha(*opa, top.alpha),
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
                        transform: top.transform.compose(transform),
                        quad: *quad,
                        texture,
                        opa: mul_alpha(*opa, top.alpha),
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
                let p = Path { cmds: path.clone() };
                renderer.draw(
                    &DrawCommand::FillPath {
                        path: &p,
                        transform: top.transform.compose(transform),
                        color: *color,
                        opa: mul_alpha(*opa, top.alpha),
                    },
                    clip,
                );
            }
        }
        i += 1;
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
        fill_opas: Vec<u8>,
    }
    impl Renderer for CaptureRenderer {
        fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::Fill { transform, opa, .. } = cmd {
                self.transforms.push(*transform);
                self.fill_opas.push(*opa);
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
            fill_opas: Vec::new(),
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
            fill_opas: Vec::new(),
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
            fill_opas: Vec::new(),
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
            fill_opas: Vec::new(),
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
            fill_opas: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut r, &rect(), &NoResolver),
            Err(ReplayError::UnsupportedFillRule)
        );
    }

    fn group(opa: Option<u8>, hint: bool) -> SceneOp {
        SceneOp::GroupBegin {
            transform: None,
            opacity: opa,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: hint,
        }
    }

    fn opaque_fill_at(x: i32, y: i32) -> SceneOp {
        SceneOp::FillRect {
            area: Rect {
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
                w: Fixed::from_int(4),
                h: Fixed::from_int(4),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            radius: Fixed::ZERO,
            opa: 255,
        }
    }

    #[test]
    fn group_opacity_zero_skips_subtree() {
        let ops = vec![
            group(Some(0), false),
            opaque_fill_at(0, 0),
            opaque_fill_at(20, 0),
            SceneOp::GroupEnd,
            opaque_fill_at(40, 0),
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.fill_opas, vec![255]);
    }

    #[test]
    fn group_opacity_255_is_passthrough() {
        let ops = vec![
            group(Some(255), false),
            opaque_fill_at(0, 0),
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.fill_opas, vec![255]);
    }

    #[test]
    fn group_opacity_disjoint_children_multiplies_into_each() {
        let ops = vec![
            group(Some(128), false),
            opaque_fill_at(0, 0),
            opaque_fill_at(20, 0),
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.fill_opas, vec![128, 128]);
    }

    #[test]
    fn group_opacity_overlap_without_hint_errors() {
        let ops = vec![
            group(Some(128), false),
            opaque_fill_at(0, 0),
            opaque_fill_at(2, 2),
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut r, &rect(), &NoResolver),
            Err(ReplayError::GroupOpacityNeedsOffscreen)
        );
    }

    #[test]
    fn group_opacity_overlap_with_hint_forces_flat() {
        let ops = vec![
            group(Some(128), true),
            opaque_fill_at(0, 0),
            opaque_fill_at(2, 2),
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.fill_opas, vec![128, 128]);
    }

    #[test]
    fn nested_group_opacity_multiplies() {
        let ops = vec![
            group(Some(200), false),
            group(Some(128), false),
            opaque_fill_at(0, 0),
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
        assert_eq!(r.fill_opas, vec![(200u16 * 128 / 255) as u8]);
    }
}
