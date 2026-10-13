//! A page selected by application state, without a navigation history.

use core::any::TypeId;

use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, EventKey, WidgetConstruction, WidgetSource};
use crate::input::event::runtime::ConstructionProvider;
use crate::ui::widgets::navigation::mount;

/// Displays the page selected by `page`. Equal values retain the mounted subtree.
///
/// `None` selects no page. The DSL converts a route value into `Some(route)` through its ordinary field conversion. Install one `CreatePage` callback and register [`view`] for the route type.
#[crate::component]
pub struct PageView<R: Clone + PartialEq + 'static> {
    pub page: Option<R>,
}

impl<R: Clone + PartialEq + 'static> Default for PageView<R> {
    fn default() -> Self {
        Self { page: None }
    }
}

impl<R: Clone + PartialEq + 'static> PageView<R> {
    #[allow(non_upper_case_globals)]
    pub const CreatePage: EventKey<CreatePage<R>> = CreatePage::<R>::EVENT;

    pub fn new(page: R) -> Self {
        Self { page: Some(page) }
    }
}

/// Constructs one page root beneath the supplied composition scope.
pub struct CreatePage<R: Clone + PartialEq + 'static> {
    route: R,
}

impl<R: Clone + PartialEq + 'static> CreatePage<R> {
    pub fn route(&self) -> &R {
        &self.route
    }
}

impl<R: Clone + PartialEq + 'static> Event for CreatePage<R> {
    type Route = WidgetConstruction;
    type Source = WidgetSource<PageView<R>>;
    type Options = ();
}

struct Selection<R: Clone + PartialEq + 'static> {
    page: Option<R>,
    generation: u64,
    provider: Option<ConstructionProvider<CreatePage<R>>>,
}

impl<R: Clone + PartialEq + 'static> Selection<R> {
    fn read(world: &World, entity: Entity) -> Option<Self> {
        Some(Self {
            page: world.get::<PageView<R>>(entity)?.page.clone(),
            generation: world.component_generation(entity, TypeId::of::<PageView<R>>())?,
            provider: world
                .get::<ConstructionProvider<CreatePage<R>>>(entity)
                .cloned(),
        })
    }

    fn matches(&self, world: &World, entity: Entity) -> bool {
        if world.component_generation(entity, TypeId::of::<PageView<R>>()) != Some(self.generation)
            || !world
                .get::<PageView<R>>(entity)
                .is_some_and(|view| view.page == self.page)
        {
            return false;
        }
        match (
            &self.provider,
            world.get::<ConstructionProvider<CreatePage<R>>>(entity),
        ) {
            (Some(left), Some(right)) => left.same_instance(right),
            (None, None) => true,
            _ => false,
        }
    }
}

struct PageState<R: Clone + PartialEq + 'static> {
    attempted: Selection<R>,
    mounted: Option<Entity>,
}
impl<R: Clone + PartialEq + 'static> crate::ecs::Component for PageState<R> {}

fn reconcile<R: Clone + PartialEq + 'static>(world: &mut World, entity: Entity) -> bool {
    if crate::ui::branch::is_hidden_in_tree(world, entity)
        || !crate::core::reactive::retained::enabled(world, entity)
    {
        return false;
    }
    if world.get::<PageState<R>>(entity).is_some_and(|state| {
        state.attempted.matches(world, entity)
            && state
                .mounted
                .is_none_or(|page| mount::attached(world, entity, page))
    }) {
        return false;
    }
    let Some(selection) = Selection::<R>::read(world, entity) else {
        return false;
    };
    let request = selection.page.clone().map(|route| CreatePage { route });
    let provider = selection.provider.clone();
    let previous = world
        .get::<PageState<R>>(entity)
        .and_then(|state| state.mounted)
        .filter(|&page| mount::attached(world, entity, page));
    world.insert(
        entity,
        PageState {
            attempted: selection,
            mounted: previous,
        },
    );

    let next = if let Some(request) = request {
        let Some(provider) = provider else {
            crate::warn!("PageView has no CreatePage callback");
            return true;
        };
        let Some(page) = mount::create(world, entity, &provider, &request) else {
            crate::warn!("PageView could not construct its selected page");
            return true;
        };
        Some(page)
    } else {
        None
    };
    crate::core::reactive::flush_signal_dirty(world);
    if !world
        .get::<PageState<R>>(entity)
        .is_some_and(|state| state.attempted.matches(world, entity))
    {
        if let Some(page) = next {
            mount::discard(world, entity, page);
        }
        return true;
    }

    world.get_mut::<PageState<R>>(entity).unwrap().mounted = next;
    if let Some(page) = next {
        mount::reveal(world, page);
    }
    if let Some(page) = previous {
        mount::discard(world, entity, page);
    }
    if world.is_alive(entity) {
        world.invalidate(entity);
    }
    true
}

#[crate::view(component = PageView<R>, name = "PageView", invalidation = Layout)]
fn page_view<R: Clone + PartialEq + 'static>() {}

/// Registers page construction and retirement for a route type.
pub fn view<R: Clone + PartialEq + 'static>() -> crate::ui::view::View {
    page_view::view::<R>().with_reconcile(reconcile::<R>)
}

#[cfg(test)]
mod tests;
