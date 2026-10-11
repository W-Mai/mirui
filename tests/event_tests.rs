#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::input::event::gesture::{GestureEvents, GestureRecognizer};
    use mirui::input::event::hit_test::hit_test;
    use mirui::input::event::input::InputEvent;
    use mirui::types::{Color, Dimension, Fixed, Viewport};
    use mirui::ui::layout::*;
    use mirui::ui::{Children, HitTarget, Parent, Style, render_system};

    /// Per-entity tap counter so tests don't share static state.
    #[mirui::component]
    struct TapHits(u32);

    #[mirui::event]
    fn count_handler(node: entity!(), hits: query!(mut TapHits)) -> bool {
        if let Some(counter) = hits.get(node) {
            counter.0 += 1;
        }
        true
    }

    fn attach_counter(world: &mut World, entity: Entity) {
        world.insert(entity, TapHits(0));
        mirui::__macro_support::install_entity_event(
            world,
            entity,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            count_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );
        world.insert(entity, HitTarget);
    }

    fn hits(world: &World, entity: Entity) -> u32 {
        world.get::<TapHits>(entity).map(|c| c.0).unwrap_or(0)
    }

    /// Drive a tap through hit testing, recognition, and dispatch using the pointer release timestamp.
    fn tap_at(world: &mut World, root: Entity, x: i32, y: i32, screen: u16) {
        let xf = Fixed::from_int(x);
        let yf = Fixed::from_int(y);
        render_system::update_layout(world, root, &Viewport::new(screen, screen, Fixed::ONE));
        let hit = hit_test(world, root, xf, yf, screen, screen);

        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        rec.update(
            &InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: xf,
                y: yf,
            },
            0,
            hit,
            &mut events,
        );
        rec.update(
            &InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: xf,
                y: yf,
            },
            50,
            None,
            &mut events,
        );
        for ev in events.buffer.drain(..) {
            {
                mirui::__macro_support::dispatch_entity_gesture(world, &ev, 50).unwrap();
                mirui::__macro_support::flush_entity_events(world).unwrap();
            };
        }
    }

    #[test]
    fn hit_test_finds_child() {
        let mut world = World::new();
        let child = world.spawn((
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(100),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        attach_counter(&mut world, child);

        let root = world.spawn((
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Row,
                    width: Dimension::px(200),
                    height: Dimension::px(200),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        world.insert(child, Parent(root));
        world.get_mut::<Children>(root).unwrap().0.push(child);

        tap_at(&mut world, root, 50, 50, 200);
        assert_eq!(hits(&world, child), 1);
    }

    #[test]
    fn hit_test_misses_outside() {
        let mut world = World::new();
        let child = world.spawn((
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(50),
                    height: Dimension::px(50),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        attach_counter(&mut world, child);

        let root = world.spawn((
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Row,
                    width: Dimension::px(200),
                    height: Dimension::px(200),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        world.insert(child, Parent(root));
        world.get_mut::<Children>(root).unwrap().0.push(child);

        tap_at(&mut world, root, 150, 150, 200);
        assert_eq!(hits(&world, child), 0);
    }

    #[test]
    fn hit_test_deepest_wins() {
        let mut world = World::new();
        let inner = world.spawn((
            Style {
                bg_color: Some(Color::rgb(0, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(50),
                    height: Dimension::px(50),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        attach_counter(&mut world, inner);

        let outer = world.spawn((
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(100),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        world.insert(inner, Parent(outer));
        world.get_mut::<Children>(outer).unwrap().0.push(inner);
        attach_counter(&mut world, outer);

        let root = world.spawn((
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(200),
                    height: Dimension::px(200),
                    ..Default::default()
                },
                ..Default::default()
            },
            Children(Vec::new()),
        ));
        world.insert(outer, Parent(root));
        world.get_mut::<Children>(root).unwrap().0.push(outer);

        tap_at(&mut world, root, 25, 25, 200);
        // Deepest entity wins; inner's handler returns true so bubble stops there.
        assert_eq!(hits(&world, inner), 1);
        assert_eq!(hits(&world, outer), 0);
    }
}
