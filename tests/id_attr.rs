#[cfg(test)]
mod tests {
    use mirui::ui;

    #[test]
    fn id_attr_registers_in_idmap() {
        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                slot (id: "src")
            }
        });
        let world = &app.world;

        let found = world.find_by_id("src").expect("id 'src' must register");
        assert_ne!(found, root);
    }

    #[test]
    fn id_attr_attaches_named_id_marker() {
        use mirui::ui::NamedId;

        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                slot (id: "hero")
            }
        });
        let world = &app.world;

        let entity = world.find_by_id("hero").expect("registered");
        let marker = world.get::<NamedId>(entity).expect("NamedId attached");
        assert_eq!(marker.0, "hero");
    }

    #[test]
    fn id_lookup_supports_bare_ident() {
        struct LinkTo(mirui::ecs::Entity);

        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                container (direction: mirui::ui::layout::FlexDirection::Row) {
                    src (id: "anchor")
                    consumer () [
                        LinkTo(id(anchor)),
                    ]
                }
            }
        });
        let world = &app.world;

        let anchor = world.find_by_id("anchor").expect("anchor registered");
        let entities = world.query::<LinkTo>().collect();
        let consumer = entities.first().copied().expect("consumer with LinkTo");
        assert_eq!(world.get::<LinkTo>(consumer).unwrap().0, anchor);
    }

    #[test]
    fn id_lookup_in_enchant_resolves_to_entity() {
        struct LinkTo(mirui::ecs::Entity);

        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                container (direction: mirui::ui::layout::FlexDirection::Row) {
                    src (id: "anchor")
                    consumer () [
                        LinkTo(id("anchor")),
                    ]
                }
            }
        });
        let world = &app.world;

        let anchor = world.find_by_id("anchor").expect("anchor registered");

        let entities = world.query::<LinkTo>().collect();
        let consumer = entities
            .first()
            .copied()
            .expect("at least one consumer with LinkTo");
        let link = world.get::<LinkTo>(consumer).unwrap();
        assert_eq!(link.0, anchor);
    }

    #[derive(Default)]
    struct Attachments(Vec<mirui::ecs::Entity>);

    fn record_attachment(world: &mut mirui::ecs::World, entity: mirui::ecs::Entity) {
        use mirui::ui::{NamedId, Parent};
        assert!(world.has::<Parent>(entity));
        if let Some(name) = world.get::<NamedId>(entity) {
            assert_eq!(world.find_by_id(name.0), Some(entity));
        }
        world.resource_mut::<Attachments>().unwrap().0.push(entity);
    }

    #[mirui::view(component = mirui::ui::Widget, attach = record_attachment)]
    fn attachment_probe() {}

    #[mirui::component]
    struct Reference(mirui::ecs::Entity);

    #[mirui::compose]
    fn reference_panel(target: mirui::ecs::Entity) -> mirui::ecs::Entity {
        ui! {
            View () [
                Reference(target),
            ]
        }
    }

    ui!(compose LinkedCard {
        @@body {
            reference_panel(id("fallback"))
            View (id: "fallback")
        }
    });

    #[test]
    fn forward_compose_arguments_work_in_niche_defaults_and_fills() {
        for fill in [false, true] {
            let mut app = mirui::app::App::headless(64, 64);
            app.with_widget(ui!(compose LinkedCard));
            let root = app.spawn_root().id();
            if fill {
                app.compose(root, |cx| {
                    ui! {
                        LinkedCard () {
                            @body {
                                reference_panel (id("filled"))
                                View (id: "filled")
                            }
                        }
                    }
                });
            } else {
                app.compose(root, |cx| {
                    ui! {
                        LinkedCard ()
                    }
                });
            }
            let name = if fill { "filled" } else { "fallback" };
            let source = app.world.find_by_id(name).unwrap();
            let links = app.world.query::<Reference>();
            let (consumer, reference) = links.iter().next().unwrap();
            assert_eq!(links.iter().count(), 1);
            assert_eq!(reference.0, source);
            let parent = app.world.get::<mirui::ui::Parent>(source).unwrap().0;
            assert_eq!(
                app.world.get::<mirui::ui::Children>(parent).unwrap().0,
                [consumer, source]
            );
            if fill {
                assert!(app.world.find_by_id("fallback").is_none());
            }
        }
    }

    #[test]
    fn forward_identity_preserves_expression_child_and_attachment_order() {
        use mirui::ui::{Children, NamedId};
        struct LinkTo(mirui::ecs::Entity);

        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();
        app.add_resource(Attachments::default());
        app.with_widget(attachment_probe::view());
        let mut evaluated = Vec::new();
        let parent = app.compose(root, |cx| {
            ui! {
                container () {
                    consumer (
                        id: "consumer",
                        width: {
                            evaluated.push(1);
                            12
                        }
                    ) [
                        LinkTo(id(anchor)),
                    ]
                    src (
                        id: "anchor",
                        width: {
                            evaluated.push(2);
                            24
                        }
                    )
                }
            }
        });
        let world = &app.world;
        let consumer = world.find_by_id("consumer").unwrap();
        let anchor = world.find_by_id("anchor").unwrap();
        assert_ne!(consumer, anchor);
        assert_eq!(world.get::<LinkTo>(consumer).unwrap().0, anchor);
        assert_eq!(world.query::<NamedId>().iter().count(), 2);
        assert_eq!(world.get::<Children>(parent).unwrap().0, [consumer, anchor]);
        assert_eq!(evaluated, [1, 2]);
        assert_eq!(
            world.resource::<Attachments>().unwrap().0,
            [parent, consumer, anchor]
        );
    }

    #[test]
    fn forward_identity_is_reserved_inside_the_executed_branch() {
        struct LinkTo(mirui::ecs::Entity);
        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();
        let enabled = false;
        app.compose(root, |cx| {
            ui! {
                container () {
                    if enabled {
                        consumer () [
                            LinkTo(id("missing")),
                        ]
                        src (id: "missing")
                    } else {
                        consumer () {
                            nested () [
                                LinkTo(id("anchor")),
                            ]
                        }
                        src (id: "anchor")
                    }
                }
            }
        });
        let world = &app.world;
        assert!(world.find_by_id("missing").is_none());
        let anchor = world.find_by_id("anchor").unwrap();
        let target = world.query::<LinkTo>().iter().next().unwrap().1.0;
        assert_eq!(target, anchor);
    }

    #[test]
    #[should_panic(expected = "widget id `shared` is already bound to another entity")]
    fn forward_identity_does_not_reuse_an_existing_named_entity() {
        struct LinkTo;
        impl LinkTo {
            fn new(_: mirui::ecs::Entity) -> Self {
                Self
            }
        }
        let mut app = mirui::app::App::headless(64, 64);
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            ui! {
                existing (id: "shared")
            }
        });
        app.compose(root, |cx| {
            ui! {
                container () {
                    consumer () [
                        LinkTo::new(id("shared")),
                    ]
                    src (id: "shared")
                }
            }
        });
    }

    #[test]
    #[should_panic(expected = "widget id `shared` is already bound to another entity")]
    fn duplicate_id_attributes_are_rejected() {
        let mut app = mirui::app::App::headless(64, 64);

        let root = app.spawn_root().id();

        app.compose(root, |cx| {
            ui! {
                row (direction: mirui::ui::layout::FlexDirection::Row) {
                    first (id: "shared")
                    second (id: "shared")
                }
            }
        });
    }
}
