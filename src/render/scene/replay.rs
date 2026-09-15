//! Replay an owned `SceneOp` stream back through a live `Renderer`.

use super::bbox::{
    BoundsError, BoundsWalkError, children_disjoint_with, op_bbox, union_of_children_with,
};
use super::{ResourceRef, SceneOp};
use crate::render::command::DrawCommand;
use crate::render::font::Font;
use crate::render::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};
use crate::render::texture::Texture;
use crate::types::{Fixed, Rect, Transform, Transform3D};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    UnbalancedGroup,
    Bounds(BoundsError),
    InsufficientWorkspace {
        required: usize,
        available: usize,
    },
    UnresolvedFont,
    UnresolvedTexture,
    UnresolvedClip,
    UnsupportedFillRule,
    /// Mid-range group opacity over overlapping children with no
    /// `disjoint_hint`. Flat alpha-multiply would seam; offscreen
    /// compositing isn't available. Separate the children, or set the
    /// hint to flatten with a visible seam.
    GroupOpacityNeedsOffscreen,
    Render(RenderError),
}

fn parse_blur_filter(filter: &str) -> Option<Fixed> {
    for part in filter.split(';') {
        if let Some(rest) = part.strip_prefix("blur:") {
            let std_dev = rest
                .split(':')
                .next()
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            if std_dev.is_finite() && std_dev > 0.0 {
                return Some(Fixed::from_f32(std_dev.min(64.0)));
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
pub struct ReplayFrame {
    transform: Transform,
    projective: Option<Transform3D>,
    alpha: u8,
    has_clip: bool,
    start_idx: usize,
}

impl ReplayFrame {
    pub const EMPTY: Self = Self {
        transform: Transform::IDENTITY,
        projective: None,
        alpha: 255,
        has_clip: false,
        start_idx: 0,
    };
}

/// One reusable exact route captured during scene preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayPlan {
    ordinal: usize,
    route: RenderRoute,
}

impl ReplayPlan {
    pub const EMPTY: Self = Self {
        ordinal: usize::MAX,
        route: RenderRoute::Native,
    };
}

struct ReplayStack<'a> {
    frames: &'a mut [ReplayFrame],
    len: usize,
}

impl<'a> ReplayStack<'a> {
    fn new(frames: &'a mut [ReplayFrame]) -> Result<Self, ReplayError> {
        if frames.is_empty() {
            return Err(ReplayError::InsufficientWorkspace {
                required: 1,
                available: 0,
            });
        }
        frames[0] = ReplayFrame::EMPTY;
        Ok(Self { frames, len: 1 })
    }

    fn top(&self) -> ReplayFrame {
        self.frames[self.len - 1]
    }

    fn push(&mut self, frame: ReplayFrame) -> Result<(), ReplayError> {
        if self.len == self.frames.len() {
            return Err(ReplayError::InsufficientWorkspace {
                required: self.len + 1,
                available: self.frames.len(),
            });
        }
        self.frames[self.len] = frame;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<ReplayFrame, ReplayError> {
        if self.len <= 1 {
            return Err(ReplayError::UnbalancedGroup);
        }
        self.len -= 1;
        Ok(self.frames[self.len])
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayPass {
    Preflight,
    Draw,
}

struct ReplayPassState<'a> {
    pass: ReplayPass,
    plans: &'a mut [ReplayPlan],
    plan_len: &'a mut usize,
    ordinal: usize,
    plan_cursor: usize,
}

impl ReplayPassState<'_> {
    fn route(
        &mut self,
        renderer: &mut dyn Renderer,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        let ordinal = self.ordinal;
        self.ordinal += 1;
        if self.pass == ReplayPass::Preflight {
            let route = renderer.route(request)?;
            if matches!(route, RenderRoute::ExactFallback(_)) && *self.plan_len < self.plans.len() {
                self.plans[*self.plan_len] = ReplayPlan { ordinal, route };
                *self.plan_len += 1;
            }
            return Ok(());
        }
        while self.plan_cursor < *self.plan_len && self.plans[self.plan_cursor].ordinal < ordinal {
            self.plan_cursor += 1;
        }
        if self.plan_cursor < *self.plan_len && self.plans[self.plan_cursor].ordinal == ordinal {
            let route = self.plans[self.plan_cursor].route;
            self.plan_cursor += 1;
            renderer.submit_with_route(request, route)
        } else {
            renderer.submit(request)
        }
    }
}

fn draw_in_frame(
    renderer: &mut dyn Renderer,
    frame: &ReplayFrame,
    command: &DrawCommand,
    clip: &Rect,
    state: &mut ReplayPassState<'_>,
) -> Result<(), ReplayError> {
    let request = DrawRequest::new(command, *clip)
        .with_projective(frame.projective.unwrap_or(Transform3D::IDENTITY));
    state.route(renderer, &request).map_err(ReplayError::Render)
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

fn resolved_leaf_bounds(
    op: &SceneOp,
    parent: Transform,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
) -> Result<Option<Rect>, ReplayError> {
    match op {
        SceneOp::GlyphRun {
            font,
            ppem,
            pos,
            transform,
            glyphs,
            ..
        } => {
            if glyphs.is_empty() {
                return Ok(None);
            }
            let mut font = resolver
                .font(font)
                .ok_or(ReplayError::UnresolvedFont)?
                .clone();
            font.size = *ppem;
            let transform = parent.compose(transform);
            let output_ppem = crate::render::font::output_ppem(
                font.size.max(1),
                output_scale * transform.raster_scale(),
            );
            font.glyph_run_ink_bounds(glyphs, *pos, transform, output_ppem)
                .map(Some)
                .ok_or(ReplayError::Bounds(BoundsError::GlyphInk))
        }
        SceneOp::PosedGlyphRun {
            font,
            ppem,
            pos,
            transform,
            glyphs,
            ..
        } => {
            if glyphs.glyphs().is_empty() {
                return Ok(None);
            }
            let mut font = resolver
                .font(font)
                .ok_or(ReplayError::UnresolvedFont)?
                .clone();
            font.size = *ppem;
            glyphs
                .as_draw()
                .ink_bounds(&font, *pos, parent.compose(transform), output_scale)
                .map(Some)
                .ok_or(ReplayError::Bounds(BoundsError::GlyphInk))
        }
        _ => op_bbox(op)
            .map(|bounds| bounds.map(|bounds| parent.apply_rect_bbox(bounds)))
            .map_err(ReplayError::Bounds),
    }
}

fn map_bounds_walk(error: BoundsWalkError<ReplayError>) -> ReplayError {
    match error {
        BoundsWalkError::Bounds(error) => ReplayError::Bounds(error),
        BoundsWalkError::Leaf(error) => error,
    }
}

fn resolved_children_disjoint(
    ops: &[SceneOp],
    parent: Transform,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
) -> Result<bool, ReplayError> {
    children_disjoint_with(ops, parent, &|op, parent| {
        resolved_leaf_bounds(op, parent, resolver, output_scale)
    })
    .map_err(map_bounds_walk)
}

fn resolved_children_union(
    ops: &[SceneOp],
    parent: Transform,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
) -> Result<Rect, ReplayError> {
    union_of_children_with(ops, parent, &|op, parent| {
        resolved_leaf_bounds(op, parent, resolver, output_scale)
    })
    .map_err(map_bounds_walk)
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

/// Replay with capacity for seven nested groups. Use
/// [`replay_scene_with_workspace`] for deeper scenes.
pub fn replay_scene(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
) -> Result<(), ReplayError> {
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut plans = [ReplayPlan::EMPTY; 8];
    replay_scene_with_scratch(ops, renderer, clip, resolver, &mut frames, &mut plans)
}

/// Replay with caller-owned group frames. One frame is needed for the root
/// and one more for each nested group.
pub fn replay_scene_with_workspace(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    frames: &mut [ReplayFrame],
) -> Result<(), ReplayError> {
    let mut plans = [ReplayPlan::EMPTY; 8];
    replay_scene_with_scratch(ops, renderer, clip, resolver, frames, &mut plans)
}

/// Replay with caller-owned group frames and exact-route slots. Exact routes
/// beyond `plans` are still validated before drawing and are recomputed only
/// when executed.
pub fn replay_scene_with_scratch(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    frames: &mut [ReplayFrame],
    plans: &mut [ReplayPlan],
) -> Result<(), ReplayError> {
    let mut plan_len = 0;
    replay_scene_pass(
        ops,
        renderer,
        clip,
        resolver,
        frames,
        &mut ReplayPassState {
            pass: ReplayPass::Preflight,
            plans,
            plan_len: &mut plan_len,
            ordinal: 0,
            plan_cursor: 0,
        },
    )?;
    replay_scene_pass(
        ops,
        renderer,
        clip,
        resolver,
        frames,
        &mut ReplayPassState {
            pass: ReplayPass::Draw,
            plans,
            plan_len: &mut plan_len,
            ordinal: 0,
            plan_cursor: 0,
        },
    )
}

fn replay_scene_pass(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    frames: &mut [ReplayFrame],
    state: &mut ReplayPassState<'_>,
) -> Result<(), ReplayError> {
    let mut stack = ReplayStack::new(frames)?;
    let mut skip_depth = 0usize;

    let mut i = 0;
    while i < ops.len() {
        let op = &ops[i];

        if skip_depth != 0 {
            match op {
                SceneOp::GroupBegin { .. } => skip_depth += 1,
                SceneOp::GroupEnd => skip_depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }

        let top = stack.top();
        match op {
            SceneOp::GroupBegin {
                transform,
                projective,
                opacity,
                clip: group_clip,
                disjoint_hint,
                ..
            } => {
                let local_affine = transform.unwrap_or(Transform::IDENTITY);
                let local_projective = projective.filter(|value| !value.is_identity());
                let (composed, composed_projective) = match (top.projective, local_projective) {
                    (None, None) => (top.transform.compose(&local_affine), None),
                    (Some(parent), local) => (
                        Transform::IDENTITY,
                        Some(
                            parent
                                .compose(local.as_ref().unwrap_or(&Transform3D::IDENTITY))
                                .compose(&Transform3D::from_affine(local_affine)),
                        ),
                    ),
                    (None, Some(local)) => (
                        Transform::IDENTITY,
                        Some(
                            Transform3D::from_affine(top.transform)
                                .compose(&local)
                                .compose(&Transform3D::from_affine(local_affine)),
                        ),
                    ),
                };
                let next_alpha = match opacity {
                    None => top.alpha,
                    Some(255) => top.alpha,
                    Some(0) => {
                        skip_depth = 1;
                        i += 1;
                        continue;
                    }
                    Some(n) => {
                        if !*disjoint_hint {
                            let end_idx =
                                matching_group_end(ops, i).ok_or(ReplayError::UnbalancedGroup)?;
                            let inner = &ops[i + 1..end_idx];
                            if !resolved_children_disjoint(
                                inner,
                                composed,
                                resolver,
                                renderer.output_scale(),
                            )? {
                                return Err(ReplayError::GroupOpacityNeedsOffscreen);
                            }
                        }
                        mul_alpha(top.alpha, *n)
                    }
                };
                let has_clip = if let Some(ResourceRef::Inline(path)) = group_clip {
                    draw_in_frame(
                        renderer,
                        &ReplayFrame {
                            transform: composed,
                            projective: composed_projective,
                            alpha: next_alpha,
                            has_clip: false,
                            start_idx: i,
                        },
                        &DrawCommand::PushClip {
                            path,
                            transform: composed,
                            fill_rule: crate::render::raster::FillRule::EvenOdd,
                        },
                        clip,
                        state,
                    )?;
                    true
                } else if group_clip.is_some() {
                    return Err(ReplayError::UnresolvedClip);
                } else {
                    false
                };
                stack.push(ReplayFrame {
                    transform: composed,
                    projective: composed_projective,
                    alpha: next_alpha,
                    has_clip,
                    start_idx: i,
                })?;
            }
            SceneOp::GroupEnd => {
                let frame = stack.pop()?;
                if frame.has_clip {
                    draw_in_frame(renderer, &frame, &DrawCommand::PopClip, clip, state)?;
                }
                let filter = match &ops[frame.start_idx] {
                    SceneOp::GroupBegin { filter, .. } => filter,
                    _ => return Err(ReplayError::UnbalancedGroup),
                };
                if let Some(ResourceRef::Token(filter_str)) = filter {
                    if let Some(blur_radius) = parse_blur_filter(filter_str) {
                        if frame.projective.is_some() {
                            return Err(ReplayError::Bounds(BoundsError::ProjectiveGroup));
                        }
                        let children = &ops[frame.start_idx + 1..i];
                        let region = resolved_children_union(
                            children,
                            frame.transform,
                            resolver,
                            renderer.output_scale(),
                        )?
                        .inflate(blur_radius * Fixed::from_int(4));
                        let alpha = crate::render::backends::sw::blur::alpha_for_radius(
                            blur_radius * renderer.output_scale(),
                        );
                        draw_in_frame(
                            renderer,
                            &frame,
                            &DrawCommand::ApplyBlur { alpha, region },
                            clip,
                            state,
                        )?;
                    }
                }
            }
            SceneOp::PushClip {
                path,
                transform,
                fill_rule,
            } => {
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::PushClip {
                        path,
                        transform: top.transform.compose(transform),
                        fill_rule: *fill_rule,
                    },
                    clip,
                    state,
                )?;
            }
            SceneOp::PopClip => {
                draw_in_frame(renderer, &top, &DrawCommand::PopClip, clip, state)?;
            }
            SceneOp::FillRect {
                area,
                transform,
                quad,
                color,
                radius,
                opa,
            } => draw_in_frame(
                renderer,
                &top,
                &DrawCommand::Fill {
                    area: *area,
                    transform: top.transform.compose(transform),
                    quad: *quad,
                    color: *color,
                    radius: *radius,
                    opa: mul_alpha(*opa, top.alpha),
                },
                clip,
                state,
            )?,
            SceneOp::Border {
                area,
                transform,
                quad,
                color,
                width,
                radius,
                opa,
            } => draw_in_frame(
                renderer,
                &top,
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
                state,
            )?,
            SceneOp::GlyphRun {
                font,
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
            } => {
                let mut font = resolver
                    .font(font)
                    .ok_or(ReplayError::UnresolvedFont)?
                    .clone();
                font.size = *ppem;
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::GlyphRun {
                        pos: *pos,
                        transform: top.transform.compose(transform),
                        glyphs,
                        font: &font,
                        color: *color,
                        opa: mul_alpha(*opa, top.alpha),
                    },
                    clip,
                    state,
                )?;
            }
            SceneOp::PosedGlyphRun {
                font,
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
            } => {
                let mut font = resolver
                    .font(font)
                    .ok_or(ReplayError::UnresolvedFont)?
                    .clone();
                font.size = *ppem;
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::PosedGlyphRun {
                        pos: *pos,
                        transform: top.transform.compose(transform),
                        glyphs: glyphs.as_draw(),
                        font: &font,
                        color: *color,
                        opa: mul_alpha(*opa, top.alpha),
                    },
                    clip,
                    state,
                )?;
            }
            SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => draw_in_frame(
                renderer,
                &top,
                &DrawCommand::Line {
                    p1: *p1,
                    p2: *p2,
                    transform: top.transform.compose(transform),
                    color: *color,
                    width: *width,
                    opa: mul_alpha(*opa, top.alpha),
                },
                clip,
                state,
            )?,
            SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => draw_in_frame(
                renderer,
                &top,
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
                state,
            )?,
            SceneOp::Blit {
                texture,
                pos,
                size,
                transform,
                quad,
                opa,
                radius,
                composite,
            } => {
                let texture = resolver
                    .texture(texture)
                    .ok_or(ReplayError::UnresolvedTexture)?;
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::Blit {
                        pos: *pos,
                        size: *size,
                        transform: top.transform.compose(transform),
                        quad: *quad,
                        texture,
                        opa: mul_alpha(*opa, top.alpha),
                        radius: *radius,
                        composite: *composite,
                    },
                    clip,
                    state,
                )?;
            }
            SceneOp::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
            } => {
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::FillPath {
                        path,
                        transform: top.transform.compose(transform),
                        paint,
                        opa: mul_alpha(*opa, top.alpha),
                        fill_rule: *fill_rule,
                    },
                    clip,
                    state,
                )?;
            }
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
            } => {
                draw_in_frame(
                    renderer,
                    &top,
                    &DrawCommand::StrokePath {
                        path,
                        transform: top.transform.compose(transform),
                        paint,
                        width: *width,
                        opa: mul_alpha(*opa, top.alpha),
                        line_cap: *line_cap,
                        line_join: *line_join,
                        miter_limit: *miter_limit,
                        dash,
                    },
                    clip,
                    state,
                )?;
            }
        }
        i += 1;
    }
    if stack.len != 1 || skip_depth != 0 {
        return Err(ReplayError::UnbalancedGroup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::command::DrawCommand;
    use crate::render::renderer::{
        FallbackRegion, ProjectiveDrawError, RenderFeature, RenderResource, RenderRoute,
    };
    use crate::render::scene::Paint;
    use crate::types::{Color, Fixed, Point, Rect};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::Cell;

    #[test]
    fn replay_reuses_exact_routes_from_caller_scratch() {
        struct RoutedCapture {
            route_calls: Cell<usize>,
            plain_submits: usize,
            routed_submits: usize,
        }

        impl Renderer for RoutedCapture {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                self.route_calls.set(self.route_calls.get() + 1);
                Ok(RenderRoute::ExactFallback(FallbackRegion::from_parts(
                    3, 4, 5, 6, 20,
                )))
            }

            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.plain_submits += 1;
                Ok(())
            }

            fn submit_with_route(
                &mut self,
                _: &DrawRequest<'_, '_>,
                route: RenderRoute,
            ) -> Result<(), RenderError> {
                assert!(matches!(route, RenderRoute::ExactFallback(_)));
                self.routed_submits += 1;
                Ok(())
            }

            fn flush(&mut self) {}
        }

        let ops = [SceneOp::FillRect {
            area: Rect::new(0, 0, 5, 6),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(1, 2, 3),
            radius: Fixed::ZERO,
            opa: 255,
        }];
        let mut renderer = RoutedCapture {
            route_calls: Cell::new(0),
            plain_submits: 0,
            routed_submits: 0,
        };
        let mut frames = [ReplayFrame::EMPTY; 1];
        let mut plans = [ReplayPlan::EMPTY; 1];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 10, 10),
            &NoResolver,
            &mut frames,
            &mut plans,
        )
        .unwrap();

        assert_eq!(renderer.route_calls.get(), 1);
        assert_eq!(renderer.plain_submits, 0);
        assert_eq!(renderer.routed_submits, 1);
        assert!(core::mem::size_of::<ReplayPlan>() <= 40);
    }

    #[test]
    fn scene_blur_uses_physical_radius_and_includes_edge_bleed() {
        struct BlurCapture(Option<(Fixed, Rect)>);

        impl Renderer for BlurCapture {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request)?;
                if let DrawCommand::ApplyBlur { alpha, region } = request.command {
                    self.0 = Some((*alpha, *region));
                }
                Ok(())
            }

            fn output_scale(&self) -> Fixed {
                Fixed::from_int(2)
            }

            fn flush(&mut self) {}
        }

        let ops = [
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: None,
                clip: None,
                mask: None,
                filter: Some(ResourceRef::Token("blur:3:3".into())),
                disjoint_hint: true,
            },
            fill(Transform::IDENTITY),
            SceneOp::GroupEnd,
        ];
        let mut renderer = BlurCapture(None);
        replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 100, 100), &NoResolver).unwrap();
        assert_eq!(
            renderer.0,
            Some((
                crate::render::backends::sw::blur::alpha_for_radius(Fixed::from_int(6)),
                Rect::new(-12, -12, 28, 28),
            ))
        );
    }

    #[test]
    fn scene_blur_resolves_glyph_ink_instead_of_using_the_clip() {
        struct BlurCapture(Option<Rect>);

        impl Renderer for BlurCapture {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                if let DrawCommand::ApplyBlur { region, .. } = request.command {
                    self.0 = Some(*region);
                }
                Ok(())
            }

            fn flush(&mut self) {}
        }

        let ops = [
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: None,
                clip: None,
                mask: None,
                filter: Some(ResourceRef::Token("blur:2".into())),
                disjoint_hint: true,
            },
            glyph_at(30),
            SceneOp::GroupEnd,
        ];
        let mut renderer = BlurCapture(None);

        replay_scene(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 200, 100),
            &BitmapResolver(Font::bitmap_8x8()),
        )
        .unwrap();

        let region = renderer.0.expect("blur command");
        assert!(region.x > Fixed::ZERO);
        assert!(region.w < Fixed::from_int(100));
        assert!(region.x <= Fixed::from_int(30));
        assert!(region.y <= Fixed::from_int(20));
        assert!(region.x + region.w >= Fixed::from_int(30));
        assert!(region.y + region.h >= Fixed::from_int(20));
    }

    #[test]
    fn group_overlap_uses_resolved_glyph_ink() {
        let ops = [glyph_at(10), glyph_at(12)];
        assert!(
            !resolved_children_disjoint(
                &ops,
                Transform::IDENTITY,
                &BitmapResolver(Font::bitmap_8x8()),
                Fixed::ONE,
            )
            .unwrap()
        );
    }

    struct CaptureRenderer {
        transforms: Vec<Transform>,
        fill_opas: Vec<u8>,
    }
    impl Renderer for CaptureRenderer {
        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.route(request)?;
            if let DrawCommand::Fill { transform, opa, .. } = request.command {
                self.transforms.push(*transform);
                self.fill_opas.push(*opa);
            }
            Ok(())
        }
        fn flush(&mut self) {}
    }

    #[derive(Default)]
    struct ProjectiveCapture {
        command_transform: Option<Transform>,
        scope: Option<Transform3D>,
    }

    impl Renderer for ProjectiveCapture {
        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.route(request)?;
            self.command_transform = Some(request.command.transform());
            self.scope = Some(request.projective);
            Ok(())
        }

        fn flush(&mut self) {}
    }

    struct FillOnlyProjectiveRenderer {
        draws: usize,
        line_error: ProjectiveDrawError,
    }

    impl Default for FillOnlyProjectiveRenderer {
        fn default() -> Self {
            Self {
                draws: 0,
                line_error: ProjectiveDrawError::Unsupported,
            }
        }
    }

    impl Renderer for FillOnlyProjectiveRenderer {
        fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            if !request.projective.is_identity()
                && !matches!(request.command, DrawCommand::Fill { .. })
            {
                return Err(RenderError::from(self.line_error));
            }
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.route(request)?;
            self.draws += 1;
            Ok(())
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

    struct BitmapResolver(Font);

    impl SceneResolver for BitmapResolver {
        fn font(&self, _: &ResourceRef) -> Option<&Font> {
            Some(&self.0)
        }

        fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
            None
        }
    }

    static BOUNDS_GLYPH: [textflow::shaping::PositionedGlyph; 1] =
        [textflow::shaping::PositionedGlyph::new(
            crate::render::font::GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 0 },
        )];

    fn glyph_at(x: i32) -> SceneOp {
        SceneOp::GlyphRun {
            font: ResourceRef::Index(0),
            ppem: 16,
            pos: Point::new(x, 20),
            transform: Transform::IDENTITY,
            color: Color::rgb(255, 255, 255),
            opa: 255,
            glyphs: (&BOUNDS_GLYPH[..]).into(),
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
                projective: None,
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
    fn group_workspace_fails_before_the_first_draw() {
        let group = SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        };
        let ops = [
            fill(Transform::IDENTITY),
            group.clone(),
            group,
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        let mut renderer = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        let mut short = [ReplayFrame::EMPTY; 2];
        assert_eq!(
            replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut short),
            Err(ReplayError::InsufficientWorkspace {
                required: 3,
                available: 2,
            })
        );
        assert!(renderer.transforms.is_empty());

        let mut enough = [ReplayFrame::EMPTY; 3];
        replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut enough)
            .unwrap();
        assert_eq!(renderer.transforms.len(), 1);
    }

    #[test]
    fn invisible_groups_do_not_consume_workspace_frames() {
        let ops = [
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: Some(0),
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(Transform::IDENTITY),
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        let mut renderer = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        let mut frames = [ReplayFrame::EMPTY; 1];
        replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut frames)
            .unwrap();
        assert!(renderer.transforms.is_empty());
    }

    #[test]
    fn projective_group_keeps_one_scope_and_leaf_affine() {
        let group_affine = Transform::translate(Fixed::from_int(10), Fixed::ZERO);
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
        let child_affine = Transform::translate(Fixed::ZERO, Fixed::from_int(5));
        let ops = vec![
            SceneOp::GroupBegin {
                transform: Some(group_affine),
                projective: Some(projective),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(child_affine),
            SceneOp::GroupEnd,
        ];
        let mut renderer = ProjectiveCapture::default();

        replay_scene(&ops, &mut renderer, &rect(), &NoResolver).unwrap();

        assert_eq!(renderer.command_transform, Some(child_affine));
        assert_eq!(
            renderer.scope,
            Some(projective.compose(&Transform3D::from_affine(group_affine)))
        );
    }

    #[test]
    fn projective_capabilities_are_checked_before_the_first_draw() {
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
        let ops = vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(projective),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(Transform::IDENTITY),
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::new(Fixed::ONE, Fixed::ONE),
                transform: Transform::IDENTITY,
                color: Color::rgb(255, 255, 255),
                width: Fixed::ONE,
                opa: 255,
            },
            SceneOp::GroupEnd,
        ];
        let mut renderer = FillOnlyProjectiveRenderer::default();

        assert_eq!(
            replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
            Err(ReplayError::Render(RenderError::Unsupported(
                RenderFeature::ProjectiveGeometry
            )))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn affine_route_failure_prevents_earlier_draws() {
        struct FillOnlyRoute {
            draws: usize,
        }

        impl Renderer for FillOnlyRoute {
            fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                if matches!(request.command, DrawCommand::Line { .. }) {
                    Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
                } else {
                    Ok(RenderRoute::Native)
                }
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request)?;
                self.draws += 1;
                Ok(())
            }

            fn flush(&mut self) {}
        }

        let ops = [
            fill(Transform::IDENTITY),
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::new(Fixed::ONE, Fixed::ONE),
                transform: Transform::IDENTITY,
                color: Color::rgb(255, 255, 255),
                width: Fixed::ONE,
                opa: 255,
            },
        ];
        let mut renderer = FillOnlyRoute { draws: 0 };
        assert_eq!(
            replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
            Err(ReplayError::Render(RenderError::Unsupported(
                RenderFeature::AffineGeometry
            )))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn execution_failure_reaches_scene_caller() {
        struct ExhaustedRenderer;

        impl Renderer for ExhaustedRenderer {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Err(RenderError::ResourceLimit(RenderResource::Uniforms))
            }

            fn flush(&mut self) {}
        }

        assert_eq!(
            replay_scene(
                &[fill(Transform::IDENTITY)],
                &mut ExhaustedRenderer,
                &rect(),
                &NoResolver
            ),
            Err(ReplayError::Render(RenderError::ResourceLimit(
                RenderResource::Uniforms
            )))
        );
    }

    #[test]
    fn invalid_projective_geometry_is_distinct_and_failure_atomic() {
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
        let ops = vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(projective),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(Transform::IDENTITY),
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::new(Fixed::ONE, Fixed::ONE),
                transform: Transform::IDENTITY,
                color: Color::rgb(255, 255, 255),
                width: Fixed::ONE,
                opa: 255,
            },
            SceneOp::GroupEnd,
        ];
        let mut renderer = FillOnlyProjectiveRenderer {
            line_error: ProjectiveDrawError::InvalidProjection,
            ..Default::default()
        };

        assert_eq!(
            replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
            Err(ReplayError::Render(RenderError::InvalidGeometry))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn fallback_capacity_error_is_distinct_and_failure_atomic() {
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
        let ops = vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(projective),
                opacity: None,
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            fill(Transform::IDENTITY),
            SceneOp::Line {
                p1: Point::ZERO,
                p2: Point::new(Fixed::ONE, Fixed::ONE),
                transform: Transform::IDENTITY,
                color: Color::rgb(255, 255, 255),
                width: Fixed::ONE,
                opa: 255,
            },
            SceneOp::GroupEnd,
        ];
        let mut renderer = FillOnlyProjectiveRenderer {
            line_error: ProjectiveDrawError::InsufficientFallbackStorage {
                required_bytes: 64,
                capacity_bytes: 32,
            },
            ..Default::default()
        };

        assert_eq!(
            replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
            Err(ReplayError::Render(RenderError::InsufficientWorkspace {
                required_bytes: 64,
                capacity_bytes: 32,
            }))
        );
        assert_eq!(renderer.draws, 0);
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
            radius: crate::types::Fixed::ZERO,
            composite: crate::render::command::CompositeMode::SourceOver,
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
            path: crate::render::path::Path::new(),
            transform: Transform::IDENTITY,
            paint: Paint::Color(mirx::types::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
            opa: 0,
            fill_rule: crate::render::raster::FillRule::NonZero,
        }];
        let mut r = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        assert!(replay_scene(&ops, &mut r, &rect(), &NoResolver).is_ok());
    }

    #[test]
    fn replay_preserves_positioned_glyphs() {
        struct GlyphRenderer {
            glyphs: usize,
            ppem: u16,
        }
        impl Renderer for GlyphRenderer {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request)?;
                if let DrawCommand::GlyphRun { glyphs, font, .. } = request.command {
                    self.glyphs = glyphs.len();
                    self.ppem = font.size;
                }
                Ok(())
            }

            fn flush(&mut self) {}
        }

        struct FontResolver(Font);
        impl SceneResolver for FontResolver {
            fn font(&self, _: &ResourceRef) -> Option<&Font> {
                Some(&self.0)
            }

            fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
                None
            }
        }

        static GLYPHS: [textflow::shaping::PositionedGlyph; 1] =
            [textflow::shaping::PositionedGlyph::new(
                crate::render::font::GlyphId::new(65),
                textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
            )];
        let ops = [SceneOp::GlyphRun {
            font: ResourceRef::Index(0),
            ppem: 19,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: Color::rgb(1, 2, 3),
            opa: 255,
            glyphs: (&GLYPHS[..]).into(),
        }];
        let mut renderer = GlyphRenderer { glyphs: 0, ppem: 0 };

        replay_scene(
            &ops,
            &mut renderer,
            &rect(),
            &FontResolver(Font::bitmap_8x8()),
        )
        .unwrap();

        assert_eq!(renderer.glyphs, 1);
        assert_eq!(renderer.ppem, 19);
    }

    #[test]
    fn replay_preserves_posed_glyph_geometry() {
        struct GlyphRenderer {
            origin: Option<textflow::shaping::FlowPoint>,
            tangent: Option<textflow::shaping::FlowPoint>,
        }
        impl Renderer for GlyphRenderer {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                self.route(request)?;
                if let DrawCommand::PosedGlyphRun { glyphs, .. } = request.command {
                    self.origin = glyphs.frames().first().map(|frame| frame.local_origin);
                    self.tangent = glyphs.frames().first().map(|frame| frame.unit_tangent);
                }
                Ok(())
            }

            fn flush(&mut self) {}
        }

        struct FontResolver(Font);
        impl SceneResolver for FontResolver {
            fn font(&self, _: &ResourceRef) -> Option<&Font> {
                Some(&self.0)
            }

            fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
                None
            }
        }

        let origin = textflow::shaping::FlowPoint {
            x: 6 << 8,
            y: 9 << 8,
        };
        let tangent = textflow::shaping::FlowPoint { x: 0, y: 1 << 8 };
        let glyphs = super::super::PosedGlyphBuffer::new(
            vec![textflow::shaping::PositionedGlyph::new(
                crate::render::font::GlyphId::new(65),
                textflow::shaping::FlowPoint { x: 0, y: 0 },
            )],
            vec![textflow::placement::GlyphFrame {
                local_origin: origin,
                unit_tangent: tangent,
            }],
        )
        .unwrap();
        let ops = [SceneOp::PosedGlyphRun {
            font: ResourceRef::Index(0),
            ppem: 19,
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            color: Color::rgb(1, 2, 3),
            opa: 255,
            glyphs,
        }];
        let mut renderer = GlyphRenderer {
            origin: None,
            tangent: None,
        };

        replay_scene(
            &ops,
            &mut renderer,
            &rect(),
            &FontResolver(Font::bitmap_8x8()),
        )
        .unwrap();

        assert_eq!(renderer.origin, Some(origin));
        assert_eq!(renderer.tangent, Some(tangent));
    }

    fn group(opa: Option<u8>, hint: bool) -> SceneOp {
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
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
    fn transformed_child_overlap_fails_before_any_draw() {
        let mut nested = group(None, false);
        if let SceneOp::GroupBegin { transform, .. } = &mut nested {
            *transform = Some(Transform::translate(Fixed::from_int(20), Fixed::ZERO));
        }
        let ops = vec![
            group(Some(128), false),
            nested,
            opaque_fill_at(0, 0),
            SceneOp::GroupEnd,
            opaque_fill_at(22, 0),
            SceneOp::GroupEnd,
        ];
        let mut renderer = CaptureRenderer {
            transforms: Vec::new(),
            fill_opas: Vec::new(),
        };
        assert_eq!(
            replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
            Err(ReplayError::GroupOpacityNeedsOffscreen)
        );
        assert!(renderer.fill_opas.is_empty());
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
