//! Replay an owned `SceneOp` stream back through a live `Renderer`.

use super::bbox::{BoundsError, op_bbox};
use super::{ResourceRef, SceneOp};
use crate::render::command::DrawCommand;
use crate::render::font::Font;
use crate::render::renderer::{
    DrawRequest, FallbackRegion, RenderError, RenderFeature, RenderRoute, Renderer,
};
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
use core::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    UnbalancedGroup,
    Bounds(BoundsError),
    InsufficientWorkspace {
        required: usize,
        available: usize,
    },
    InsufficientScopePlans {
        required: usize,
        available: usize,
    },
    UnresolvedFont,
    UnresolvedTexture,
    UnresolvedClip,
    UnsupportedFillRule,
    /// Mid-range group opacity over overlapping children needs borrowed RGBA
    /// storage. Supply it through [`ReplayScratch::with_rgba`].
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
    visual_bounds: Option<Rect>,
    blur_support: Fixed,
}

impl ReplayFrame {
    pub const EMPTY: Self = Self {
        transform: Transform::IDENTITY,
        projective: None,
        alpha: 255,
        has_clip: false,
        visual_bounds: None,
        blur_support: Fixed::ZERO,
    };

    fn include_visual_bounds(&mut self, bounds: Rect) {
        self.visual_bounds = Some(match self.visual_bounds {
            Some(current) => current.union(&bounds),
            None => bounds,
        });
    }
}

/// One reusable exact route captured during scene preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayPlan {
    ordinal: usize,
    route: RenderRoute,
}

/// One ordered fallback scope retained during scene preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayScopePlan {
    start_idx: usize,
    end_idx: usize,
    region: FallbackRegion,
    kind: ReplayScopeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayScopeKind {
    Fallback,
    Isolated {
        opacity: u8,
        blur_alpha: Option<Fixed>,
    },
}

impl ReplayScopePlan {
    pub const EMPTY: Self = Self {
        start_idx: usize::MAX,
        end_idx: usize::MAX,
        region: FallbackRegion::EMPTY,
        kind: ReplayScopeKind::Fallback,
    };
}

/// Caller-owned storage used by the two-pass scene replay.
pub struct ReplayScratch<'a> {
    frames: &'a mut [ReplayFrame],
    routes: &'a mut [ReplayPlan],
    scopes: &'a mut [ReplayScopePlan],
    rgba: &'a mut [u8],
}

impl<'a> ReplayScratch<'a> {
    pub fn new(
        frames: &'a mut [ReplayFrame],
        routes: &'a mut [ReplayPlan],
        scopes: &'a mut [ReplayScopePlan],
    ) -> Self {
        Self {
            frames,
            routes,
            scopes,
            rgba: &mut [],
        }
    }

    /// Supply fixed-capacity RGBA storage for isolated scene groups.
    pub fn with_rgba(mut self, rgba: &'a mut [u8]) -> Self {
        self.rgba = rgba;
        self
    }
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
    fn new(frames: &'a mut [ReplayFrame], root: ReplayFrame) -> Result<Self, ReplayError> {
        if frames.is_empty() {
            return Err(ReplayError::InsufficientWorkspace {
                required: 1,
                available: 0,
            });
        }
        frames[0] = root;
        Ok(Self { frames, len: 1 })
    }

    fn top(&self) -> ReplayFrame {
        self.frames[self.len - 1]
    }

    fn top_mut(&mut self) -> &mut ReplayFrame {
        &mut self.frames[self.len - 1]
    }

    fn has_active_clip(&self) -> bool {
        self.frames[..self.len].iter().any(|frame| frame.has_clip)
    }

    fn workspace_from_top(&mut self) -> &mut [ReplayFrame] {
        &mut self.frames[self.len - 1..]
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

struct ScopeProbe<'a> {
    renderer: &'a dyn Renderer,
    needs_scope: Cell<bool>,
    error: Cell<Option<RenderError>>,
}

impl Renderer for ScopeProbe<'_> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        match self.renderer.route(request) {
            Ok(RenderRoute::Native) => {}
            Ok(RenderRoute::ExactFallback(_)) | Err(RenderError::Unsupported(_)) => {
                self.needs_scope.set(true);
            }
            Err(error) => {
                if self.error.get().is_none() {
                    self.error.set(Some(error));
                }
            }
        }
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        Err(RenderError::BackendFailure)
    }

    fn flush(&mut self) {}

    fn output_scale(&self) -> Fixed {
        self.renderer.output_scale()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayPass {
    Preflight,
    Draw,
}

struct ReplayPassState<'a> {
    pass: ReplayPass,
    probe_only: bool,
    plans: &'a mut [ReplayPlan],
    plan_len: &'a mut usize,
    ordinal: usize,
    plan_cursor: usize,
    scopes: &'a mut [ReplayScopePlan],
    scope_len: &'a mut usize,
    nested_scope_required: &'a mut usize,
    scope_cursor: usize,
    rgba: Option<&'a mut [u8]>,
    rgba_capacity: usize,
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

    fn retain_scope(
        &mut self,
        start_idx: usize,
        end_idx: usize,
        region: FallbackRegion,
        kind: ReplayScopeKind,
    ) -> Result<(), ReplayError> {
        if *self.scope_len == self.scopes.len() {
            return Err(ReplayError::InsufficientScopePlans {
                required: *self.scope_len + 1,
                available: self.scopes.len(),
            });
        }
        self.scopes[*self.scope_len] = ReplayScopePlan {
            start_idx,
            end_idx,
            region,
            kind,
        };
        *self.scope_len += 1;
        Ok(())
    }

    fn require_nested_scopes(&mut self, required: usize) {
        *self.nested_scope_required = (*self.nested_scope_required).max(required);
    }

    fn scope_at(&mut self, start_idx: usize) -> Option<ReplayScopePlan> {
        while self.scope_cursor < *self.scope_len
            && self.scopes[self.scope_cursor].start_idx < start_idx
        {
            self.scope_cursor += 1;
        }
        if self.scope_cursor < *self.scope_len
            && self.scopes[self.scope_cursor].start_idx == start_idx
        {
            let scope = self.scopes[self.scope_cursor];
            self.scope_cursor += 1;
            Some(scope)
        } else {
            None
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

fn resolved_children_disjoint(
    ops: &[SceneOp],
    root: ReplayFrame,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
    frames: &mut [ReplayFrame],
) -> Result<bool, ReplayError> {
    let mut first_index = 0;
    while let Some(bounds) =
        next_resolved_child_bounds(ops, &mut first_index, root, resolver, output_scale, frames)?
    {
        let mut other_index = first_index;
        while let Some(other) =
            next_resolved_child_bounds(ops, &mut other_index, root, resolver, output_scale, frames)?
        {
            if bounds.intersect(&other).is_some() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn next_resolved_child_bounds(
    ops: &[SceneOp],
    index: &mut usize,
    root: ReplayFrame,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
    frames: &mut [ReplayFrame],
) -> Result<Option<Rect>, ReplayError> {
    while *index < ops.len() {
        match &ops[*index] {
            SceneOp::GroupBegin {
                transform,
                projective,
                opacity,
                ..
            } => {
                let end = matching_group_end(ops, *index).ok_or(ReplayError::UnbalancedGroup)?;
                let child_root = composed_frame(
                    root,
                    transform.unwrap_or(Transform::IDENTITY),
                    projective.filter(|value| !value.is_identity()),
                    opacity.map_or(root.alpha, |value| mul_alpha(root.alpha, value)),
                );
                let bounds = resolved_visual_union(
                    &ops[*index + 1..end],
                    child_root,
                    frames,
                    resolver,
                    output_scale,
                )?;
                *index = end + 1;
                if bounds.w > Fixed::ZERO && bounds.h > Fixed::ZERO {
                    return Ok(Some(bounds));
                }
            }
            SceneOp::GroupEnd => {
                *index = ops.len();
                return Ok(None);
            }
            SceneOp::PushClip { .. } | SceneOp::PopClip => *index += 1,
            op => {
                *index += 1;
                let Some(mut bounds) =
                    resolved_leaf_bounds(op, root.transform, resolver, output_scale)?
                else {
                    continue;
                };
                if let Some(projective) = root.projective {
                    let quad = projective
                        .apply_rect(bounds)
                        .ok_or(ReplayError::Render(RenderError::InvalidGeometry))?;
                    bounds = Rect::bounding_quad(&quad);
                }
                return Ok(Some(bounds));
            }
        }
    }
    Ok(None)
}

fn composed_frame(
    parent: ReplayFrame,
    local_affine: Transform,
    local_projective: Option<Transform3D>,
    alpha: u8,
) -> ReplayFrame {
    let (transform, projective) = match (parent.projective, local_projective) {
        (None, None) => (parent.transform.compose(&local_affine), None),
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
                Transform3D::from_affine(parent.transform)
                    .compose(&local)
                    .compose(&Transform3D::from_affine(local_affine)),
            ),
        ),
    };
    ReplayFrame {
        transform,
        projective,
        alpha,
        has_clip: false,
        visual_bounds: None,
        blur_support: Fixed::ZERO,
    }
}

fn resolved_visual_union(
    ops: &[SceneOp],
    root: ReplayFrame,
    frames: &mut [ReplayFrame],
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
) -> Result<Rect, ReplayError> {
    let mut root = root;
    root.visual_bounds = None;
    root.blur_support = Fixed::ZERO;
    let mut stack = ReplayStack::new(frames, root)?;
    for op in ops {
        match op {
            SceneOp::GroupBegin {
                transform,
                projective,
                opacity,
                filter,
                ..
            } => {
                let parent = stack.top();
                let mut frame = composed_frame(
                    parent,
                    transform.unwrap_or(Transform::IDENTITY),
                    projective.filter(|value| !value.is_identity()),
                    opacity.map_or(parent.alpha, |value| mul_alpha(parent.alpha, value)),
                );
                frame.blur_support = match filter {
                    None => Fixed::ZERO,
                    Some(ResourceRef::Token(value)) => {
                        parse_blur_filter(value).ok_or(ReplayError::Render(
                            RenderError::Unsupported(RenderFeature::Blur),
                        ))? * Fixed::from_int(4)
                    }
                    Some(ResourceRef::Inline(_) | ResourceRef::Index(_)) => {
                        return Err(ReplayError::Render(RenderError::Unsupported(
                            RenderFeature::Blur,
                        )));
                    }
                };
                stack.push(frame)?;
            }
            SceneOp::GroupEnd => {
                let frame = stack.pop()?;
                if let Some(bounds) = frame.visual_bounds {
                    stack
                        .top_mut()
                        .include_visual_bounds(bounds.inflate(frame.blur_support));
                }
            }
            _ => {
                let frame = stack.top();
                let Some(mut leaf) =
                    resolved_leaf_bounds(op, frame.transform, resolver, output_scale)?
                else {
                    continue;
                };
                if let Some(projective) = frame.projective {
                    let quad = projective
                        .apply_rect(leaf)
                        .ok_or(ReplayError::Render(RenderError::InvalidGeometry))?;
                    leaf = Rect::bounding_quad(&quad);
                }
                stack.top_mut().include_visual_bounds(leaf);
            }
        }
    }
    if stack.len != 1 {
        return Err(ReplayError::UnbalancedGroup);
    }
    Ok(stack.top().visual_bounds.unwrap_or(Rect::ZERO))
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

fn scope_bounds(
    ops: &[SceneOp],
    root: ReplayFrame,
    clip: Rect,
    resolver: &dyn SceneResolver,
    output_scale: Fixed,
    frames: &mut [ReplayFrame],
) -> Result<Rect, ReplayError> {
    if output_scale <= Fixed::ZERO {
        return Err(ReplayError::Render(RenderError::InvalidGeometry));
    }
    let local = resolved_visual_union(ops, root, frames, resolver, output_scale)?;
    if local.w <= Fixed::ZERO || local.h <= Fixed::ZERO {
        return Ok(Rect::ZERO);
    }
    Ok(local
        .inflate(Fixed::ONE / output_scale)
        .intersect(&clip)
        .unwrap_or(Rect::ZERO))
}

struct ScopePreflight<'a> {
    clip: &'a Rect,
    resolver: &'a dyn SceneResolver,
    root: ReplayFrame,
    output_scale: Fixed,
    region: FallbackRegion,
    rgba_capacity: usize,
}

fn preflight_software_scope(
    ops: &[SceneOp],
    frames: &mut [ReplayFrame],
    scopes: &mut [ReplayScopePlan],
    preflight: ScopePreflight<'_>,
) -> Result<usize, ReplayError> {
    let mut pixel = [0u8; 4];
    let texture = Texture::new(&mut pixel, 1, 1, ColorFormat::RGBA8888);
    let mut software = crate::render::backends::sw::SwRenderer::new(texture);
    software.viewport = crate::types::Viewport::new(
        preflight.region.width(),
        preflight.region.height(),
        preflight.output_scale,
    );
    let origin_x = Fixed::from_int(preflight.region.x()) / preflight.output_scale;
    let origin_y = Fixed::from_int(preflight.region.y()) / preflight.output_scale;
    let mut local = crate::render::renderer::RegionRenderer::new(&mut software, origin_x, origin_y);
    let mut routes = [ReplayPlan::EMPTY; 0];
    let mut route_len = 0;
    let mut scope_len = 0;
    let mut nested_scope_required = 0;
    replay_scene_pass(
        ops,
        &mut local,
        preflight.clip,
        preflight.resolver,
        frames,
        preflight.root,
        &mut ReplayPassState {
            pass: ReplayPass::Preflight,
            probe_only: false,
            plans: &mut routes,
            plan_len: &mut route_len,
            ordinal: 0,
            plan_cursor: 0,
            scopes,
            scope_len: &mut scope_len,
            nested_scope_required: &mut nested_scope_required,
            scope_cursor: 0,
            rgba: None,
            rgba_capacity: preflight.rgba_capacity,
        },
    )?;
    let required = scope_len.checked_add(nested_scope_required).ok_or(
        ReplayError::InsufficientScopePlans {
            required: usize::MAX,
            available: scopes.len(),
        },
    )?;
    if required > scopes.len() {
        return Err(ReplayError::InsufficientScopePlans {
            required,
            available: scopes.len(),
        });
    }
    Ok(required)
}

fn probe_scope(
    ops: &[SceneOp],
    renderer: &dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    root: ReplayFrame,
    frames: &mut [ReplayFrame],
) -> Result<bool, ReplayError> {
    let mut probe = ScopeProbe {
        renderer,
        needs_scope: Cell::new(false),
        error: Cell::new(None),
    };
    let mut routes = [ReplayPlan::EMPTY; 0];
    let mut scopes = [ReplayScopePlan::EMPTY; 0];
    let mut route_len = 0;
    let mut scope_len = 0;
    let mut nested_scope_required = 0;
    replay_scene_pass(
        ops,
        &mut probe,
        clip,
        resolver,
        frames,
        root,
        &mut ReplayPassState {
            pass: ReplayPass::Preflight,
            probe_only: true,
            plans: &mut routes,
            plan_len: &mut route_len,
            ordinal: 0,
            plan_cursor: 0,
            scopes: &mut scopes,
            scope_len: &mut scope_len,
            nested_scope_required: &mut nested_scope_required,
            scope_cursor: 0,
            rgba: None,
            rgba_capacity: 0,
        },
    )?;
    if let Some(error) = probe.error.get() {
        return Err(ReplayError::Render(error));
    }
    Ok(probe.needs_scope.get())
}

struct ScopeDraw<'a> {
    renderer: &'a mut dyn Renderer,
    clip: &'a Rect,
    resolver: &'a dyn SceneResolver,
    root: ReplayFrame,
    frames: &'a mut [ReplayFrame],
    routes: &'a mut [ReplayPlan],
    scopes: &'a mut [ReplayScopePlan],
    rgba: &'a mut [u8],
}

fn draw_scope(
    ops: &[SceneOp],
    scope: ReplayScopePlan,
    draw: &mut ScopeDraw<'_>,
) -> Result<(), ReplayError> {
    if let ReplayScopeKind::Isolated {
        opacity,
        blur_alpha,
    } = scope.kind
    {
        return draw_isolated_scope(ops, scope, draw, opacity, blur_alpha);
    }
    let scoped_ops = &ops[scope.start_idx..=scope.end_idx];
    let mut nested_error = None;
    let result = draw.renderer.render_scope(scope.region, &mut |local| {
        let result = replay_scene_with_root(
            scoped_ops,
            local,
            draw.clip,
            draw.resolver,
            ReplayScratch::new(draw.frames, draw.routes, draw.scopes).with_rgba(draw.rgba),
            draw.root,
        );
        match result {
            Ok(()) => Ok(()),
            Err(ReplayError::Render(error)) => Err(error),
            Err(error) => {
                nested_error = Some(error);
                Err(RenderError::BackendFailure)
            }
        }
    });
    if let Some(error) = nested_error {
        return Err(error);
    }
    result.map(|_| ()).map_err(ReplayError::Render)
}

fn draw_isolated_scope(
    ops: &[SceneOp],
    scope: ReplayScopePlan,
    draw: &mut ScopeDraw<'_>,
    opacity: u8,
    blur_alpha: Option<Fixed>,
) -> Result<(), ReplayError> {
    let SceneOp::GroupBegin {
        transform,
        projective,
        clip: group_clip,
        ..
    } = &ops[scope.start_idx]
    else {
        return Err(ReplayError::UnbalancedGroup);
    };
    let required = scope.region.required_bytes();
    if required > draw.rgba.len() {
        return Err(ReplayError::Render(RenderError::InsufficientWorkspace {
            required_bytes: required,
            capacity_bytes: draw.rgba.len(),
        }));
    }
    let (target, nested_rgba) = draw.rgba.split_at_mut(required);
    target.fill(0);
    let scale = draw.renderer.output_scale();
    let local_w = scope.region.width();
    let local_h = scope.region.height();
    let origin_x = Fixed::from_int(scope.region.x()) / scale;
    let origin_y = Fixed::from_int(scope.region.y()) / scale;
    let group = composed_frame(
        draw.root,
        transform.unwrap_or(Transform::IDENTITY),
        projective.filter(|value| !value.is_identity()),
        255,
    );
    {
        let texture = Texture::new(target, local_w, local_h, ColorFormat::RGBA8888);
        let mut software =
            crate::render::backends::sw::SwRenderer::new(texture).with_alpha_mode(AlphaMode::Blend);
        software.viewport = crate::types::Viewport::new(local_w, local_h, scale);
        {
            let mut local =
                crate::render::renderer::RegionRenderer::new(&mut software, origin_x, origin_y);
            if let Some(ResourceRef::Inline(path)) = group_clip {
                let command = DrawCommand::PushClip {
                    path,
                    transform: group.transform,
                    fill_rule: crate::render::raster::FillRule::EvenOdd,
                };
                local
                    .submit(
                        &DrawRequest::new(&command, *draw.clip)
                            .with_projective(group.projective.unwrap_or(Transform3D::IDENTITY)),
                    )
                    .map_err(ReplayError::Render)?;
            } else if group_clip.is_some() {
                return Err(ReplayError::UnresolvedClip);
            }
            replay_scene_with_root(
                &ops[scope.start_idx + 1..scope.end_idx],
                &mut local,
                draw.clip,
                draw.resolver,
                ReplayScratch::new(draw.frames, draw.routes, draw.scopes).with_rgba(nested_rgba),
                group,
            )?;
            if group_clip.is_some() {
                local
                    .submit(
                        &DrawRequest::new(&DrawCommand::PopClip, *draw.clip)
                            .with_projective(group.projective.unwrap_or(Transform3D::IDENTITY)),
                    )
                    .map_err(ReplayError::Render)?;
            }
        }
        if let Some(alpha) = blur_alpha {
            software
                .blur_target_region(
                    alpha,
                    &Rect::new(
                        0,
                        0,
                        Fixed::from(local_w) / scale,
                        Fixed::from(local_h) / scale,
                    ),
                )
                .map_err(ReplayError::Render)?;
        }
    }

    let source = Texture::from_ref(target, local_w, local_h, ColorFormat::RGBA8888);
    draw.renderer
        .submit(&DrawRequest::new(
            &DrawCommand::Blit {
                pos: Point::new(origin_x, origin_y),
                size: Point::new(Fixed::from(local_w) / scale, Fixed::from(local_h) / scale),
                transform: Transform::IDENTITY,
                quad: None,
                texture: &source,
                opa: opacity,
                radius: Fixed::ZERO,
                composite: crate::render::command::CompositeMode::SourceOver,
            },
            *draw.clip,
        ))
        .map_err(ReplayError::Render)
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
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    replay_scene_with_scratch(
        ops,
        renderer,
        clip,
        resolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes),
    )
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
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    replay_scene_with_scratch(
        ops,
        renderer,
        clip,
        resolver,
        ReplayScratch::new(frames, &mut routes, &mut scopes),
    )
}

/// Replay with caller-owned group frames, exact-route slots, and fallback
/// scope plans. Exact routes beyond the supplied slots are recomputed during
/// drawing; fallback scopes require one retained plan each.
pub fn replay_scene_with_scratch(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    scratch: ReplayScratch<'_>,
) -> Result<(), ReplayError> {
    replay_scene_with_root(ops, renderer, clip, resolver, scratch, ReplayFrame::EMPTY)
}

fn replay_scene_with_root(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    scratch: ReplayScratch<'_>,
    root: ReplayFrame,
) -> Result<(), ReplayError> {
    let ReplayScratch {
        frames,
        routes,
        scopes,
        rgba,
    } = scratch;
    let mut plan_len = 0;
    let mut scope_len = 0;
    let mut nested_scope_required = 0;
    let rgba_capacity = rgba.len();
    replay_scene_pass(
        ops,
        renderer,
        clip,
        resolver,
        &mut *frames,
        root,
        &mut ReplayPassState {
            pass: ReplayPass::Preflight,
            probe_only: false,
            plans: &mut *routes,
            plan_len: &mut plan_len,
            ordinal: 0,
            plan_cursor: 0,
            scopes: &mut *scopes,
            scope_len: &mut scope_len,
            nested_scope_required: &mut nested_scope_required,
            scope_cursor: 0,
            rgba: None,
            rgba_capacity,
        },
    )?;
    let required_scopes = scope_len.checked_add(nested_scope_required).ok_or(
        ReplayError::InsufficientScopePlans {
            required: usize::MAX,
            available: scopes.len(),
        },
    )?;
    if required_scopes > scopes.len() {
        return Err(ReplayError::InsufficientScopePlans {
            required: required_scopes,
            available: scopes.len(),
        });
    }
    replay_scene_pass(
        ops,
        renderer,
        clip,
        resolver,
        &mut *frames,
        root,
        &mut ReplayPassState {
            pass: ReplayPass::Draw,
            probe_only: false,
            plans: &mut *routes,
            plan_len: &mut plan_len,
            ordinal: 0,
            plan_cursor: 0,
            scopes: &mut *scopes,
            scope_len: &mut scope_len,
            nested_scope_required: &mut nested_scope_required,
            scope_cursor: 0,
            rgba: Some(&mut *rgba),
            rgba_capacity,
        },
    )
}

fn replay_scene_pass(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    frames: &mut [ReplayFrame],
    root: ReplayFrame,
    state: &mut ReplayPassState<'_>,
) -> Result<(), ReplayError> {
    let mut stack = ReplayStack::new(frames, root)?;
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
        if state.pass == ReplayPass::Draw {
            if let Some(scope) = state.scope_at(i) {
                let route_len = *state.plan_len;
                let scope_len = *state.scope_len;
                let rgba = state.rgba.take().unwrap_or(&mut []);
                let result = draw_scope(
                    ops,
                    scope,
                    &mut ScopeDraw {
                        renderer,
                        clip,
                        resolver,
                        root: top,
                        frames: stack.workspace_from_top(),
                        routes: &mut state.plans[route_len..],
                        scopes: &mut state.scopes[scope_len..],
                        rgba: &mut *rgba,
                    },
                );
                state.rgba = Some(rgba);
                result?;
                i = scope.end_idx + 1;
                continue;
            }
        }
        match op {
            SceneOp::GroupBegin {
                transform,
                projective,
                opacity,
                clip: group_clip,
                mask,
                filter,
                disjoint_hint,
            } => {
                let local_affine = transform.unwrap_or(Transform::IDENTITY);
                let local_projective = projective.filter(|value| !value.is_identity());
                let next = composed_frame(top, local_affine, local_projective, top.alpha);
                let composed = next.transform;
                let composed_projective = next.projective;
                let group_opacity = opacity.unwrap_or(255);
                if group_opacity == 0 {
                    skip_depth = 1;
                    i += 1;
                    continue;
                }
                if mask.is_some() {
                    return Err(ReplayError::Render(RenderError::Unsupported(
                        RenderFeature::Mask,
                    )));
                }
                let blur_radius = match filter {
                    None => None,
                    Some(ResourceRef::Token(value)) => Some(parse_blur_filter(value).ok_or(
                        ReplayError::Render(RenderError::Unsupported(RenderFeature::Blur)),
                    )?),
                    Some(ResourceRef::Inline(_) | ResourceRef::Index(_)) => {
                        return Err(ReplayError::Render(RenderError::Unsupported(
                            RenderFeature::Blur,
                        )));
                    }
                };
                let needs_subtree =
                    blur_radius.is_some() || (group_opacity != 255 && !*disjoint_hint);
                let end_idx = if needs_subtree {
                    Some(matching_group_end(ops, i).ok_or(ReplayError::UnbalancedGroup)?)
                } else {
                    None
                };
                let opacity_isolation = if group_opacity != 255 && !*disjoint_hint {
                    let end_idx = end_idx.expect("opacity subtree was resolved");
                    !resolved_children_disjoint(
                        &ops[i + 1..end_idx],
                        next,
                        resolver,
                        renderer.output_scale(),
                        stack.workspace_from_top(),
                    )?
                } else {
                    false
                };
                if opacity_isolation || blur_radius.is_some() {
                    if state.probe_only {
                        let end_idx = end_idx.expect("isolated subtree was resolved");
                        i = end_idx + 1;
                        continue;
                    }
                    if state.pass != ReplayPass::Preflight {
                        return Err(ReplayError::GroupOpacityNeedsOffscreen);
                    }
                    let end_idx = end_idx.expect("isolated subtree was resolved");
                    let inner = &ops[i + 1..end_idx];
                    if group_clip.is_some() && !matches!(group_clip, Some(ResourceRef::Inline(_))) {
                        return Err(ReplayError::UnresolvedClip);
                    }
                    let available = state.rgba_capacity;
                    let scope_root = ReplayFrame { alpha: 255, ..next };
                    let mut bounds = scope_bounds(
                        inner,
                        scope_root,
                        *clip,
                        resolver,
                        renderer.output_scale(),
                        stack.workspace_from_top(),
                    )?;
                    if let Some(radius) = blur_radius {
                        bounds = bounds
                            .inflate(radius * Fixed::from_int(4))
                            .intersect(clip)
                            .unwrap_or(Rect::ZERO);
                    }
                    if bounds.w <= Fixed::ZERO || bounds.h <= Fixed::ZERO {
                        i = end_idx + 1;
                        continue;
                    }
                    if available == 0 {
                        return Err(if opacity_isolation && blur_radius.is_none() {
                            ReplayError::GroupOpacityNeedsOffscreen
                        } else {
                            ReplayError::Render(RenderError::MissingWorkspace)
                        });
                    }
                    let region = renderer.plan_scope(&bounds).map_err(ReplayError::Render)?;
                    if region.required_bytes() > available {
                        return Err(ReplayError::Render(RenderError::InsufficientWorkspace {
                            required_bytes: region.required_bytes(),
                            capacity_bytes: available,
                        }));
                    }
                    let nested_required = preflight_software_scope(
                        inner,
                        stack.workspace_from_top(),
                        &mut state.scopes[*state.scope_len..],
                        ScopePreflight {
                            clip,
                            resolver,
                            root: scope_root,
                            output_scale: renderer.output_scale(),
                            region,
                            rgba_capacity: available - region.required_bytes(),
                        },
                    )?;
                    state.require_nested_scopes(nested_required);
                    state.retain_scope(
                        i,
                        end_idx,
                        region,
                        ReplayScopeKind::Isolated {
                            opacity: mul_alpha(top.alpha, group_opacity),
                            blur_alpha: blur_radius.map(|radius| {
                                crate::render::backends::sw::blur::alpha_for_radius(
                                    radius * renderer.output_scale(),
                                )
                            }),
                        },
                    )?;
                    i = end_idx + 1;
                    continue;
                }
                let next_alpha = mul_alpha(top.alpha, group_opacity);
                let next = ReplayFrame {
                    alpha: next_alpha,
                    ..next
                };
                if state.pass == ReplayPass::Preflight
                    && !state.probe_only
                    && !stack.has_active_clip()
                {
                    if let Some(ResourceRef::Inline(path)) = group_clip {
                        let command = DrawCommand::PushClip {
                            path,
                            transform: composed,
                            fill_rule: crate::render::raster::FillRule::EvenOdd,
                        };
                        let projection = composed_projective.unwrap_or(Transform3D::IDENTITY);
                        let request = DrawRequest::new(&command, *clip).with_projective(projection);
                        let mut needs_scope = match renderer.route(&request) {
                            Ok(RenderRoute::Native) => false,
                            Ok(RenderRoute::ExactFallback(_))
                            | Err(RenderError::Unsupported(RenderFeature::PathClip))
                            | Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry)) => {
                                true
                            }
                            Err(error) => return Err(ReplayError::Render(error)),
                        };
                        let end_idx =
                            matching_group_end(ops, i).ok_or(ReplayError::UnbalancedGroup)?;
                        let scoped_ops = &ops[i..=end_idx];
                        let scope_root = ReplayFrame {
                            has_clip: true,
                            ..next
                        };
                        if !needs_scope {
                            needs_scope = probe_scope(
                                &ops[i + 1..end_idx],
                                renderer,
                                clip,
                                resolver,
                                scope_root,
                                stack.workspace_from_top(),
                            )?;
                        }
                        if needs_scope {
                            let bounds = scope_bounds(
                                &ops[i + 1..end_idx],
                                scope_root,
                                *clip,
                                resolver,
                                renderer.output_scale(),
                                stack.workspace_from_top(),
                            )?;
                            let region =
                                renderer.plan_scope(&bounds).map_err(ReplayError::Render)?;
                            let nested_required = preflight_software_scope(
                                scoped_ops,
                                stack.workspace_from_top(),
                                &mut state.scopes[*state.scope_len..],
                                ScopePreflight {
                                    clip,
                                    resolver,
                                    root: top,
                                    output_scale: renderer.output_scale(),
                                    region,
                                    rgba_capacity: state.rgba_capacity,
                                },
                            )?;
                            state.require_nested_scopes(nested_required);
                            state.retain_scope(i, end_idx, region, ReplayScopeKind::Fallback)?;
                            i = end_idx + 1;
                            continue;
                        }
                    }
                }
                let has_clip = if let Some(ResourceRef::Inline(path)) = group_clip {
                    draw_in_frame(
                        renderer,
                        &ReplayFrame {
                            alpha: next_alpha,
                            ..next
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
                stack.push(ReplayFrame { has_clip, ..next })?;
            }
            SceneOp::GroupEnd => {
                let frame = stack.pop()?;
                if frame.has_clip {
                    draw_in_frame(renderer, &frame, &DrawCommand::PopClip, clip, state)?;
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

    struct ScopeFallback<'a> {
        target: crate::render::backends::sw::SwRenderer<'a>,
        outer_submits: usize,
        scope_edits: usize,
        projective_clip_only: bool,
    }

    impl Renderer for ScopeFallback<'_> {
        fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            if matches!(request.command, DrawCommand::PushClip { .. })
                && (!self.projective_clip_only || !request.projective.is_identity())
            {
                return Err(RenderError::Unsupported(RenderFeature::PathClip));
            }
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.outer_submits += 1;
            Ok(())
        }

        fn flush(&mut self) {}

        fn output_scale(&self) -> Fixed {
            self.target.output_scale()
        }

        fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
            Renderer::plan_scope(&self.target, bounds)
        }

        fn modify_target_region(
            &mut self,
            src: &Rect,
            draw: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
        ) -> Result<bool, RenderError> {
            self.scope_edits += 1;
            Renderer::modify_target_region(&mut self.target, src, draw)
        }
    }

    fn clipped_projective_group() -> [SceneOp; 3] {
        let clip_path = crate::render::path::Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(10),
            Fixed::from_int(10),
        );
        [
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(Transform3D::translate(
                    Fixed::from_int(5),
                    Fixed::from_int(3),
                )),
                opacity: None,
                clip: Some(ResourceRef::Inline(clip_path)),
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::FillRect {
                area: Rect::new(0, 0, 20, 20),
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgb(210, 40, 30),
                radius: Fixed::ZERO,
                opa: 255,
            },
            SceneOp::GroupEnd,
        ]
    }

    fn clipped_affine_group() -> [SceneOp; 3] {
        let mut ops = clipped_projective_group();
        let SceneOp::GroupBegin {
            transform,
            projective,
            ..
        } = &mut ops[0]
        else {
            unreachable!()
        };
        *transform = Some(Transform::translate(Fixed::from_int(5), Fixed::from_int(3)));
        *projective = None;
        ops
    }

    fn nested_clip_group() -> [SceneOp; 5] {
        let outer_clip = crate::render::path::Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(15),
            Fixed::from_int(15),
        );
        let inner_clip = crate::render::path::Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(10),
            Fixed::from_int(10),
        );
        [
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: None,
                clip: Some(ResourceRef::Inline(outer_clip)),
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::GroupBegin {
                transform: None,
                projective: Some(Transform3D::translate(
                    Fixed::from_int(5),
                    Fixed::from_int(3),
                )),
                opacity: None,
                clip: Some(ResourceRef::Inline(inner_clip)),
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::FillRect {
                area: Rect::new(0, 0, 20, 20),
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgb(210, 40, 30),
                radius: Fixed::ZERO,
                opa: 255,
            },
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ]
    }

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
        let mut scopes = [ReplayScopePlan::EMPTY; 1];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 10, 10),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut plans, &mut scopes),
        )
        .unwrap();

        assert_eq!(renderer.route_calls.get(), 1);
        assert_eq!(renderer.plain_submits, 0);
        assert_eq!(renderer.routed_submits, 1);
        assert!(core::mem::size_of::<ReplayPlan>() <= 40);
    }

    #[test]
    fn projective_clip_group_uses_one_bounded_scope() {
        let ops = clipped_projective_group();
        let mut pixels = [0u8; 40 * 30 * 4];
        let target = Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888);
        let mut renderer = ScopeFallback {
            target: crate::render::backends::sw::SwRenderer::new(target),
            outer_submits: 0,
            scope_edits: 0,
            projective_clip_only: false,
        };

        replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

        assert_eq!(renderer.scope_edits, 1);
        assert_eq!(renderer.outer_submits, 0);
        drop(renderer);
        assert_eq!(
            &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
            &[210, 40, 30, 255]
        );
        assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
    }

    #[test]
    fn unsupported_affine_clip_group_uses_one_bounded_scope() {
        let ops = clipped_affine_group();
        let mut pixels = [0u8; 40 * 30 * 4];
        let target = Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888);
        let mut renderer = ScopeFallback {
            target: crate::render::backends::sw::SwRenderer::new(target),
            outer_submits: 0,
            scope_edits: 0,
            projective_clip_only: false,
        };

        replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

        assert_eq!(renderer.scope_edits, 1);
        assert_eq!(renderer.outer_submits, 0);
        drop(renderer);
        assert_eq!(
            &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
            &[210, 40, 30, 255]
        );
        assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
    }

    #[test]
    fn fallback_clip_preserves_nested_group_isolation() {
        let clip_path = crate::render::path::Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(10),
            Fixed::from_int(10),
        );
        let red_fill = |x, y| SceneOp::FillRect {
            area: Rect::new(x, y, 4, 4),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(255, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let ops = [
            SceneOp::GroupBegin {
                transform: Some(Transform::translate(Fixed::from_int(5), Fixed::from_int(3))),
                projective: None,
                opacity: None,
                clip: Some(ResourceRef::Inline(clip_path)),
                mask: None,
                filter: None,
                disjoint_hint: false,
            },
            group(Some(128), false),
            red_fill(0, 0),
            red_fill(2, 2),
            SceneOp::GroupEnd,
            SceneOp::GroupEnd,
        ];
        let mut pixels = [0u8; 40 * 30 * 4];
        let target = Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888);
        let mut renderer = ScopeFallback {
            target: crate::render::backends::sw::SwRenderer::new(target),
            outer_submits: 0,
            scope_edits: 0,
            projective_clip_only: false,
        };
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 40 * 30 * 4];

        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 40, 30),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();

        assert_eq!(renderer.scope_edits, 1);
        assert_eq!(renderer.outer_submits, 0);
        drop(renderer);
        let overlap = (6 * 40 + 8) * 4;
        assert_eq!(&pixels[overlap..overlap + 4], &[128, 0, 0, 255]);
    }

    #[test]
    fn native_outer_clip_absorbs_a_nested_fallback_scope() {
        let ops = nested_clip_group();
        let mut pixels = [0u8; 40 * 30 * 4];
        let target = Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888);
        let mut renderer = ScopeFallback {
            target: crate::render::backends::sw::SwRenderer::new(target),
            outer_submits: 0,
            scope_edits: 0,
            projective_clip_only: true,
        };

        replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

        assert_eq!(renderer.scope_edits, 1);
        assert_eq!(renderer.outer_submits, 0);
        drop(renderer);
        assert_eq!(
            &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
            &[210, 40, 30, 255]
        );
        assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
    }

    #[test]
    fn projective_clip_group_requires_scope_plan_before_drawing() {
        let ops = clipped_projective_group();
        let mut pixels = [0u8; 40 * 30 * 4];
        let target = Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888);
        let mut renderer = ScopeFallback {
            target: crate::render::backends::sw::SwRenderer::new(target),
            outer_submits: 0,
            scope_edits: 0,
            projective_clip_only: false,
        };
        let mut frames = [ReplayFrame::EMPTY; 2];
        let mut routes = [ReplayPlan::EMPTY; 1];
        let mut scopes = [ReplayScopePlan::EMPTY; 0];

        assert_eq!(
            replay_scene_with_scratch(
                &ops,
                &mut renderer,
                &Rect::new(0, 0, 40, 30),
                &NoResolver,
                ReplayScratch::new(&mut frames, &mut routes, &mut scopes),
            ),
            Err(ReplayError::InsufficientScopePlans {
                required: 1,
                available: 0,
            })
        );
        assert_eq!(renderer.scope_edits, 0);
        assert_eq!(renderer.outer_submits, 0);
    }

    fn nested_isolated_groups(depth: usize) -> Vec<SceneOp> {
        let mut ops = Vec::new();
        for _ in 0..depth {
            ops.push(SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: Some(128),
                clip: None,
                mask: None,
                filter: None,
                disjoint_hint: false,
            });
        }
        ops.push(fill(Transform::IDENTITY));
        ops.push(fill(Transform::IDENTITY));
        for level in (0..depth).rev() {
            ops.push(SceneOp::GroupEnd);
            if level != 0 {
                ops.push(fill(Transform::IDENTITY));
            }
        }
        ops
    }

    #[test]
    fn nested_isolation_uses_caller_scope_capacity_without_a_hidden_depth_limit() {
        let ops = nested_isolated_groups(10);
        let mut output = [0u8; 8 * 8 * 4];
        let mut rgba = [0u8; 8 * 8 * 4 * 10];
        let target = Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(target);
        let mut frames = [ReplayFrame::EMPTY; 12];
        let mut routes = [ReplayPlan::EMPTY; 0];
        let mut short_scopes = [ReplayScopePlan::EMPTY; 9];

        assert_eq!(
            replay_scene_with_scratch(
                &ops,
                &mut renderer,
                &Rect::new(0, 0, 8, 8),
                &NoResolver,
                ReplayScratch::new(&mut frames, &mut routes, &mut short_scopes)
                    .with_rgba(&mut rgba),
            ),
            Err(ReplayError::InsufficientScopePlans {
                required: 10,
                available: 9,
            })
        );
        assert!(output.iter().all(|byte| *byte == 0));

        let target = Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(target);
        let mut scopes = [ReplayScopePlan::EMPTY; 10];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 8, 8),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();
        assert!(output.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn scene_blur_uses_physical_radius_and_includes_edge_bleed() {
        struct ScopePlanCapture {
            bounds: Cell<Option<Rect>>,
            scale: Fixed,
        }

        impl Renderer for ScopePlanCapture {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn output_scale(&self) -> Fixed {
                self.scale
            }

            fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
                self.bounds.set(Some(*bounds));
                FallbackRegion::from_logical_bounds(
                    *bounds,
                    crate::types::Viewport::new(200, 200, self.scale),
                    Some(200 * 200 * 4),
                )
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
        let mut renderer = ScopePlanCapture {
            bounds: Cell::new(None),
            scale: Fixed::from_int(2),
        };
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 200 * 200 * 4];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 100, 100),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();
        let bounds = renderer.bounds.get().expect("isolated blur scope");
        assert_eq!(bounds.x, Fixed::ZERO);
        assert_eq!(bounds.y, Fixed::ZERO);
        assert!(bounds.w >= Fixed::from_int(16));
        assert!(bounds.h >= Fixed::from_int(16));
    }

    #[test]
    fn outer_isolation_bounds_include_nested_blur_support() {
        struct FirstScopeBounds {
            calls: Cell<usize>,
            first: Cell<Option<Rect>>,
        }

        impl Renderer for FirstScopeBounds {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
                if self.calls.get() == 0 {
                    self.first.set(Some(*bounds));
                }
                self.calls.set(self.calls.get() + 1);
                FallbackRegion::from_logical_bounds(
                    *bounds,
                    crate::types::Viewport::new(32, 32, Fixed::ONE),
                    Some(32 * 32 * 4),
                )
            }

            fn flush(&mut self) {}
        }

        let ops = [
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: Some(128),
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
                filter: Some(ResourceRef::Token("blur:2".into())),
                disjoint_hint: true,
            },
            fill(Transform::IDENTITY),
            SceneOp::GroupEnd,
            fill(Transform::IDENTITY),
            SceneOp::GroupEnd,
        ];
        let mut renderer = FirstScopeBounds {
            calls: Cell::new(0),
            first: Cell::new(None),
        };
        let mut frames = [ReplayFrame::EMPTY; 4];
        let mut routes = [ReplayPlan::EMPTY; 0];
        let mut scopes = [ReplayScopePlan::EMPTY; 2];
        let mut rgba = [0u8; 32 * 32 * 4 * 2];

        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 32, 32),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();

        let bounds = renderer.first.get().expect("outer isolation scope");
        assert!(bounds.w >= Fixed::from_int(12));
        assert!(bounds.h >= Fixed::from_int(12));
    }

    #[test]
    fn scene_blur_resolves_glyph_ink_instead_of_using_the_clip() {
        struct BlurCapture(Cell<Option<Rect>>);

        impl Renderer for BlurCapture {
            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
                self.0.set(Some(*bounds));
                FallbackRegion::from_logical_bounds(
                    *bounds,
                    crate::types::Viewport::new(200, 100, Fixed::ONE),
                    Some(200 * 100 * 4),
                )
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
        let mut renderer = BlurCapture(Cell::new(None));
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 200 * 100 * 4];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 200, 100),
            &BitmapResolver(Font::bitmap_8x8()),
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();

        let region = renderer.0.get().expect("blur scope");
        assert!(region.x > Fixed::ZERO);
        assert!(region.w < Fixed::from_int(100));
        assert!(region.x <= Fixed::from_int(30));
        assert!(region.y <= Fixed::from_int(20));
        assert!(region.x + region.w >= Fixed::from_int(30));
        assert!(region.y + region.h >= Fixed::from_int(20));
    }

    #[test]
    fn isolated_blur_does_not_filter_the_existing_target() {
        let mut red = opaque_fill_at(14, 14);
        if let SceneOp::FillRect { color, .. } = &mut red {
            *color = Color::rgb(255, 0, 0);
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
            red,
            SceneOp::GroupEnd,
        ];
        let mut output = [0u8; 32 * 32 * 4];
        for pixel in output.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 255, 255]);
        }
        let texture = Texture::new(&mut output, 32, 32, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 32 * 32 * 4];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 32, 32),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();

        assert_eq!(&output[..4], &[0, 0, 255, 255]);
        let edge = (15 * 32 + 12) * 4;
        assert!(output[edge] > 0);
        assert!(output[edge + 2] > 0);
    }

    #[test]
    fn group_overlap_uses_resolved_glyph_ink() {
        let ops = [glyph_at(10), glyph_at(12)];
        let mut frames = [ReplayFrame::EMPTY; 1];
        assert!(
            !resolved_children_disjoint(
                &ops,
                ReplayFrame::EMPTY,
                &BitmapResolver(Font::bitmap_8x8()),
                Fixed::ONE,
                &mut frames,
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
    fn group_opacity_overlap_flattens_once_with_borrowed_rgba() {
        let mut first = opaque_fill_at(0, 0);
        let mut second = opaque_fill_at(2, 2);
        for op in [&mut first, &mut second] {
            if let SceneOp::FillRect { color, .. } = op {
                *color = Color::rgb(255, 0, 0);
            }
        }
        let ops = vec![group(Some(128), false), first, second, SceneOp::GroupEnd];
        let mut output = [0u8; 8 * 8 * 4];
        let texture = Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 8 * 8 * 4];
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 8, 8),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        )
        .unwrap();

        let overlap = (3 * 8 + 3) * 4;
        assert_eq!(&output[overlap..overlap + 4], &[128, 0, 0, 255]);
    }

    #[test]
    fn nested_isolation_exhaustion_leaves_output_untouched() {
        let ops = vec![
            group(Some(128), false),
            group(Some(128), false),
            opaque_fill_at(0, 0),
            opaque_fill_at(2, 2),
            SceneOp::GroupEnd,
            opaque_fill_at(1, 1),
            SceneOp::GroupEnd,
        ];
        let mut output = [17u8; 8 * 8 * 4];
        let before = output;
        let texture = Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888);
        let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
        let mut frames = [ReplayFrame::EMPTY; 8];
        let mut routes = [ReplayPlan::EMPTY; 8];
        let mut scopes = [ReplayScopePlan::EMPTY; 8];
        let mut rgba = [0u8; 200];
        assert!(matches!(
            replay_scene_with_scratch(
                &ops,
                &mut renderer,
                &Rect::new(0, 0, 8, 8),
                &NoResolver,
                ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
            ),
            Err(ReplayError::Render(
                RenderError::InsufficientWorkspace { .. }
            ))
        ));
        assert_eq!(output, before);
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
