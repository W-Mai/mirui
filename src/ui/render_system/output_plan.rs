use super::*;
use crate::render::scratch::PlaneLayout;
use crate::ui::offscreen::BufferKey;
use crate::ui::outputs::graph::OutputGraph;
use crate::ui::outputs::relations::OutputRelations;
use crate::ui::outputs::workspace::PaintWorkspace;
use core::any::TypeId;

#[cfg(test)]
mod drop_effect_tests;
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

#[derive(Default)]
pub(crate) struct TexturePlan {
    graph: OutputGraph,
    captures: Vec<Capture>,
    batch: u64,
    root: Option<Entity>,
    cycle_reported: bool,
    pub(crate) workspace: PaintWorkspace,
}

impl TexturePlan {
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

    fn output_layout(&self, entity: Entity) -> Option<(PlaneLayout, Rect)> {
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
        Some((
            plane,
            Rect::new(0, 0, capture.key.layout_rect.w, capture.key.layout_rect.h),
        ))
    }

    fn measure_workspace(&mut self, world: &World, entities: &[Entity]) -> usize {
        let Some(registry) = world.resource::<ViewRegistry>() else {
            return 0;
        };
        let mut required = 0;
        for (position, &entity) in entities.iter().enumerate() {
            let owner = self
                .captures
                .iter()
                .rposition(|c| c.start <= position && position < c.end);
            if owner.is_some_and(|id| self.captures[id].state == CaptureState::Blocked) {
                continue;
            }
            for view in registry.paint_iter() {
                let Some(measure) = view.workspace() else {
                    continue;
                };
                if view
                    .component_filter()
                    .is_some_and(|ty| !world.has_type(entity, ty))
                {
                    continue;
                }
                let Some(layout) = measure(world, entity, &|source| self.output_layout(source))
                else {
                    continue;
                };
                let bytes = layout.required_bytes();
                required = required.max(bytes);
                if let Some(owner) = owner {
                    self.captures[owner].workspace_bytes =
                        self.captures[owner].workspace_bytes.max(bytes);
                }
            }
        }
        required
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
    renderer: &dyn Renderer,
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
            plan.root = None;
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
    if plan.batch == outputs.batch_id() && plan.root == entities.first().copied() {
        return Ok(Some(prepared));
    }
    plan.build(world, tree, entities);
    let workspace_bytes = plan.measure_workspace(world, entities);
    if workspace_bytes == 0 {
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
        let state = if missing {
            outputs.invalidate(capture.entity);
            CaptureState::Missing
        } else if outputs.texture_matches(capture.key) {
            CaptureState::Ready
        } else if outputs.reserve_texture(capture.key) {
            CaptureState::Pending
        } else {
            CaptureState::Missing
        };
        plan.captures[id].state = state;
    }
    if workspace_bytes != 0 {
        outputs.reserve_workspace(&mut plan.workspace, workspace_bytes);
    }
    for position in 0..plan.graph.order().len() {
        let id = plan.graph.order()[position];
        let missing = plan.captures[id].workspace_bytes > plan.workspace.capacity()
            || plan
                .graph
                .dependencies(id)
                .any(|dependency| plan.captures[dependency].state == CaptureState::Missing);
        if missing {
            outputs.invalidate(plan.captures[id].entity);
            plan.captures[id].state = CaptureState::Missing;
        }
    }
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
        let result = super::prepare_offscreen(
            node,
            world,
            entities,
            &mut index,
            renderer,
            &node.rect,
            capture.config,
            capture.entity,
            Some(plan),
        );
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
