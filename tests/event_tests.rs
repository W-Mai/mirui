#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::event::gesture::{GestureEvent, GestureEvents, GestureRecognizer};
    use mirui::event::hit_test::hit_test;
    use mirui::event::input::InputEvent;
    use mirui::event::{GestureHandler, bubble_dispatch};
    use mirui::layout::*;
    use mirui::types::{Color, Dimension, Fixed};
    use mirui::widget::builder::WidgetBuilder;

    /// Per-entity tap counter so tests don't share static state.
    struct TapHits(u32);

    fn count_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
        if matches!(event, GestureEvent::Tap { .. }) {
            if let Some(c) = world.get_mut::<TapHits>(entity) {
                c.0 += 1;
            }
            true
        } else {
            false
        }
    }

    fn attach_counter(world: &mut World, entity: Entity) {
        world.insert(entity, TapHits(0));
        world.insert(entity, GestureHandler::from_fn(count_handler));
    }

    fn hits(world: &World, entity: Entity) -> u32 {
        world.get::<TapHits>(entity).map(|c| c.0).unwrap_or(0)
    }

    /// Drive a Tap gesture on (x, y) through hit_test + recognizer + bubble_dispatch.
    /// Mirrors what App::run does for one event pair.
    fn tap_at(world: &mut World, root: Entity, x: i32, y: i32, screen: u16) {
        let xf = Fixed::from_int(x);
        let yf = Fixed::from_int(y);
        let hit = hit_test(world, root, xf, yf, screen, screen);

        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        rec.update(
            &InputEvent::PointerDown {
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
                id: 0,
                x: xf,
                y: yf,
            },
            50,
            None,
            &mut events,
        );
        for ev in events.buffer.drain(..) {
            bubble_dispatch(world, &ev);
        }
    }

    #[test]
    fn hit_test_finds_child() {
        let mut world = World::new();
        let child = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(100),
                height: Dimension::px(100),
                ..Default::default()
            })
            .id();
        attach_counter(&mut world, child);

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(child)
            .id();

        tap_at(&mut world, root, 50, 50, 200);
        assert_eq!(hits(&world, child), 1);
    }

    #[test]
    fn hit_test_misses_outside() {
        let mut world = World::new();
        let child = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();
        attach_counter(&mut world, child);

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(child)
            .id();

        tap_at(&mut world, root, 150, 150, 200);
        assert_eq!(hits(&world, child), 0);
    }

    #[test]
    fn hit_test_deepest_wins() {
        let mut world = World::new();
        let inner = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(0, 255, 0))
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();
        attach_counter(&mut world, inner);

        let outer = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(100),
                height: Dimension::px(100),
                ..Default::default()
            })
            .child(inner)
            .id();
        attach_counter(&mut world, outer);

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(outer)
            .id();

        tap_at(&mut world, root, 25, 25, 200);
        // Deepest entity wins; inner's handler returns true so bubble stops there.
        assert_eq!(hits(&world, inner), 1);
        assert_eq!(hits(&world, outer), 0);
    }
}
