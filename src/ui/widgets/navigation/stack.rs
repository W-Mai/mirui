use alloc::vec::Vec;
use core::any::TypeId;

use super::{NavigationPath, PageTransition, mount, transition};
use crate::core::model::{BindingOwner, ModelHandle, ModelInvalidation, ModelSubscription};
use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, EventKey, WidgetConstruction, WidgetSource};
use crate::input::event::runtime::ConstructionProvider;

/// Retains visited pages while their entries remain in the bound path.
///
/// Ordinary children form the root page. `CreatePage` constructs pushed destinations. One registered path can drive one live stack at a time.
#[crate::component]
pub struct NavigationStack<R: Clone + 'static> {
    pub path: model!(optional NavigationPath<R>),
    pub transition: PageTransition,
}

impl<R: Clone + 'static> Default for NavigationStack<R> {
    fn default() -> Self {
        Self {
            path: None,
            transition: PageTransition::None,
        }
    }
}

impl<R: Clone + 'static> NavigationStack<R> {
    #[allow(non_upper_case_globals)]
    pub const CreatePage: EventKey<CreatePage<R>> = CreatePage::<R>::EVENT;
}

/// Constructs the page for one distinct path entry.
pub struct CreatePage<R: Clone + 'static> {
    route: R,
}

impl<R: Clone + 'static> CreatePage<R> {
    pub fn route(&self) -> &R {
        &self.route
    }
}

impl<R: Clone + 'static> Event for CreatePage<R> {
    type Route = WidgetConstruction;
    type Source = WidgetSource<NavigationStack<R>>;
    type Options = ();
}

type Path<R> = <NavigationPath<R> as crate::core::model::Model>::Handle;

struct Binding<R: Clone + 'static> {
    path: Option<Path<R>>,
    provider: Option<ConstructionProvider<CreatePage<R>>>,
    generation: u64,
}

fn same_path<R: Clone + 'static>(a: Option<&Path<R>>, b: Option<&Path<R>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.access().same_identity(b.access()),
        (None, None) => true,
        _ => false,
    }
}

impl<R: Clone + 'static> Binding<R> {
    fn read(world: &World, entity: Entity) -> Option<Self> {
        Some(Self {
            path: world.get::<NavigationStack<R>>(entity)?.path.clone(),
            provider: world
                .get::<ConstructionProvider<CreatePage<R>>>(entity)
                .cloned(),
            generation: world.component_generation(entity, TypeId::of::<NavigationStack<R>>())?,
        })
    }
    fn matches(&self, world: &World, entity: Entity) -> bool {
        if world.component_generation(entity, TypeId::of::<NavigationStack<R>>())
            != Some(self.generation)
            || !world
                .get::<NavigationStack<R>>(entity)
                .is_some_and(|stack| same_path::<R>(self.path.as_ref(), stack.path.as_ref()))
        {
            return false;
        }
        match (
            &self.provider,
            world.get::<ConstructionProvider<CreatePage<R>>>(entity),
        ) {
            (Some(a), Some(b)) => a.same_instance(b),
            (None, None) => true,
            _ => false,
        }
    }
    fn valid(&self, world: &World) -> bool {
        self.path.as_ref().is_none_or(|path| {
            BindingOwner::registered(path).check(world.id()).is_ok()
                && ModelHandle::callback_ready(path)
        })
    }
}

struct StackState<R: Clone + 'static> {
    root: Entity,
    shown: Entity,
    pages: Vec<(u64, Entity)>,
    binding: Binding<R>,
    attempted: Option<u64>,
    claimed: bool,
    warned: bool,
    prepared: bool,
    _subscription: Option<ModelSubscription>,
}
impl<R: Clone + 'static> crate::ecs::Component for StackState<R> {}

fn claim<R: Clone + 'static>(world: &World, entity: Entity, binding: &Binding<R>) -> bool {
    binding.path.is_none()
        || !world.query::<StackState<R>>().iter().any(|(other, state)| {
            other != entity
                && state.claimed
                && state.binding.matches(world, other)
                && same_path::<R>(state.binding.path.as_ref(), binding.path.as_ref())
        })
}

fn reset<R: Clone + 'static>(world: &mut World, entity: Entity, binding: Binding<R>) -> bool {
    transition::cancel(world, entity);
    while let Some((_, page)) = world
        .get_mut::<StackState<R>>(entity)
        .and_then(|state| state.pages.pop())
    {
        mount::discard(world, entity, page);
        if !binding.matches(world, entity) {
            return true;
        }
    }
    let root = world
        .get::<StackState<R>>(entity)
        .map(|state| state.root)
        .filter(|&root| mount::attached(world, entity, root))
        .unwrap_or_else(|| mount::wrap_root(world, entity));
    let valid = binding.valid(world);
    let path = binding.path.as_ref().filter(|_| valid);
    let capacity = path.map_or(0, |path| path.capacity());
    let subscription =
        path.map(|path| path.__mirui_subscribe_revision(world, entity, ModelInvalidation::Layout));
    world.insert(
        entity,
        StackState {
            root,
            shown: root,
            pages: Vec::with_capacity(capacity.saturating_add(1)),
            binding,
            attempted: None,
            claimed: false,
            warned: false,
            prepared: valid,
            _subscription: subscription,
        },
    );
    mount::reveal(world, root);
    true
}

fn ready<R: Clone + 'static>(world: &mut World, entity: Entity) -> bool {
    let Some(state) = world.get::<StackState<R>>(entity) else {
        return false;
    };
    if !state.binding.matches(world, entity) {
        return false;
    }
    if !state.binding.valid(world) {
        if !state.warned {
            crate::warn!("navigation path registration is unavailable for this App");
            world.get_mut::<StackState<R>>(entity).unwrap().warned = true;
        }
        return false;
    }
    if !state.claimed && !claim(world, entity, &state.binding) {
        if !state.warned {
            crate::warn!("navigation path is already bound to another stack");
            world.get_mut::<StackState<R>>(entity).unwrap().warned = true;
        }
        return false;
    }
    if !state.prepared {
        let path = state.binding.path.clone();
        let capacity = path.as_ref().map_or(0, |path| path.capacity());
        let subscription = path
            .as_ref()
            .map(|path| path.__mirui_subscribe_revision(world, entity, ModelInvalidation::Layout));
        let state = world.get_mut::<StackState<R>>(entity).unwrap();
        state.pages = Vec::with_capacity(capacity.saturating_add(1));
        state._subscription = subscription;
        state.prepared = true;
    }
    true
}

fn reconcile<R: Clone + 'static>(world: &mut World, entity: Entity) -> bool {
    if crate::ui::branch::is_hidden_in_tree(world, entity)
        || !crate::core::reactive::retained::enabled(world, entity)
    {
        return false;
    }
    let mut changed = false;
    if !world.get::<StackState<R>>(entity).is_some_and(|state| {
        state.binding.matches(world, entity) && mount::attached(world, entity, state.root)
    }) {
        let Some(binding) = Binding::<R>::read(world, entity) else {
            return false;
        };
        changed = reset(world, entity, binding);
    }
    if !ready::<R>(world, entity) {
        return changed;
    }
    let state = world.get::<StackState<R>>(entity).unwrap();
    let path = state.binding.path.clone();
    let revision = path.as_ref().map_or(0, |path| path.revision());
    let provider = state.binding.provider.clone();
    let root = state.root;
    let id = path
        .as_ref()
        .and_then(|path| path.read(|path| path.entries.last().map(|entry| entry.0)));
    let cached = id
        .and_then(|id| {
            state
                .pages
                .iter()
                .find(|entry| entry.0 == id)
                .map(|entry| entry.1)
        })
        .filter(|&page| mount::attached(world, entity, page));
    if transition::running(world, entity).is_some() {
        let target = if id.is_none() { Some(root) } else { cached };
        changed |= transition::redirect(world, entity, target);
        let Some((winner, loser)) = transition::finish(world, entity) else {
            return changed;
        };
        let state = world.get_mut::<StackState<R>>(entity).unwrap();
        state.shown = winner;
        state.attempted = None;
        mount::conceal(world, loser);
        if current::<R>(world, entity, revision) && mount::attached(world, entity, winner) {
            mount::reveal(world, winner);
            prune(world, entity, path.as_ref(), revision);
        }
        return true;
    }
    if state.attempted == Some(revision) && mount::attached(world, entity, state.shown) {
        return changed;
    }
    let state = world.get_mut::<StackState<R>>(entity).unwrap();
    state.claimed = true;
    state.warned = false;
    state.attempted = Some(revision);
    if !mount::attached(
        world,
        entity,
        world.get::<StackState<R>>(entity).unwrap().shown,
    ) {
        world.get_mut::<StackState<R>>(entity).unwrap().shown = root;
        mount::reveal(world, root);
    }
    let next = match (id, cached) {
        (None, _) => root,
        (_, Some(page)) => page,
        (Some(id), None) => {
            let Some(provider) = provider else {
                crate::warn!("NavigationStack has no CreatePage callback");
                return true;
            };
            let route = path
                .as_ref()
                .unwrap()
                .read(|path| path.entries.last().unwrap().1.clone());
            let Some(page) = mount::create(world, entity, &provider, &CreatePage { route }) else {
                crate::warn!("NavigationStack could not construct its destination");
                return true;
            };
            crate::core::reactive::flush_signal_dirty(world);
            if !current::<R>(world, entity, revision) {
                mount::discard(world, entity, page);
                return true;
            }
            world
                .get_mut::<StackState<R>>(entity)
                .unwrap()
                .pages
                .push((id, page));
            page
        }
    };
    let previous = world.get::<StackState<R>>(entity).unwrap().shown;
    let previous_id = world
        .get::<StackState<R>>(entity)
        .unwrap()
        .pages
        .iter()
        .find(|entry| entry.1 == previous)
        .map_or(0, |entry| entry.0);
    world.get_mut::<StackState<R>>(entity).unwrap().shown = next;
    let config = world.get::<NavigationStack<R>>(entity).unwrap().transition;
    if transition::begin(
        world,
        entity,
        previous,
        next,
        config,
        id.unwrap_or(0) > previous_id,
    ) {
        return true;
    }
    if previous != next {
        mount::conceal(world, previous);
    }
    if !current::<R>(world, entity, revision) || !mount::attached(world, entity, next) {
        return true;
    }
    mount::reveal(world, next);
    prune(world, entity, path.as_ref(), revision);
    if world.is_alive(entity) {
        world.invalidate(entity);
    }
    true
}

fn prune<R: Clone + 'static>(
    world: &mut World,
    entity: Entity,
    path: Option<&Path<R>>,
    revision: u64,
) {
    loop {
        let retired = world.get::<StackState<R>>(entity).and_then(|state| {
            state.pages.iter().position(|&(id, page)| {
                !mount::attached(world, entity, page)
                    || (page != state.shown
                        && path.is_none_or(|path| {
                            path.read(|path| !path.entries.iter().any(|entry| entry.0 == id))
                        }))
            })
        });
        let Some(index) = retired else {
            break;
        };
        let (_, page) = world
            .get_mut::<StackState<R>>(entity)
            .unwrap()
            .pages
            .remove(index);
        mount::discard(world, entity, page);
        if !current::<R>(world, entity, revision) {
            return;
        }
    }
}

fn current<R: Clone + 'static>(world: &World, entity: Entity, revision: u64) -> bool {
    world.get::<StackState<R>>(entity).is_some_and(|state| {
        state.binding.matches(world, entity)
            && state.binding.valid(world)
            && state
                .binding
                .path
                .as_ref()
                .map_or(0, |path| path.revision())
                == revision
    })
}

#[crate::view(component = NavigationStack<R>, name = "NavigationStack", invalidation = Layout)]
fn navigation_stack<R: Clone + 'static>() {}

/// Registers retained page construction and path coordination for a route type.
pub fn view<R: Clone + 'static>() -> crate::ui::view::View {
    navigation_stack::view::<R>()
        .with_reconcile(reconcile::<R>)
        .with_systems(
            const {
                &[crate::ecs::System::new(
                    "navigation_stack_transition",
                    crate::ecs::SystemSlot::Animation.priority(),
                    transition::advance::<StackState<R>>,
                )]
            } as &'static [crate::ecs::System],
        )
}

#[cfg(test)]
mod tests;
