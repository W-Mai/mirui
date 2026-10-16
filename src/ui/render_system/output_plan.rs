use super::*;
use crate::render::scratch::PlaneLayout;
use crate::types::Fixed64;
use crate::ui::offscreen::BufferKey;
use crate::ui::outputs::graph::OutputGraph;
use crate::ui::outputs::relations::OutputRelations;
use crate::ui::outputs::workspace::PaintWorkspace;
use crate::ui::view::SourceGeometry;
use core::any::TypeId;

#[cfg(test)]
mod drop_effect_tests;
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod raster_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod workspace_tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureState {
    Pending,
    Ready,
    Missing,
    Blocked,
}

struct Capture {
    entity: Entity,
    start: usize,
    end: usize,
    config: super::super::OffscreenRender,
    key: BufferKey,
    state: CaptureState,
    workspace_bytes: usize,
}

struct NodeGeometry {
    extra: Option<Rect>,
    capture: Option<usize>,
}

fn geometry_fixed(value: Fixed64) -> Result<Fixed, ()> {
    if value < Fixed64::from_fixed(Fixed::MIN) || value > Fixed64::from_fixed(Fixed::MAX) {
        return Err(());
    }
    Ok(value.to_fixed())
}

fn geometry_rect(x: Fixed64, y: Fixed64, w: Fixed64, h: Fixed64) -> Result<Rect, ()> {
    if w < Fixed64::ZERO || h < Fixed64::ZERO {
        return Err(());
    }
    geometry_fixed(x + w)?;
    geometry_fixed(y + h)?;
    Ok(Rect::new(
        geometry_fixed(x)?,
        geometry_fixed(y)?,
        geometry_fixed(w)?,
        geometry_fixed(h)?,
    ))
}

fn geometry_translate(rect: Rect, from: Point, to: Point) -> Result<Rect, ()> {
    geometry_rect(
        Fixed64::from_fixed(rect.x) - Fixed64::from_fixed(from.x) + Fixed64::from_fixed(to.x),
        Fixed64::from_fixed(rect.y) - Fixed64::from_fixed(from.y) + Fixed64::from_fixed(to.y),
        Fixed64::from_fixed(rect.w),
        Fixed64::from_fixed(rect.h),
    )
}

fn geometry_union(a: Rect, b: Rect) -> Result<Rect, ()> {
    let a = geometry_translate(a, Point::ZERO, Point::ZERO)?;
    let b = geometry_translate(b, Point::ZERO, Point::ZERO)?;
    let x = Fixed64::from_fixed(a.x.min(b.x));
    let y = Fixed64::from_fixed(a.y.min(b.y));
    let right = (Fixed64::from_fixed(a.x) + Fixed64::from_fixed(a.w))
        .max(Fixed64::from_fixed(b.x) + Fixed64::from_fixed(b.w));
    let bottom = (Fixed64::from_fixed(a.y) + Fixed64::from_fixed(a.h))
        .max(Fixed64::from_fixed(b.y) + Fixed64::from_fixed(b.h));
    geometry_rect(x, y, right - x, bottom - y)
}

fn geometry_clip(bounds: Rect, clip: Option<Rect>) -> Result<Option<Rect>, ()> {
    let bounds = geometry_translate(bounds, Point::ZERO, Point::ZERO)?;
    if bounds.w == Fixed::ZERO || bounds.h == Fixed::ZERO {
        return Ok(None);
    }
    match clip {
        Some(clip) => Ok(bounds.intersect(&geometry_translate(clip, Point::ZERO, Point::ZERO)?)),
        None => Ok(Some(bounds)),
    }
}

#[derive(Default)]
pub(crate) struct TexturePlan {
    graph: OutputGraph,
    captures: Vec<Capture>,
    nodes: Vec<NodeGeometry>,
    geometry_ready: bool,
    workspace_bytes: usize,
    batch: u64,
    root: Option<Entity>,
    cycle_reported: bool,
    raster: Option<crate::render::backends::sw::SwScratch>,
    pub(crate) workspace: PaintWorkspace,
    pub(crate) target_workspace: PaintWorkspace,
}

impl TexturePlan {
    pub(crate) fn trim_memory(&mut self) {
        self.raster = None;
        self.workspace.release();
        self.target_workspace.release();
    }

    pub(super) fn matches_root(&self, root: Entity) -> bool {
        self.root == Some(root)
    }

    fn reserve_workspaces(&mut self, outputs: &Outputs<WidgetTexture>) {
        if self.workspace_bytes != 0 {
            outputs.reserve_workspace(&mut self.workspace, self.workspace_bytes);
        }
        let target_bytes = self
            .captures
            .iter()
            .filter(|capture| matches!(capture.state, CaptureState::Pending | CaptureState::Ready))
            .map(|capture| outputs.target_workspace_bytes(capture.entity))
            .max()
            .unwrap_or(0);
        outputs.reserve_workspace(&mut self.target_workspace, target_bytes);
        for position in 0..self.graph.order().len() {
            let id = self.graph.order()[position];
            let capture = &self.captures[id];
            let missing = capture.workspace_bytes > self.workspace.capacity()
                || (capture.state == CaptureState::Pending
                    && outputs.target_workspace_bytes(capture.entity)
                        > self.target_workspace.capacity())
                || self
                    .graph
                    .dependencies(id)
                    .any(|dependency| self.captures[dependency].state == CaptureState::Missing);
            if missing {
                outputs.invalidate(capture.entity);
                self.captures[id].state = CaptureState::Missing;
            }
        }
    }

    fn collect(
        &mut self,
        node: &LayoutNode,
        world: &World,
        entities: &[Entity],
        index: &mut usize,
        parent: Option<usize>,
    ) {
        let start = *index;
        let Some(&entity) = entities.get(start) else {
            return;
        };
        let capture = super::super::offscreen::config(world, entity).map(|config| {
            let id = self.graph.add_node();
            debug_assert_eq!(id, self.captures.len());
            self.captures.push(Capture {
                entity,
                start,
                end: start + 1,
                config,
                key: BufferKey::for_widget(
                    entity,
                    node.rect,
                    Rect::new(0, 0, node.rect.w, node.rect.h),
                    config,
                    world
                        .get::<super::super::OffscreenGeneration>(entity)
                        .map_or(0, |g| g.0),
                ),
                state: CaptureState::Pending,
                workspace_bytes: 0,
            });
            if let Some(parent) = parent {
                self.graph.add_dependency(id, parent);
            }
            id
        });
        self.nodes.push(NodeGeometry {
            extra: None,
            capture,
        });
        *index += 1;
        for child in &node.children {
            self.collect(child, world, entities, index, capture.or(parent));
        }
        if let Some(id) = capture {
            self.captures[id].end = *index;
        }
    }

    fn build(&mut self, world: &World, node: &LayoutNode, entities: &[Entity]) {
        self.graph.clear();
        self.captures.clear();
        self.nodes.clear();
        self.collect(node, world, entities, &mut 0, None);
        if let Some(relations) = world.resource::<OutputRelations>() {
            relations.visit_current_dependencies(|consumer, source, descriptor| {
                if (descriptor.kind)() != TypeId::of::<WidgetTexture>() {
                    return;
                }
                let Some(position) = entities.iter().position(|e| *e == consumer) else {
                    return;
                };
                let owner = self
                    .captures
                    .iter()
                    .rposition(|c| c.start <= position && position < c.end);
                match self.captures.iter().position(|c| c.entity == source) {
                    Some(producer) => {
                        if let Some(owner) = owner {
                            self.graph.add_dependency(producer, owner);
                        }
                    }
                    None => {
                        (descriptor.reset)(world, source);
                        if let Some(owner) = owner {
                            self.captures[owner].state = CaptureState::Missing;
                        }
                    }
                }
            });
        }
        self.graph.resolve();
        for (index, capture) in self.captures.iter_mut().enumerate() {
            if self.graph.is_blocked(index) {
                capture.state = CaptureState::Blocked;
            }
        }
        let blocked = self
            .captures
            .iter()
            .any(|c| c.state == CaptureState::Blocked);
        if blocked && !self.cycle_reported {
            crate::warn!(target: "mirui::outputs", "cyclic output dependencies are unavailable");
        }
        self.cycle_reported = blocked;
    }

    pub(super) fn blocked(&self, entity: Entity) -> bool {
        self.captures
            .iter()
            .any(|capture| capture.entity == entity && capture.state == CaptureState::Blocked)
    }

    pub(super) fn ready(&self, entity: Entity) -> bool {
        self.captures
            .iter()
            .any(|capture| capture.entity == entity && capture.state == CaptureState::Ready)
    }

    pub(super) fn output_layout(&self, entity: Entity) -> Option<SourceGeometry> {
        let capture = self
            .captures
            .iter()
            .find(|capture| capture.entity == entity)?;
        if matches!(capture.state, CaptureState::Blocked | CaptureState::Missing)
            || capture.key.w == 0
            || capture.key.h == 0
        {
            return None;
        }
        let plane = PlaneLayout::packed(capture.key.w, capture.key.h, capture.key.format).ok()?;
        Some(SourceGeometry {
            plane,
            layout_rect: capture.key.layout_rect,
            local_bounds: capture.key.local_bounds,
        })
    }

    fn measure_node(
        &mut self,
        world: &World,
        entity: Entity,
        rect: Rect,
        position: usize,
        owner: Option<usize>,
    ) -> Result<(), ()> {
        let Some(registry) = world.resource::<ViewRegistry>() else {
            return Ok(());
        };
        for view in registry.paint_iter() {
            let Some(measure) = view.measurement() else {
                continue;
            };
            if view
                .component_filter()
                .is_some_and(|ty| !world.has_type(entity, ty))
            {
                continue;
            }
            let Some(measured) = measure(world, entity, rect, &|source| self.output_layout(source))
            else {
                continue;
            };
            if !measured.coverage.is_empty() {
                let bounds = measured.coverage.outer;
                let bounds = geometry_translate(bounds, Point::new(rect.x, rect.y), Point::ZERO)?;
                self.nodes[position].extra = Some(match self.nodes[position].extra {
                    Some(previous) => geometry_union(previous, bounds)?,
                    None => bounds,
                });
            }
            if let Some(layout) = measured.workspace {
                let bytes = layout.required_bytes();
                self.workspace_bytes = self.workspace_bytes.max(bytes);
                if let Some(owner) = owner {
                    self.captures[owner].workspace_bytes =
                        self.captures[owner].workspace_bytes.max(bytes);
                }
            }
        }
        Ok(())
    }

    pub(super) fn local_paint_bounds(&self, position: usize, rect: Rect) -> Option<Rect> {
        let node = self.nodes.get(position)?;
        let bounds = match node.capture {
            Some(capture) => self.captures[capture].key.local_bounds,
            None => node.extra?,
        };
        geometry_translate(bounds, Point::ZERO, Point::new(rect.x, rect.y)).ok()
    }

    pub(super) fn captured_paint_bounds(&self, position: usize, rect: Rect) -> Option<Rect> {
        self.nodes.get(position)?.capture?;
        self.local_paint_bounds(position, rect)
    }

    #[allow(clippy::too_many_arguments)]
    fn measure_tree(
        &mut self,
        world: &World,
        node: &LayoutNode,
        entities: &[Entity],
        index: &mut usize,
        owner: Option<usize>,
        parent: Transform,
        parent_3d: Transform3D,
        offset: Point,
        clip: Option<Rect>,
        scale: Fixed,
    ) -> Result<Option<Rect>, ()> {
        let position = *index;
        let result = (|| {
            let Some(&entity) = entities.get(position) else {
                return Ok(None);
            };
            let capture = self.nodes[position].capture;
            let rect = geometry_translate(node.rect, offset, Point::ZERO)?;
            let is_root = capture.is_some() && capture == owner;
            let (affine, projective) = if is_root {
                (parent, parent_3d)
            } else {
                render_transforms(parent, parent_3d, world, entity, rect)
            };
            if let Some(id) = capture.filter(|id| Some(*id) != owner) {
                *index += count_nodes(node);
                if self.captures[id].state == CaptureState::Blocked {
                    return Ok(None);
                }
                let local = self.local_paint_bounds(position, rect).ok_or(())?;
                let bounds = transformed_clip_bounds(local, affine, projective).ok_or(())?;
                return geometry_clip(bounds, clip);
            }
            self.measure_node(world, entity, node.rect, position, owner)?;
            let mut bounds = visual_bounds(world, entity, rect, affine, projective, scale);
            if let Some(extra) = self.nodes[position].extra {
                let shifted = geometry_translate(extra, Point::ZERO, Point::new(rect.x, rect.y))?;
                let extra = transformed_clip_bounds(shifted, affine, projective).ok_or(())?;
                bounds = geometry_union(bounds, extra)?;
            }
            let mut result = geometry_clip(bounds, clip)?;
            let scroll = world.get::<crate::input::event::scroll::ScrollOffset>(entity);
            let clips =
                scroll.is_some() || world.get::<Style>(entity).is_some_and(|s| s.clip_children);
            let child_clip = if clips {
                let area = transformed_clip_bounds(rect, affine, projective).unwrap_or(Rect::ZERO);
                Some(geometry_clip(area, clip)?.unwrap_or(Rect::ZERO))
            } else {
                clip
            };
            let child_offset = match scroll {
                Some(scroll) => Point::new(
                    geometry_fixed(Fixed64::from_fixed(offset.x) + Fixed64::from_fixed(scroll.x))?,
                    geometry_fixed(Fixed64::from_fixed(offset.y) + Fixed64::from_fixed(scroll.y))?,
                ),
                None => offset,
            };
            *index += 1;
            let mut invalid = false;
            for child in &node.children {
                match self.measure_tree(
                    world,
                    child,
                    entities,
                    index,
                    owner,
                    affine,
                    projective,
                    child_offset,
                    child_clip,
                    scale,
                ) {
                    Ok(Some(child_bounds)) => match result {
                        Some(bounds) => match geometry_union(bounds, child_bounds) {
                            Ok(bounds) => result = Some(bounds),
                            Err(()) => invalid = true,
                        },
                        None => result = Some(child_bounds),
                    },
                    Ok(None) => {}
                    Err(()) => invalid = true,
                }
            }
            if invalid { Err(()) } else { Ok(result) }
        })();
        if result.is_err() {
            *index = position + count_nodes(node);
        }
        result
    }

    fn measure(&mut self, world: &World, tree: &LayoutNode, entities: &[Entity]) {
        self.build(world, tree, entities);
        self.workspace_bytes = 0;
        for position in 0..self.graph.order().len() {
            let id = self.graph.order()[position];
            if self.captures[id].state == CaptureState::Blocked {
                continue;
            }
            let capture = &self.captures[id];
            let node =
                node_at(tree, capture.start).expect("capture belongs to the layout snapshot");
            let mut index = capture.start;
            let scale = capture.config.scale.max(Fixed::ONE / 8);
            let bounds = self
                .measure_tree(
                    world,
                    node,
                    entities,
                    &mut index,
                    Some(id),
                    Transform::IDENTITY,
                    Transform3D::IDENTITY,
                    Point::ZERO,
                    None,
                    scale,
                )
                .and_then(|bounds| match bounds {
                    Some(bounds) => geometry_translate(
                        bounds,
                        Point::new(node.rect.x, node.rect.y),
                        Point::ZERO,
                    ),
                    None => Ok(Rect::ZERO),
                })
                .unwrap_or(Rect::ZERO);
            let capture = &mut self.captures[id];
            capture.key = BufferKey::for_widget(
                capture.entity,
                node.rect,
                bounds,
                capture.config,
                capture.key.generation,
            );
            if geometry_translate(
                capture.key.local_bounds,
                Point::ZERO,
                Point::new(node.rect.x, node.rect.y),
            )
            .is_err()
            {
                capture.key = BufferKey::for_widget(
                    capture.entity,
                    node.rect,
                    Rect::ZERO,
                    capture.config,
                    capture.key.generation,
                );
            }
            if capture.key.w == 0 || capture.key.h == 0 {
                capture.state = CaptureState::Missing;
            }
        }
        let _ = self.measure_tree(
            world,
            tree,
            entities,
            &mut 0,
            None,
            Transform::IDENTITY,
            Transform3D::IDENTITY,
            Point::ZERO,
            None,
            Fixed::ONE,
        );
        self.root = entities.first().copied();
        self.geometry_ready = true;
    }
}

pub(super) fn measure_for_dirty(
    world: &World,
    tree: &LayoutNode,
    entities: &[Entity],
) -> Option<TexturePlan> {
    if !super::super::offscreen::has_texture_requests(world)
        && world
            .storage::<super::super::OffscreenRender>()
            .is_none_or(|storage| storage.is_empty())
    {
        return None;
    }
    let mut plan = world
        .resource::<Outputs<WidgetTexture>>()?
        .take_texture_plan()?;
    plan.measure(world, tree, entities);
    Some(plan)
}

pub(super) fn return_plan(world: &World, plan: Option<TexturePlan>) {
    if let Some(plan) = plan {
        world
            .resource::<Outputs<WidgetTexture>>()
            .expect("output storage remains installed")
            .put_texture_plan(plan);
    }
}

pub(super) fn invalidate_geometry(world: &World) {
    let Some(outputs) = world.resource::<Outputs<WidgetTexture>>() else {
        return;
    };
    if let Some(mut plan) = outputs.take_texture_plan() {
        plan.nodes.clear();
        plan.captures.clear();
        plan.root = None;
        plan.geometry_ready = false;
        outputs.put_texture_plan(plan);
    }
}

pub(super) struct PreparedTextures<'a> {
    outputs: &'a Outputs<WidgetTexture>,
    pub(super) plan: TexturePlan,
}

impl Drop for PreparedTextures<'_> {
    fn drop(&mut self) {
        self.outputs
            .put_texture_plan(core::mem::take(&mut self.plan));
    }
}

pub(super) fn prepare<'a>(
    world: &'a World,
    tree: &LayoutNode,
    entities: &[Entity],
    renderer: &mut dyn Renderer,
) -> Result<Option<PreparedTextures<'a>>, RenderError> {
    let Some(outputs) = world.resource::<Outputs<WidgetTexture>>() else {
        return Ok(None);
    };
    if !super::super::offscreen::has_texture_requests(world)
        && world
            .storage::<super::super::OffscreenRender>()
            .is_none_or(|storage| storage.is_empty())
    {
        if let Some(mut plan) = outputs.take_texture_plan() {
            outputs.reserve_workspace(&mut plan.workspace, 0);
            outputs.reserve_workspace(&mut plan.target_workspace, 0);
            plan.raster = None;
            plan.root = None;
            plan.nodes.clear();
            plan.captures.clear();
            plan.geometry_ready = false;
            outputs.put_texture_plan(plan);
        }
        return Ok(None);
    }
    if renderer.offscreen_format().is_none() {
        return Ok(None);
    }
    let plan = outputs
        .take_texture_plan()
        .ok_or(RenderError::BackendFailure)?;
    let mut prepared = PreparedTextures { outputs, plan };
    let plan = &mut prepared.plan;
    if !plan.geometry_ready
        && plan.batch == outputs.batch_id()
        && plan.root == entities.first().copied()
    {
        return Ok(Some(prepared));
    }
    if !plan.geometry_ready || plan.root != entities.first().copied() {
        plan.measure(world, tree, entities);
    }
    plan.geometry_ready = false;
    for capture in &mut plan.captures {
        capture.key.generation = world
            .get::<super::super::OffscreenGeneration>(capture.entity)
            .map_or(0, |g| g.0);
    }
    if plan.workspace_bytes == 0 {
        outputs.reserve_workspace(&mut plan.workspace, 0);
    }
    plan.batch = outputs.batch_id();
    plan.root = entities.first().copied();
    for capture in &plan.captures {
        if capture.state == CaptureState::Blocked {
            outputs.invalidate(capture.entity);
        }
    }
    // All output slots are admitted before a View can run. No result is evicted
    // to admit a later node in the same working set.
    for position in 0..plan.graph.order().len() {
        let id = plan.graph.order()[position];
        let capture = &plan.captures[id];
        let missing = capture.state == CaptureState::Missing
            || plan
                .graph
                .dependencies(id)
                .any(|dependency| plan.captures[dependency].state == CaptureState::Missing);
        let changes = plan
            .graph
            .dependencies(id)
            .any(|dependency| plan.captures[dependency].state == CaptureState::Pending);
        if changes {
            outputs.invalidate(capture.entity);
        }
        let state = if capture.key.w == 0 || capture.key.h == 0 {
            outputs.reserve_texture(capture.key, None)?;
            CaptureState::Missing
        } else if missing {
            outputs.invalidate(capture.entity);
            CaptureState::Missing
        } else if outputs.texture_matches(capture.key)? {
            CaptureState::Ready
        } else if outputs.reserve_texture(capture.key, None)? {
            CaptureState::Pending
        } else {
            CaptureState::Missing
        };
        plan.captures[id].state = state;
    }
    plan.reserve_workspaces(outputs);
    let mut failure = None;
    for position in 0..plan.graph.order().len() {
        let id = plan.graph.order()[position];
        let capture = &plan.captures[id];
        if capture.state != CaptureState::Pending {
            continue;
        }
        if plan
            .graph
            .dependencies(id)
            .any(|dependency| plan.captures[dependency].state != CaptureState::Ready)
        {
            plan.captures[id].state = CaptureState::Missing;
            continue;
        }
        let node = node_at(tree, capture.start).expect("capture belongs to the layout snapshot");
        let mut index = capture.start;
        let mut raster = plan
            .raster
            .take()
            .unwrap_or_else(crate::render::backends::sw::SwScratch::new);
        let result = super::prepare_offscreen(
            node,
            world,
            entities,
            &mut index,
            renderer,
            &node.rect,
            capture.key,
            capture.entity,
            Some(plan),
            &mut raster,
        );
        plan.raster = Some(raster);
        let published = match result {
            Ok(published) => published,
            Err(error) => {
                failure.get_or_insert(error);
                false
            }
        };
        plan.captures[id].state = if published {
            CaptureState::Ready
        } else {
            CaptureState::Missing
        };
        if published {
            acknowledge(
                world,
                &entities[plan.captures[id].start..plan.captures[id].end],
            );
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(Some(prepared)),
    }
}

fn node_at(root: &LayoutNode, mut index: usize) -> Option<&LayoutNode> {
    if index == 0 {
        return Some(root);
    }
    index -= 1;
    for child in &root.children {
        let count = super::count_nodes(child);
        if index < count {
            return node_at(child, index);
        }
        index -= count;
    }
    None
}

pub(super) fn acknowledge(world: &World, entities: &[Entity]) {
    if let Some(relations) = world.resource::<OutputRelations>() {
        for &entity in entities {
            relations.acknowledge(world, entity);
        }
    }
}

#[cfg(test)]
mod geometry_arithmetic_tests {
    use super::*;

    #[test]
    fn local_node_bounds_do_not_retain_or_double_add_layout_coordinates() {
        let rect = Rect::new(6_000_000, 6_000_000, 8, 8);
        let origin = Point::new(rect.x, rect.y);
        assert_eq!(geometry_translate(rect, origin, origin), Ok(rect));
        assert_eq!(
            geometry_translate(rect, origin, Point::ZERO),
            Ok(Rect::new(0, 0, 8, 8))
        );
        let mut plan = TexturePlan::default();
        plan.nodes.push(NodeGeometry {
            extra: Some(geometry_translate(rect, origin, Point::ZERO).unwrap()),
            capture: None,
        });
        assert_eq!(
            core::mem::size_of::<NodeGeometry>(),
            if core::mem::size_of::<usize>() == 8 {
                40
            } else {
                28
            }
        );
        assert_eq!(plan.local_paint_bounds(0, rect), Some(rect));
        assert_eq!(
            plan.local_paint_bounds(0, Rect::new(0, 0, 8, 8)),
            Some(Rect::new(0, 0, 8, 8))
        );
        assert_eq!(
            plan.local_paint_bounds(0, Rect::new(12, -4, 8, 8)),
            Some(Rect::new(12, -4, 8, 8))
        );
    }

    #[test]
    fn unrepresentable_footprints_are_rejected_before_rect_arithmetic() {
        assert!(
            geometry_translate(Rect::new(Fixed::MAX, 0, 1, 1), Point::ZERO, Point::ZERO).is_err()
        );
        assert!(
            geometry_translate(
                Rect::new(-6_000_000, 0, 1, 1),
                Point::new(6_000_000, 0),
                Point::ZERO,
            )
            .is_err()
        );
        assert!(
            geometry_union(
                Rect::new(-6_000_000, 0, 1, 1),
                Rect::new(6_000_000, 0, 1, 1),
            )
            .is_err()
        );
        assert_eq!(
            geometry_union(Rect::new(-4, -2, 8, 4), Rect::new(2, 1, 4, 3)),
            Ok(Rect::new(-4, -2, 10, 6))
        );
        let mut plan = TexturePlan::default();
        plan.nodes.push(NodeGeometry {
            extra: Some(Rect::new(0, 0, 1, 1)),
            capture: None,
        });
        assert!(
            plan.local_paint_bounds(0, Rect::new(Fixed::MAX, 0, 1, 1))
                .is_none()
        );
    }

    #[test]
    fn invalid_capture_geometry_is_missing_and_preserves_preorder_progress() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(root, crate::ui::OffscreenRender::new());
        let mut tree = LayoutNode::new(LayoutStyle::default());
        tree.rect = Rect::new(-6_000_000, 0, 1, 1);
        let mut child_node = LayoutNode::new(LayoutStyle::default());
        child_node.rect = Rect::new(6_000_000, 0, 1, 1);
        tree.children.push(child_node);
        let entities = [root, child];
        let mut plan = TexturePlan::default();
        plan.build(&world, &tree, &entities);
        let mut index = 0;
        assert!(
            plan.measure_tree(
                &world,
                &tree,
                &entities,
                &mut index,
                Some(0),
                Transform::IDENTITY,
                Transform3D::IDENTITY,
                Point::ZERO,
                None,
                Fixed::ONE,
            )
            .is_err()
        );
        assert_eq!(index, entities.len());
        plan.measure(&world, &tree, &entities);
        assert!(matches!(plan.captures[0].state, CaptureState::Missing));
        assert_eq!((plan.captures[0].key.w, plan.captures[0].key.h), (0, 0));

        let node_capacity = plan.nodes.capacity();
        tree.children.clear();
        tree.rect = Rect::new(Fixed::MAX - Fixed::ONE, 0, 1, 1);
        world.insert(root, crate::ui::OffscreenRender::with_scale(Fixed::ONE / 8));
        plan.measure(&world, &tree, &entities[..1]);
        assert!(matches!(plan.captures[0].state, CaptureState::Missing));
        assert_eq!((plan.captures[0].key.w, plan.captures[0].key.h), (0, 0));
        assert_eq!(plan.nodes.capacity(), node_capacity);
    }
}
