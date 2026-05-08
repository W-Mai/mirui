#[cfg(test)]
mod tests {
    use mirui::backend::InputEvent;
    use mirui::ecs::World;
    use mirui::event::EventHandler;
    use mirui::event::dispatch::dispatch;
    use mirui::layout::*;
    use mirui::types::{Color, Dimension};
    use mirui::widget::builder::WidgetBuilder;

    use std::sync::{Arc, Mutex};

    #[test]
    fn hit_test_finds_child() {
        let mut world = World::new();
        let clicked = Arc::new(Mutex::new(Vec::new()));

        let clicked_clone = clicked.clone();
        let child = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(100),
                height: Dimension::px(100),
                ..Default::default()
            })
            .id();
        world.insert(
            child,
            EventHandler::new(move |entity, _event| {
                clicked_clone.lock().unwrap().push(entity);
            }),
        );

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(child)
            .id();

        // Click inside child (50, 50)
        dispatch(
            &world,
            root,
            &InputEvent::Release {
                x: 50.into(),
                y: 50.into(),
            },
            200,
            200,
        );
        assert_eq!(clicked.lock().unwrap().len(), 1);
        assert_eq!(clicked.lock().unwrap()[0], child);
    }

    #[test]
    fn hit_test_misses_outside() {
        let mut world = World::new();
        let clicked = Arc::new(Mutex::new(false));

        let clicked_clone = clicked.clone();
        let child = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();
        world.insert(
            child,
            EventHandler::new(move |_entity, _event| {
                *clicked_clone.lock().unwrap() = true;
            }),
        );

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(child)
            .id();

        // Click outside child (150, 150)
        dispatch(
            &world,
            root,
            &InputEvent::Release {
                x: 150.into(),
                y: 150.into(),
            },
            200,
            200,
        );
        assert!(!*clicked.lock().unwrap());
    }

    #[test]
    fn hit_test_deepest_wins() {
        let mut world = World::new();
        let hit_entity = Arc::new(Mutex::new(None));

        let hit_clone = hit_entity.clone();
        let inner = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(0, 255, 0))
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();
        world.insert(
            inner,
            EventHandler::new(move |entity, _| {
                *hit_clone.lock().unwrap() = Some(entity);
            }),
        );

        let outer = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(100),
                height: Dimension::px(100),
                ..Default::default()
            })
            .child(inner)
            .id();

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(200),
                height: Dimension::px(200),
                ..Default::default()
            })
            .child(outer)
            .id();

        // Click at (25, 25) — inside inner
        dispatch(
            &world,
            root,
            &InputEvent::Release {
                x: 25.into(),
                y: 25.into(),
            },
            200,
            200,
        );
        assert_eq!(*hit_entity.lock().unwrap(), Some(inner));
    }
}
