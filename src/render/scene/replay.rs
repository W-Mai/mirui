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
use core::task::{Context, Poll, Waker};

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

    pub(crate) const fn with_transform(transform: Transform) -> Self {
        Self {
            transform,
            ..Self::EMPTY
        }
    }

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Allocation-free counters collected during scene replay.
pub struct ReplayMetricsSnapshot {
    /// Commands accepted directly by the target renderer.
    pub native_commands: usize,
    /// Commands routed through exact software fallback.
    pub fallback_commands: usize,
    /// Commands reclassified because no retained route slot was available.
    pub reclassified_commands: usize,
    /// Path-clip scopes replayed through exact software fallback.
    pub fallback_scopes: usize,
    /// Opacity or blur scopes rendered into temporary RGBA storage.
    pub isolated_scopes: usize,
    /// Total fallback workspace requested during preflight.
    pub planned_fallback_bytes: usize,
    /// Maximum group nesting observed during replay.
    pub peak_frame_depth: usize,
    /// Maximum retained command-route slots used.
    pub peak_route_slots: usize,
    /// Maximum retained scope-plan slots used.
    pub peak_scope_slots: usize,
    /// Maximum concurrent RGBA workspace used.
    pub peak_rgba_bytes: usize,
}

#[derive(Default)]
/// Caller-owned scene replay metrics with no heap storage.
pub struct ReplayMetrics {
    snapshot: Cell<ReplayMetricsSnapshot>,
}

impl ReplayMetrics {
    /// Returns the current counters and high-water marks.
    pub fn snapshot(&self) -> ReplayMetricsSnapshot {
        self.snapshot.get()
    }

    /// Clears all counters and high-water marks.
    pub fn reset(&self) {
        self.snapshot.set(ReplayMetricsSnapshot::default());
    }

    fn update(&self, update: impl FnOnce(&mut ReplayMetricsSnapshot)) {
        let mut snapshot = self.snapshot.get();
        update(&mut snapshot);
        self.snapshot.set(snapshot);
    }
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
    metrics: Option<&'a ReplayMetrics>,
    rgba_in_use: usize,
    waker: &'a Waker,
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
            metrics: None,
            rgba_in_use: 0,
            waker: Waker::noop(),
        }
    }

    /// Supply fixed-capacity RGBA storage for isolated scene groups.
    pub fn with_rgba(mut self, rgba: &'a mut [u8]) -> Self {
        self.rgba = rgba;
        self
    }

    pub fn with_metrics(mut self, metrics: &'a ReplayMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    fn with_rgba_usage(mut self, rgba_in_use: usize) -> Self {
        self.rgba_in_use = rgba_in_use;
        self
    }

    pub(crate) fn with_waker(mut self, waker: &'a Waker) -> Self {
        self.waker = waker;
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
    type TargetState = ();

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

    fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        Err(RenderError::BackendFailure)
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }

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
    metrics: Option<&'a ReplayMetrics>,
    rgba_in_use: usize,
    waker: &'a Waker,
    clip_depth: usize,
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
            if !self.probe_only {
                if let Some(metrics) = self.metrics {
                    metrics.update(|snapshot| match route {
                        RenderRoute::Native => snapshot.native_commands += 1,
                        RenderRoute::ExactFallback(region) => {
                            snapshot.fallback_commands += 1;
                            snapshot.planned_fallback_bytes = snapshot
                                .planned_fallback_bytes
                                .saturating_add(region.required_bytes());
                        }
                    });
                }
            }
            if matches!(route, RenderRoute::ExactFallback(_)) && *self.plan_len < self.plans.len() {
                self.plans[*self.plan_len] = ReplayPlan { ordinal, route };
                *self.plan_len += 1;
                if let Some(metrics) = self.metrics {
                    metrics.update(|snapshot| {
                        snapshot.peak_route_slots = snapshot.peak_route_slots.max(*self.plan_len);
                    });
                }
            }
            return Ok(());
        }
        while self.plan_cursor < *self.plan_len && self.plans[self.plan_cursor].ordinal < ordinal {
            self.plan_cursor += 1;
        }
        let route = if self.plan_cursor < *self.plan_len
            && self.plans[self.plan_cursor].ordinal == ordinal
        {
            let route = self.plans[self.plan_cursor].route;
            self.plan_cursor += 1;
            route
        } else {
            if let Some(metrics) = self.metrics {
                metrics.update(|snapshot| snapshot.reclassified_commands += 1);
            }
            renderer.route(request)?
        };
        let mut cx = Context::from_waker(self.waker);
        let Poll::Ready(draw) = renderer.prepare(*request, route, &mut cx) else {
            return Err(RenderError::ResourcePending);
        };
        renderer.submit(&draw?)?;
        match request.command {
            DrawCommand::PushClip { .. } => self.clip_depth += 1,
            DrawCommand::PopClip => self.clip_depth = self.clip_depth.saturating_sub(1),
            _ => {}
        }
        Ok(())
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
        if let Some(metrics) = self.metrics {
            metrics.update(|snapshot| {
                match kind {
                    ReplayScopeKind::Fallback => snapshot.fallback_scopes += 1,
                    ReplayScopeKind::Isolated { .. } => snapshot.isolated_scopes += 1,
                }
                snapshot.planned_fallback_bytes = snapshot
                    .planned_fallback_bytes
                    .saturating_add(region.required_bytes());
                snapshot.peak_scope_slots = snapshot.peak_scope_slots.max(*self.scope_len);
            });
        }
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
    let texture =
        Texture::new(&mut pixel, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut software = crate::render::backends::sw::SwRenderer::new(texture);
    software.viewport = crate::types::Viewport::new(
        preflight.region.width(),
        preflight.region.height(),
        preflight.output_scale,
    );
    let mut local = crate::render::renderer::RegionRenderer::new(
        &mut software,
        (preflight.region.x(), preflight.region.y()),
    );
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
            metrics: None,
            rgba_in_use: 0,
            waker: Waker::noop(),
            clip_depth: 0,
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
            metrics: None,
            rgba_in_use: 0,
            waker: Waker::noop(),
            clip_depth: 0,
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
    metrics: Option<&'a ReplayMetrics>,
    rgba_in_use: usize,
    waker: &'a Waker,
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
        let scratch = ReplayScratch::new(draw.frames, draw.routes, draw.scopes)
            .with_rgba(draw.rgba)
            .with_rgba_usage(draw.rgba_in_use)
            .with_waker(draw.waker);
        let scratch = match draw.metrics {
            Some(metrics) => scratch.with_metrics(metrics),
            None => scratch,
        };
        let result = replay_scene_with_root(
            scoped_ops,
            local,
            draw.clip,
            draw.resolver,
            scratch,
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
    let rgba_in_use = draw.rgba_in_use.saturating_add(required);
    if let Some(metrics) = draw.metrics {
        metrics.update(|snapshot| {
            snapshot.peak_rgba_bytes = snapshot.peak_rgba_bytes.max(rgba_in_use);
        });
    }
    target.fill(0);
    let scale = draw.renderer.output_scale();
    let local_w = scope.region.width();
    let local_h = scope.region.height();
    let origin_x = Fixed::from(scope.region.x()) / scale;
    let origin_y = Fixed::from(scope.region.y()) / scale;
    let group = composed_frame(
        draw.root,
        transform.unwrap_or(Transform::IDENTITY),
        projective.filter(|value| !value.is_identity()),
        255,
    );
    {
        let texture = Texture::new(target, local_w, local_h, ColorFormat::RGBA8888)
            .map_err(ReplayError::Render)?;
        let mut software =
            crate::render::backends::sw::SwRenderer::new(texture).with_alpha_mode(AlphaMode::Blend);
        software.viewport = crate::types::Viewport::new(local_w, local_h, scale);
        {
            let mut local = crate::render::renderer::RegionRenderer::new(
                &mut software,
                (scope.region.x(), scope.region.y()),
            );
            if let Some(ResourceRef::Inline(path)) = group_clip {
                let command = DrawCommand::PushClip {
                    path,
                    transform: group.transform,
                    fill_rule: crate::render::raster::FillRule::EvenOdd,
                };
                local
                    .draw_request(
                        &DrawRequest::new(&command, *draw.clip)
                            .with_projective(group.projective.unwrap_or(Transform3D::IDENTITY)),
                    )
                    .map_err(ReplayError::Render)?;
            } else if group_clip.is_some() {
                return Err(ReplayError::UnresolvedClip);
            }
            let scratch = ReplayScratch::new(draw.frames, draw.routes, draw.scopes)
                .with_rgba(nested_rgba)
                .with_rgba_usage(rgba_in_use)
                .with_waker(draw.waker);
            let scratch = match draw.metrics {
                Some(metrics) => scratch.with_metrics(metrics),
                None => scratch,
            };
            replay_scene_with_root(
                &ops[scope.start_idx + 1..scope.end_idx],
                &mut local,
                draw.clip,
                draw.resolver,
                scratch,
                group,
            )?;
            if group_clip.is_some() {
                local
                    .draw_request(
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

    let source = Texture::from_ref(target, local_w, local_h, ColorFormat::RGBA8888)
        .map_err(ReplayError::Render)?;
    draw.renderer
        .draw_request(&DrawRequest::new(
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

pub(crate) fn replay_scene_with_root(
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
        metrics,
        rgba_in_use,
        waker,
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
            metrics,
            rgba_in_use,
            waker,
            clip_depth: 0,
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
            metrics,
            rgba_in_use,
            waker,
            clip_depth: 0,
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
    let result = replay_scene_pass_inner(ops, renderer, clip, resolver, frames, root, state);
    if result.is_err() && state.pass == ReplayPass::Draw {
        while state.clip_depth != 0 {
            state.clip_depth -= 1;
            if renderer
                .draw_request(&DrawRequest::new(&DrawCommand::PopClip, *clip))
                .is_err()
            {
                break;
            }
        }
    }
    result
}

fn replay_scene_pass_inner(
    ops: &[SceneOp],
    renderer: &mut dyn Renderer,
    clip: &Rect,
    resolver: &dyn SceneResolver,
    frames: &mut [ReplayFrame],
    root: ReplayFrame,
    state: &mut ReplayPassState<'_>,
) -> Result<(), ReplayError> {
    let mut stack = ReplayStack::new(frames, root)?;
    if state.pass == ReplayPass::Preflight
        && !state.probe_only
        && let Some(metrics) = state.metrics
    {
        metrics.update(|snapshot| snapshot.peak_frame_depth = snapshot.peak_frame_depth.max(1));
    }
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
                        metrics: state.metrics,
                        rgba_in_use: state.rgba_in_use,
                        waker: state.waker,
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
                if state.pass == ReplayPass::Preflight
                    && !state.probe_only
                    && let Some(metrics) = state.metrics
                {
                    metrics.update(|snapshot| {
                        snapshot.peak_frame_depth = snapshot.peak_frame_depth.max(stack.len);
                    });
                }
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
#[path = "../../../tests/unit/render/scene/replay.rs"]
mod tests;
