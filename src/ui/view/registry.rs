use alloc::{rc::Rc, vec::Vec};
use core::any::TypeId;

use super::{QueryScope, View, ViewInvalidation, ViewObservationBindings};
use crate::ecs::{Entity, World};
use crate::ui::{self, Style, outputs::relations::OutputRelations};

#[derive(Default)]
pub struct ViewRegistry {
    pub(super) views: Vec<View>,
    input_attach_plan: Option<Rc<Vec<crate::ui::lifecycle::InputAttach>>>,
    prepared_layout_plan: Option<Rc<Vec<ui::prepared_layout::RegisteredLayout>>>,
    paint_order: Vec<usize>,
    next_registration: u64,
    pub(super) bindings: Vec<ViewBinding>,
    typography: ui::style::TypographyDeclarations,
    owner: Option<crate::ecs::world::WorldId>,
    output_registration: Option<u64>,
}

pub(super) struct ViewBinding {
    pub(super) entity: Entity,
    pub(super) registration_id: u64,
    pub(super) active: bool,
    pub(super) subscriptions: ViewObservationBindings,
}

impl ViewRegistry {
    /// Rebind changed components before layout and paint inspect their state.
    pub(crate) fn reconcile_observations(world: &mut World) {
        ui::lifecycle::prepare(world);
        let registered = world.with_resource_box::<Self, _>(|registry, world| {
            let owner = world.id();
            match registry.owner {
                None => registry.owner = Some(owner),
                Some(registered) => {
                    assert_eq!(registered, owner, "ViewRegistry belongs to a different App")
                }
            }
            if world.resource::<OutputRelations>().is_none()
                && registry
                    .views
                    .iter()
                    .any(|view| !view.definition.output_dependencies.is_empty())
            {
                world.insert_resource(OutputRelations::default());
            }
            let tracking_changed = registry.prepare_tracking(world);
            if world.resource::<OutputRelations>().is_some() {
                world.with_resource_box::<OutputRelations, _>(|relations, world| {
                    let activated = relations.activate(world, &mut registry.output_registration);
                    if activated {
                        ui::render_system::output_plan::invalidate_measurements(world);
                        for view in &registry.views {
                            if !view.definition.output_dependencies.is_empty() {
                                world.watch_component_type(
                                    view.component_filter().expect("output component filter"),
                                );
                            }
                        }
                    }
                    if tracking_changed || activated {
                        for view in &registry.views {
                            for dependency in view.definition.output_dependencies {
                                relations.register(&dependency.descriptor);
                            }
                        }
                    }
                    world.drain_component_changes(|world, change| {
                        registry.apply_change(world, change, Some(relations));
                    });
                    registry.invalidate_pending_queries(world);
                    relations.reconcile(world);
                });
            } else {
                world.drain_component_changes(|world, change| {
                    registry.apply_change(world, change, None);
                });
                registry.invalidate_pending_queries(world);
            }
        });
        if registered.is_none() {
            ui::render_system::output_plan::invalidate_measurements(world);
            world.with_resource_box::<OutputRelations, _>(|relations, world| {
                relations.clear(world);
                relations.reconcile(world);
            });
        }
    }

    pub(crate) fn invalidate_queries(world: &mut World) {
        Self::reconcile_observations(world);
    }

    fn prepare_tracking(&mut self, world: &mut World) -> bool {
        // Views are appended and prepared in registration order; the ready prefix is contiguous.
        if self
            .views
            .last()
            .is_none_or(|view| view.definition.component_tracking_ready)
        {
            return false;
        }
        ui::render_system::output_plan::invalidate_measurements(world);
        if !self.views[0].definition.component_tracking_ready {
            world.watch_component_type(TypeId::of::<Style>());
            world.watch_component_type(TypeId::of::<ui::Parent>());
        }
        for view in self
            .views
            .iter_mut()
            .filter(|view| !view.definition.component_tracking_ready)
        {
            for dependency in view.definition.output_dependencies {
                (dependency.descriptor.install)(world);
            }
            if let Some(type_id) = view.component_filter() {
                world.watch_component_type(type_id);
            }
            for dependency in view.definition.query_dependencies {
                world.watch_query_dependency_type(dependency());
                if view.component_filter() != Some(dependency()) {
                    world.watch_component_type(dependency());
                }
            }
            if !view.definition.query_dependencies.is_empty() {
                Self::invalidate_instances(world, view);
            }
            view.definition.component_tracking_ready = true;
        }
        true
    }

    /// Invalidate readers of a published component without re-running attachment or output preparation.
    pub(crate) fn component_published<T: crate::ecs::Component>(world: &mut World, entity: Entity) {
        let changed = TypeId::of::<T>();
        world.with_resource_box::<Self, _>(|registry, world| {
            for view in &registry.views {
                let owns_component = view.component_filter() == Some(changed);
                if !owns_component
                    && !view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|kind| kind() == changed)
                {
                    continue;
                }
                if !owns_component && view.definition.query_scope.get() != QueryScope::Local {
                    Self::invalidate_instances(world, view);
                } else if owns_component
                    || view
                        .component_filter()
                        .is_some_and(|kind| world.has_type(entity, kind))
                {
                    match view.invalidation() {
                        ViewInvalidation::Visual => world.invalidate_visual(entity),
                        ViewInvalidation::Layout => world.invalidate(entity),
                    }
                }
            }
        });
    }

    /// Mark consumers of newly published results without invalidating prepared producers.
    pub(crate) fn propagate_output_publications(world: &mut World) {
        world.with_resource_box::<OutputRelations, _>(|relations, world| {
            relations.propagate_publications(world);
        });
    }

    fn invalidate_pending_queries(&self, world: &mut World) {
        world.drain_query_dependency_changes(|world, changed, cause| {
            for view in &self.views {
                if (cause == crate::ecs::world::QueryDependencyCause::Source
                    || view.definition.query_scope.get() != QueryScope::Local)
                    && view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|dependency| dependency() == changed)
                {
                    Self::invalidate_instances(world, view);
                }
            }
        });
    }

    fn invalidate_instances(world: &mut World, view: &View) {
        if let Some(type_id) = view.component_filter() {
            world.for_each_stable_type(type_id, |world, entity| match view.invalidation() {
                ViewInvalidation::Visual => world.invalidate_visual(entity),
                ViewInvalidation::Layout => world.invalidate(entity),
            });
        }
    }

    fn apply_change(
        &mut self,
        world: &mut World,
        change: crate::ecs::world::ComponentChange,
        mut relations: Option<&mut OutputRelations>,
    ) {
        if change.type_id == TypeId::of::<Style>() {
            use crate::ecs::world::ComponentChangeKind;
            match change.kind {
                ComponentChangeKind::Component | ComponentChangeKind::InheritedStyle(_) => {
                    self.typography.update(world, change.entity, false);
                }
                _ => {}
            }
        } else if change.type_id == TypeId::of::<ui::Parent>() {
            self.typography.update(world, change.entity, true);
        } else if change.type_id == TypeId::of::<ui::Children>() {
            self.typography.update(world, change.entity, false);
        }
        for view in &self.views {
            if view.component_filter() != Some(change.type_id) {
                if view.definition.query_scope.get() == QueryScope::Local
                    && view
                        .definition
                        .query_dependencies
                        .iter()
                        .any(|kind| kind() == change.type_id)
                    && world.is_alive(change.entity)
                    && view
                        .component_filter()
                        .is_some_and(|kind| world.has_type(change.entity, kind))
                {
                    match view.invalidation() {
                        ViewInvalidation::Visual => world.invalidate_visual(change.entity),
                        ViewInvalidation::Layout => world.invalidate(change.entity),
                    }
                }
                continue;
            }
            let invalidation = Self::invalidation_for_change(view, change);
            if let Some(relations) = relations.as_deref_mut() {
                if world.is_alive(change.entity) && world.has_type(change.entity, change.type_id) {
                    for (slot, dependency) in view.definition.output_dependencies.iter().enumerate()
                    {
                        relations.bind(
                            world,
                            change.entity,
                            view.registration_id(),
                            slot,
                            dependency,
                            invalidation,
                        );
                    }
                } else {
                    relations.remove(change.entity, view.registration_id());
                }
            }
            if let Some(observe) = view.observation() {
                let registration_id = view.registration_id();
                let slot = self.bindings.iter().position(|binding| {
                    binding.entity == change.entity && binding.registration_id == registration_id
                });
                let slot = slot.or_else(|| {
                    self.bindings.iter().position(|binding| {
                        !binding.active && binding.registration_id == registration_id
                    })
                });
                let slot =
                    slot.or_else(|| self.bindings.iter().position(|binding| !binding.active));
                let index = slot.unwrap_or_else(|| {
                    self.bindings.push(ViewBinding {
                        entity: change.entity,
                        registration_id,
                        active: false,
                        subscriptions: ViewObservationBindings::default(),
                    });
                    self.bindings.len() - 1
                });
                let binding = &mut self.bindings[index];
                binding
                    .subscriptions
                    .reset(invalidation, view.definition.observation_count);
                binding.entity = change.entity;
                binding.registration_id = registration_id;
                binding.active =
                    world.is_alive(change.entity) && world.has_type(change.entity, change.type_id);
                if binding.active {
                    observe(world, change.entity, &mut binding.subscriptions);
                    binding.subscriptions.assert_complete();
                }
            }
            if world.is_alive(change.entity) {
                match invalidation {
                    ViewInvalidation::Visual => world.invalidate_visual(change.entity),
                    ViewInvalidation::Layout => world.invalidate(change.entity),
                }
            }
        }
    }

    fn invalidation_for_change(
        view: &View,
        change: crate::ecs::world::ComponentChange,
    ) -> ViewInvalidation {
        if view.component_filter() != Some(TypeId::of::<Style>()) {
            return view.invalidation();
        }
        match change.kind {
            crate::ecs::world::ComponentChangeKind::Visual => ViewInvalidation::Visual,
            crate::ecs::world::ComponentChangeKind::Layout => ViewInvalidation::Layout,
            crate::ecs::world::ComponentChangeKind::Transform => ViewInvalidation::Visual,
            crate::ecs::world::ComponentChangeKind::Component => view.invalidation(),
            crate::ecs::world::ComponentChangeKind::InheritedStyle(fields) => {
                if ui::style::metrics_changed(fields)
                    || fields & crate::ecs::world::ComponentChangeKind::LOCAL_LAYOUT != 0
                {
                    ViewInvalidation::Layout
                } else {
                    ViewInvalidation::Visual
                }
            }
        }
    }

    /// Pre-populated registry containing every built-in widget kind.
    /// `App::with_default_widgets` iterates this to wire each widget's
    /// `View::systems` into the scheduler before storing the registry
    /// as a `World` resource.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        reg.insert(ui::children_view::view());
        reg.insert(ui::style_view::view());
        reg.insert(crate::ui::widgets::button::view());
        reg.insert(crate::ui::widgets::checkbox::view());
        reg.insert(crate::ui::widgets::progress_bar::view());
        reg.insert(crate::ui::widgets::tabbar::view());
        reg.insert(crate::ui::widgets::text_input::view());
        reg.insert(crate::ui::widgets::text_editing::view());
        reg.insert(crate::ui::widgets::image::view());
        reg.insert(crate::ui::widgets::text::view());
        reg.insert(crate::ui::widgets::text::static_glyph_run_view());
        reg.insert(crate::ui::widgets::slider::view());
        reg.insert(crate::ui::widgets::switch::view());
        reg.insert(crate::ui::widgets::tab_pages::view());
        reg.insert(crate::ui::widgets::lazy_list::view());
        reg.insert(crate::ui::widgets::follow_bounds::view());
        reg.insert(crate::input::event::scroll::view());
        reg.insert(crate::ui::widgets::mirror::view());
        reg.insert(crate::ui::widgets::temporal_mix::view());
        reg.insert(crate::ui::widgets::background_blur::view());
        reg.insert(crate::ui::widgets::drop_shadow::view());
        reg.insert(crate::ui::widgets::drop_shadow::glow_view());
        reg.insert(crate::ui::widgets::icon::view());
        reg
    }

    /// Add a View descriptor in registration order and index its optional
    /// paint hook independently by paint priority.
    pub fn insert(&mut self, mut view: View) {
        assert!(
            view.definition.query_dependencies.is_empty() || view.component_filter().is_some(),
            "queried View requires a component filter"
        );
        if view.observation().is_some() {
            assert!(
                view.component_filter().is_some(),
                "observed View requires a component filter"
            );
        }
        view.definition.registration_id = Some(self.next_registration);
        view.definition.component_tracking_ready = false;
        self.next_registration = self
            .next_registration
            .checked_add(1)
            .expect("View registration identity exhausted");
        let index = self.views.len();
        let paint_priority = view.priority();
        if let Some(hook) = crate::ui::lifecycle::InputAttach::for_view(&view) {
            let plan = self
                .input_attach_plan
                .get_or_insert_with(|| Rc::new(Vec::new()));
            Rc::make_mut(plan).push(hook);
        }
        if let Some(hook) = view.definition.prepared_layout {
            let plan = self
                .prepared_layout_plan
                .get_or_insert_with(|| Rc::new(Vec::new()));
            let plan = Rc::make_mut(plan);
            plan.reserve_exact(1);
            plan.push(ui::prepared_layout::RegisteredLayout {
                registration: view.registration_id(),
                hook,
            });
        }
        self.views.push(view);
        if let Some(priority) = paint_priority {
            let pos = self
                .paint_order
                .iter()
                .position(|&index| self.views[index].priority().is_some_and(|p| p > priority))
                .unwrap_or(self.paint_order.len());
            self.paint_order.insert(pos, index);
        }
    }

    /// Iterate all descriptors in registration order, including those without paint.
    pub fn iter(&self) -> impl Iterator<Item = &View> {
        self.views.iter()
    }

    pub(crate) fn input_attach_plan(&self) -> Option<Rc<Vec<crate::ui::lifecycle::InputAttach>>> {
        self.input_attach_plan.as_ref().map(Rc::clone)
    }

    pub(crate) fn prepared_layout_plan(
        &self,
    ) -> Option<Rc<Vec<ui::prepared_layout::RegisteredLayout>>> {
        self.prepared_layout_plan.as_ref().map(Rc::clone)
    }

    pub(crate) fn prepared_layout_count(world: &World) -> usize {
        world
            .resource::<Self>()
            .and_then(|registry| registry.prepared_layout_plan.as_deref())
            .map_or(0, |plan| {
                plan.iter()
                    .filter(|entry| (entry.hook.needed)(world))
                    .count()
            })
    }

    pub(crate) fn input_bindings(
        &self,
    ) -> impl Iterator<
        Item = (
            &Option<TypeId>,
            &crate::input::event::view_input::SharedInputs,
        ),
    > {
        self.input_attach_plan
            .as_deref()
            .map_or(&[][..], |plan| plan.as_slice())
            .iter()
            .filter_map(crate::ui::lifecycle::InputAttach::inputs)
    }

    pub(crate) fn paint_iter(&self) -> impl ExactSizeIterator<Item = &View> {
        self.paint_order.iter().map(|&index| &self.views[index])
    }

    pub(crate) fn paint_at(&self, position: usize) -> Option<&View> {
        self.paint_order
            .get(position)
            .map(|&index| &self.views[index])
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

impl IntoIterator for ViewRegistry {
    type Item = View;
    type IntoIter = alloc::vec::IntoIter<View>;

    fn into_iter(self) -> Self::IntoIter {
        self.views.into_iter()
    }
}
