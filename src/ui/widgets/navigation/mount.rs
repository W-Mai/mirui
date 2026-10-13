use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, WidgetConstruction};
use crate::input::event::focus::{FocusState, Focusable};
use crate::input::event::runtime::ConstructionProvider;
use crate::types::{Dimension, Fixed};
use crate::ui::layout::{AlignItems, FlexDirection, LayoutStyle, Position};
use crate::ui::{Children, Hidden, Parent, Style};

#[derive(Default)]
struct PageFocus(Option<Entity>);
impl crate::ecs::Component for PageFocus {}

pub(crate) fn attached(world: &World, parent: Entity, page: Entity) -> bool {
    world.is_alive(page)
        && world
            .get::<Parent>(page)
            .is_some_and(|owner| owner.0 == parent)
}

pub(crate) fn create<E: Event<Route = WidgetConstruction>>(
    world: &mut World,
    parent: Entity,
    provider: &ConstructionProvider<E>,
    request: &E,
) -> Option<Entity> {
    let page = provider.invoke(world, request, parent)?;
    let host = host(world, parent);
    world
        .get_mut::<Children>(parent)
        .unwrap()
        .0
        .retain(|&child| child != page);
    world.insert(page, Parent(host));
    world.get_mut::<Children>(host).unwrap().0.push(page);
    Some(host)
}

fn host(world: &mut World, parent: Entity) -> Entity {
    let host = crate::ui::UiScope::new(world, parent).spawn((
        Style {
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::px(0),
                top: Dimension::px(0),
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                direction: FlexDirection::Column,
                align: AlignItems::Stretch,
                grow: Fixed::ONE,
                ..LayoutStyle::default()
            },
            clip_children: true,
            ..Style::default()
        },
        Hidden,
        PageFocus::default(),
    ));
    let mut gate = crate::core::reactive::retained::RetainedEffects::new(world, |_, _| true);
    gate.active = false;
    world.insert(host, gate);
    host
}

pub(super) fn wrap_root(world: &mut World, parent: Entity) -> Entity {
    let children = world
        .get_mut::<Children>(parent)
        .map(|children| core::mem::take(&mut children.0))
        .unwrap_or_default();
    let host = host(world, parent);
    for &child in &children {
        world.insert(child, Parent(host));
    }
    world.get_mut::<Children>(host).unwrap().0 = children;
    host
}

pub(crate) fn reveal(world: &mut World, page: Entity) {
    if let Some(gate) = world.get_mut::<crate::core::reactive::retained::RetainedEffects>(page) {
        gate.active = true;
    }
    world.remove::<Hidden>(page);
    crate::core::reactive::retained::wake(world, page);
    world.mark_subtree_dirty(page);
    let focus = world
        .get::<PageFocus>(page)
        .and_then(|focus| focus.0)
        .filter(|&target| {
            world.is_alive(target)
                && world.has::<Focusable>(target)
                && crate::ui::branch::is_in_subtree(world, page, target)
                && !crate::ui::branch::is_hidden_in_tree(world, target)
                && crate::core::reactive::retained::enabled(world, target)
        });
    if let Some(state) = world.resource_mut::<FocusState>()
        && state.focused.is_none()
    {
        state.focused = focus;
    }
}

pub(super) fn conceal(world: &mut World, page: Entity) {
    if !world.is_alive(page) {
        return;
    }
    let focus = world
        .resource::<FocusState>()
        .and_then(|state| state.focused)
        .filter(|&target| crate::ui::branch::is_in_subtree(world, page, target));
    if let Some(saved) = world.get_mut::<PageFocus>(page) {
        saved.0 = focus;
    }
    if let Some(gate) = world.get_mut::<crate::core::reactive::retained::RetainedEffects>(page) {
        gate.active = false;
    }
    world.insert(page, Hidden);
    crate::input::event::retire_subtree_interaction(world, page);
}

pub(crate) fn discard(world: &mut World, parent: Entity, page: Entity) {
    if !attached(world, parent, page) {
        return;
    }
    conceal(world, page);
    // Cancellation callbacks may remove or reparent the page.
    if attached(world, parent, page) {
        crate::ui::despawn_subtree(world, page);
    }
}
