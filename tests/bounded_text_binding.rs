use mirui::app::{App, TextContentFailure};
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::render::RenderError;
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui;
use mirui::ui::property::{PropertyChange, apply_to_world, prop};
use mirui::ui::widgets::text::TextContentError;
use mirui::ui::widgets::{Button, Text};
use mirui::{compose, model};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[model]
struct Reading {
    #[observe]
    value: u16,
}

#[model]
impl Reading {
    fn set(&mut self, value: u16) {
        self.value = value;
    }
}

#[compose(bind(reading))]
fn bounded_panel(reading: Reading) {
    ui! {
        Row {
            Text(text: ${ format_args!("{:03}", reading.value()) }, text_capacity: 4, id: "composed")
            Button(text: "SET", text_capacity: 3) on Tap { reading.set(12); }
        }
    };
}

#[test]
fn bounded_text_and_button_bindings_keep_storage_and_recover_after_overflow() {
    let mut app = App::headless(80, 40);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let reading = app.add_model(Reading { value: 7 });
    let bound = reading.share();
    let bound_button = reading.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column {
            Text(text: ${ format_args!("{:03}", bound.value()) }, text_capacity: 4, id: "reading")
            Button(text_capacity: 4, text: ${ (::core::format_args!("{:03}", bound_button.value())) }, id: "button")
            Text("ready", text_capacity: 5, id: "static")
            Text(text: (::core::format_args!("n={}", 7)), text_capacity: 3, id: "static_format")
        }
    };

    let reading_entity = app.world.find_by_id("reading").unwrap();
    let button_entity = app.world.find_by_id("button").unwrap();
    let static_entity = app.world.find_by_id("static").unwrap();
    let static_format_entity = app.world.find_by_id("static_format").unwrap();
    assert!(app.world.has::<Button>(button_entity));
    assert_eq!(
        app.world
            .get::<Text>(reading_entity)
            .unwrap()
            .resolve(&app.world),
        "007"
    );
    assert_eq!(
        app.world
            .get::<Text>(button_entity)
            .unwrap()
            .resolve(&app.world),
        "007"
    );
    assert_eq!(
        app.world
            .get::<Text>(static_entity)
            .unwrap()
            .resolve(&app.world),
        "ready"
    );
    assert_eq!(
        app.world
            .get::<Text>(static_format_entity)
            .unwrap()
            .resolve(&app.world),
        "n=7"
    );

    let content_pointer = match app.world.get::<Text>(reading_entity).unwrap().content() {
        mirui::ui::widgets::TextContent::Plain(std::borrow::Cow::Owned(value)) => value.as_ptr(),
        _ => panic!("bounded text must own its reserved content"),
    };

    reading.set(42);
    flush_signal_dirty(&mut app.world);
    let text = app.world.get::<Text>(reading_entity).unwrap();
    assert_eq!(text.resolve(&app.world), "042");
    assert!(text.has_valid_content());
    assert_eq!(text.last_content_error(), None);
    let pointer = match text.content() {
        mirui::ui::widgets::TextContent::Plain(std::borrow::Cow::Owned(value)) => value.as_ptr(),
        _ => unreachable!(),
    };
    assert_eq!(pointer, content_pointer);

    reading.set(10000);
    flush_signal_dirty(&mut app.world);
    for entity in [reading_entity, button_entity] {
        let text = app.world.get::<Text>(entity).unwrap();
        assert_eq!(text.resolve(&app.world), "042");
        assert_eq!(
            text.last_content_error(),
            Some(TextContentError::Capacity {
                required: 5,
                capacity: 4,
            })
        );
    }
    assert_eq!(
        tracked_allocations(|| {
            let failure = app.last_text_content_failure().unwrap();
            assert_eq!(
                failure.error,
                TextContentError::Capacity {
                    required: 5,
                    capacity: 4,
                }
            );
        }),
        0
    );

    reading.set(3);
    flush_signal_dirty(&mut app.world);
    for entity in [reading_entity, button_entity] {
        let text = app.world.get::<Text>(entity).unwrap();
        assert_eq!(text.resolve(&app.world), "003");
        assert_eq!(text.last_content_error(), None);
    }
}

#[test]
fn bounded_text_formats_locals_inside_static_and_reactive_blocks() {
    let mut app = App::headless(64, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let reading = app.add_model(Reading { value: 7 });
    let bound = reading.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column {
            Text(
                text: ${ let value = bound.value(); format_args!("{value:03}") },
                text_capacity: 3,
                id: "reactive_block"
            )
            Text(
                text: { let value = 12; format_args!("{value:03}") },
                text_capacity: 3,
                id: "static_block"
            )
        }
    };

    let reactive = app.world.find_by_id("reactive_block").unwrap();
    let static_text = app.world.find_by_id("static_block").unwrap();
    assert_eq!(
        app.world.get::<Text>(reactive).unwrap().resolve(&app.world),
        "007"
    );
    assert_eq!(
        app.world
            .get::<Text>(static_text)
            .unwrap()
            .resolve(&app.world),
        "012"
    );

    reading.set(42);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Text>(reactive).unwrap().resolve(&app.world),
        "042"
    );
    assert_eq!(
        app.world
            .get::<Text>(reactive)
            .unwrap()
            .last_content_error(),
        None
    );
}

#[derive(Default)]
struct CustomCard {
    text_capacity: usize,
}

#[test]
fn custom_component_keeps_its_own_text_capacity_field() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        CustomCard(text_capacity: 8usize, id: "custom")
    };

    let entity = app.world.find_by_id("custom").unwrap();
    assert_eq!(
        app.world.get::<CustomCard>(entity).unwrap().text_capacity,
        8
    );
}

#[test]
fn composed_bounded_binding_tracks_its_model() {
    let mut app = App::headless(64, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let reading = app.add_model(Reading { value: 5 });
    app.compose(root, |cx| bounded_panel(cx, reading.clone()));

    let entity = app.world.find_by_id("composed").unwrap();
    assert_eq!(
        app.world.get::<Text>(entity).unwrap().resolve(&app.world),
        "005"
    );
    assert_eq!(
        tracked_allocations(|| {
            reading.set(98);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    assert_eq!(
        app.world.get::<Text>(entity).unwrap().resolve(&app.world),
        "098"
    );
}

#[test]
fn first_bounded_value_overflow_has_no_valid_content_until_recovery() {
    let flushes = std::rc::Rc::new(std::cell::Cell::new(0));
    let flush_count = flushes.clone();
    let mut app = App::new(FramebufSurface::new(64, 24, move |_, _| {
        flush_count.set(flush_count.get() + 1);
    }));
    app.with_default_widgets();
    let root = app.spawn_root().id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Text("OVER", text_capacity: 3, id: "label")
    };

    let entity = app.world.find_by_id("label").unwrap();
    let text = app.world.get::<Text>(entity).unwrap();
    assert!(!text.has_valid_content());
    assert_eq!(
        text.last_content_error(),
        Some(TextContentError::Capacity {
            required: 4,
            capacity: 3,
        })
    );
    assert_eq!(
        app.last_text_content_failure(),
        Some(TextContentFailure {
            entity,
            error: TextContentError::Capacity {
                required: 4,
                capacity: 3,
            },
        })
    );
    assert_eq!(app.render(), Err(RenderError::TextContent));
    assert_eq!(app.render_dirty(), Err(RenderError::TextContent));
    assert_eq!(flushes.get(), 0);

    assert_eq!(
        apply_to_world::<prop::TextContent>(&mut app.world, entity, String::new()),
        PropertyChange::Layout
    );
    assert!(app.world.get::<Text>(entity).unwrap().has_valid_content());
    assert_eq!(
        app.world.get::<Text>(entity).unwrap().resolve(&app.world),
        ""
    );
    assert_eq!(app.last_text_content_failure(), None);
    assert_eq!(app.render_dirty(), Ok(()));
    assert_eq!(flushes.get(), 1);

    app.world
        .get_mut::<Text>(entity)
        .unwrap()
        .try_set_bounded::<3>(format_args!("OVER"))
        .unwrap_err();
    assert!(app.last_text_content_failure().is_some());
    assert_eq!(app.render_dirty(), Ok(()));
    let replacement = app.spawn_root().id();
    assert_eq!(app.last_text_content_failure(), None);
    app.set_root(root);
    assert_ne!(replacement, root);
}

#[test]
fn property_binding_preserves_bounded_content_after_overflow() {
    let mut app = App::headless(64, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Text("OK", text_capacity: 3, id: "bounded")
    };

    let entity = app.world.find_by_id("bounded").unwrap();
    assert_eq!(
        apply_to_world::<prop::TextContent>(&mut app.world, entity, "FOUR".into()),
        PropertyChange::Unchanged
    );
    let text = app.world.get::<Text>(entity).unwrap();
    assert_eq!(text.resolve(&app.world), "OK");
    assert_eq!(
        text.last_content_error(),
        Some(TextContentError::Capacity {
            required: 4,
            capacity: 3,
        })
    );
    assert_eq!(app.render_dirty(), Ok(()));
}

#[test]
fn reactive_initial_overflow_recovers_without_rebuilding_widgets() {
    let mut app = App::headless(64, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let reading = app.add_model(Reading { value: 10000 });
    let bound = reading.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Text(text: ${ format_args!("{:03}", bound.value()) }, text_capacity: 4, id: "reading")
    };

    let entity = app.world.find_by_id("reading").unwrap();
    assert_eq!(app.render_dirty(), Err(RenderError::TextContent));
    assert_eq!(app.last_text_content_failure().unwrap().entity, entity);

    reading.set(7);
    assert_eq!(app.render_dirty(), Ok(()));
    assert_eq!(app.world.find_by_id("reading"), Some(entity));
    assert_eq!(
        app.world.get::<Text>(entity).unwrap().resolve(&app.world),
        "007"
    );
    assert_eq!(app.last_text_content_failure(), None);
}
