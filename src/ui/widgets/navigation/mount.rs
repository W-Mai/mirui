use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, WidgetConstruction};
use crate::input::event::focus::{FocusState, Focusable};
use crate::input::event::listeners::InputBlocked;
use crate::input::event::runtime::ConstructionProvider;
use crate::types::{Dimension, Fixed};
use crate::ui::layout::{AlignItems, FlexDirection, LayoutStyle, Position};
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::{Children, Hidden, Parent, Style};

struct PageViewport(Entity);
impl crate::ecs::Component for PageViewport {}

#[derive(Default)]
pub(super) struct PageOffset(pub Fixed);
impl crate::ecs::Component for PageOffset {}

#[derive(Default)]
struct PageFocus(Option<Entity>);
impl crate::ecs::Component for PageFocus {}

pub(crate) fn attached(world: &World, parent: Entity, page: Entity) -> bool {
    world.is_alive(page)
        && world.get::<Parent>(page).is_some_and(|owner| {
            world.get::<PageViewport>(parent).is_some_and(|viewport| {
                owner.0 == viewport.0
                    && world
                        .get::<Parent>(viewport.0)
                        .is_some_and(|p| p.0 == parent)
            })
        })
}

fn viewport(world: &mut World, parent: Entity) -> Entity {
    if let Some(viewport) = world.get::<PageViewport>(parent)
        && world.is_alive(viewport.0)
        && world
            .get::<Parent>(viewport.0)
            .is_some_and(|owner| owner.0 == parent)
    {
        return viewport.0;
    }
    let mut scope = crate::ui::UiScope::new(world, parent);
    let viewport = scope.spawn(Style {
        layout: LayoutStyle {
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            direction: FlexDirection::Column,
            align: AlignItems::Stretch,
            grow: Fixed::ONE,
            ..LayoutStyle::default()
        },
        clip_children: true,
        ..Style::default()
    });
    scope.on_event(
        viewport,
        crate::input::event::layout::SizeChanged::EVENT,
        resize::callback(),
    );
    world.insert(parent, PageViewport(viewport));
    viewport
}

#[crate::event]
fn resize(
    event: event!(crate::input::event::layout::SizeChanged),
    target: entity!(),
    children: query!(Children),
    pages: query!(PageOffset, mut WidgetTransform),
) {
    let Some(children) = children.get(target) else {
        return;
    };
    for &page in &children.0 {
        if let Some((offset, transform)) = pages.get(page) {
            transform.0 =
                crate::types::Transform::translate(offset.0 * event.new.width, Fixed::ZERO);
        }
    }
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
    let viewport = viewport(world, parent);
    let host = crate::ui::UiScope::new(world, viewport).spawn((
        page_style(),
        Hidden,
        PageFocus::default(),
        PageOffset::default(),
        WidgetTransform::default(),
    ));
    let mut gate = crate::core::reactive::retained::RetainedEffects::new(world, |_, _| true);
    gate.active = false;
    world.insert(host, gate);
    host
}

fn page_style() -> Style {
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
    }
}

pub(super) fn wrap_root(world: &mut World, parent: Entity) -> Entity {
    let existing = world
        .get::<PageViewport>(parent)
        .map(|viewport| viewport.0)
        .filter(|&viewport| {
            world.is_alive(viewport)
                && world
                    .get::<Parent>(viewport)
                    .is_some_and(|owner| owner.0 == parent)
        });
    let mut children = world
        .get_mut::<Children>(parent)
        .map(|children| core::mem::take(&mut children.0))
        .unwrap_or_default();
    if let Some(viewport) = existing {
        children.retain(|&child| child != viewport);
        world.get_mut::<Children>(parent).unwrap().0.push(viewport);
    }
    let host = host(world, parent);
    for &child in &children {
        world.insert(child, Parent(host));
    }
    world.get_mut::<Children>(host).unwrap().0 = children;
    host
}

pub(crate) fn reveal(world: &mut World, page: Entity) {
    world.remove::<InputBlocked>(page);
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
    suspend_input(world, page);
    if !world.is_alive(page) {
        return;
    }
    if let Some(gate) = world.get_mut::<crate::core::reactive::retained::RetainedEffects>(page) {
        gate.active = false;
    }
    world.insert(page, Hidden);
}

pub(super) fn suspend_input(world: &mut World, page: Entity) {
    if !world.is_alive(page) || world.has::<InputBlocked>(page) {
        return;
    }
    let focus = world
        .resource::<FocusState>()
        .and_then(|state| state.focused)
        .filter(|&target| crate::ui::branch::is_in_subtree(world, page, target));
    if let Some(saved) = world.get_mut::<PageFocus>(page) {
        saved.0 = focus;
    }
    world.insert(page, InputBlocked);
    crate::input::event::retire_subtree_interaction(world, page);
}

pub(super) fn present(world: &mut World, page: Entity) {
    suspend_input(world, page);
    if !world.is_alive(page) {
        return;
    }
    if let Some(gate) = world.get_mut::<crate::core::reactive::retained::RetainedEffects>(page) {
        gate.active = true;
    }
    world.remove::<Hidden>(page);
    crate::core::reactive::retained::wake(world, page);
    world.mark_subtree_dirty(page);
}

pub(super) fn offset(world: &mut World, page: Entity, fraction: Fixed) {
    if let Some(offset) = world.get_mut::<PageOffset>(page) {
        offset.0 = fraction;
    }
    let width = world
        .get::<Parent>(page)
        .and_then(|parent| world.get::<crate::ui::ComputedRect>(parent.0))
        .map_or(Fixed::ZERO, |rect| rect.0.w);
    if let Some(mut widget) = world.widget_mut(page) {
        widget.set::<crate::ui::property::prop::Transform>(crate::types::Transform::translate(
            width * fraction,
            Fixed::ZERO,
        ));
    }
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
