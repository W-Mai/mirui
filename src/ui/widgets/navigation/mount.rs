use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, WidgetConstruction};
use crate::input::event::runtime::ConstructionProvider;
use crate::types::{Dimension, Fixed};
use crate::ui::layout::{AlignItems, FlexDirection, LayoutStyle, Position};
use crate::ui::{Children, Hidden, Parent, Style};

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
    ));
    world
        .get_mut::<Children>(parent)
        .unwrap()
        .0
        .retain(|&child| child != page);
    world.insert(page, Parent(host));
    world.get_mut::<Children>(host).unwrap().0.push(page);
    Some(host)
}

pub(crate) fn reveal(world: &mut World, page: Entity) {
    world.remove::<Hidden>(page);
    world.invalidate(page);
}

pub(crate) fn discard(world: &mut World, parent: Entity, page: Entity) {
    if !attached(world, parent, page) {
        return;
    }
    world.insert(page, Hidden);
    crate::input::event::retire_subtree_interaction(world, page);
    // Cancellation callbacks may remove or reparent the page.
    if attached(world, parent, page) {
        crate::ui::despawn_subtree(world, page);
    }
}
