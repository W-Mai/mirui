use mirui::core::reactive::flush_signal_dirty;
use mirui::prelude::*;
use mirui::ui::widgets::Text;
use std::{cell::Cell, rc::Rc};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[test]
fn static_inputs_add_no_framework_allocations() {
    let allocations = tracked_allocations(|| {
        let value = Prop::from(42_u32);
        let forwarded = value.clone();
        assert_eq!(forwarded.get(), 42);
        assert!(!Prop::<bool>::default().get());
        assert_eq!(Prop::from(ColorToken::Primary).get(), ColorToken::Primary);
    });
    assert_eq!(allocations, 0);
    assert_eq!(Prop::<String>::from("Nearby").get(), "Nearby");
}

#[test]
fn getters_are_lazy_and_forwarding_shares_one_allocation() {
    let calls = Rc::new(Cell::new(0));
    let input_calls = calls.clone();
    let mut input = None;
    let allocations = tracked_allocations(|| {
        input = Some(Prop::new(move || {
            input_calls.set(input_calls.get() + 1);
            input_calls.get()
        }));
    });
    assert_eq!(allocations, 1);
    assert_eq!(calls.get(), 0);
    let input = input.unwrap();
    let allocations = tracked_allocations(|| {
        let first = input.clone();
        let second = first.clone();
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
    });
    assert_eq!(allocations, 0);
    assert_eq!(calls.get(), 2);
}

#[test]
fn the_last_forwarded_input_releases_its_captures() {
    let captured = Rc::new(Cell::new(7));
    let retired = Rc::downgrade(&captured);
    let input = Prop::new(move || captured.get());
    let forwarded = input.clone();
    drop(input);
    assert!(retired.upgrade().is_some());
    assert_eq!(forwarded.get(), 7);
    drop(forwarded);
    assert!(retired.upgrade().is_none());
}

#[model]
struct Status {
    #[observe]
    number: u32,
}

#[model]
impl Status {
    fn set_number(&mut self, number: u32) {
        self.number = number;
    }
}

#[compose]
fn caption(text: prop!(String)) -> Entity {
    ui! {
        Text (text: ${ text })
    }
}

#[compose]
fn header(text: prop!(String)) -> Entity {
    ui! {
        Column () {
            caption (text)
            caption (text)
        }
    }
}

#[compose]
fn screen(status: model!(Status)) -> Entity {
    ui! {
        Column () {
            header ("STATIC")
            header (${ status.number().to_string() })
            header (${ status.number().to_string() })
        }
    }
}

fn text_values<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> Vec<String> {
    app.world
        .query::<Text>()
        .collect()
        .into_iter()
        .map(|entity| {
            app.world
                .get::<Text>(entity)
                .unwrap()
                .resolve(app.world.resource())
                .to_string()
        })
        .collect()
}

#[test]
fn shared_inputs_cross_multiple_layers_without_rebuilding_children() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let status = app.add_model(Status { number: 7 }).finish();
    app.compose(root, screen(status.clone()));
    let entities = app.world.query::<Text>().collect();
    assert_eq!(text_values(&app), ["STATIC", "STATIC", "7", "7", "7", "7"]);
    status.set_number(19);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        text_values(&app),
        ["STATIC", "STATIC", "19", "19", "19", "19"]
    );
    assert_eq!(app.world.query::<Text>().collect(), entities);
}

#[compose]
fn qualified_calls(status: model!(Status), prefix: prop!(String)) -> (Entity, Entity) {
    let first = ui!(self::caption(${ format!("{}{}", prefix.get(), status.number()) }));
    let second = ui!(self::caption(${ status.number().to_string() }));
    (first, second)
}

#[test]
fn call_form_and_explicit_rust_inputs_keep_the_same_reading_scope() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let status = app.add_model(Status { number: 3 }).finish();
    app.compose(root, qualified_calls(status.clone(), "N"));
    assert_eq!(text_values(&app), ["N3", "3"]);
    status.set_number(4);
    flush_signal_dirty(&mut app.world);
    assert_eq!(text_values(&app), ["N4", "4"]);
    let source = Signal::new(String::from("Rust"));
    let input = source.clone();
    app.compose(root, caption(Prop::new(move || input.get())));
    source.set(String::from("Updated"));
    flush_signal_dirty(&mut app.world);
    assert_eq!(text_values(&app), ["N4", "4", "Updated"]);
}

#[compose]
fn typed<T: Clone>(value: prop!(T)) -> T {
    value.get()
}

#[compose]
fn configured(#[cfg(any())] _absent: prop!(NotDefined), #[cfg(test)] value: prop!(u32)) -> u32 {
    value.get()
}

#[compose]
fn rust_getter(content: impl Fn() -> String + 'static) -> Entity {
    ui! {
        Text (text: ${ content() })
    }
}

#[test]
fn ordinary_generics_cfg_and_rust_getters_keep_their_signatures() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, typed::<u32>(7_u32)), 7);
    assert_eq!(app.compose(root, typed::<String>("typed")), "typed");
    assert_eq!(app.compose(root, configured(11_u32)), 11);
    app.compose(root, rust_getter(|| String::from("closure")));
    assert_eq!(text_values(&app), ["closure"]);
}

#[compose]
fn numeric_inputs(shown: prop!(bool), width: prop!(Fixed), color: prop!(ColorToken)) -> Entity {
    ui! {
        View (visible: ${ shown }, width: ${ width }, bg_color: ${ color })
    }
}

#[test]
fn non_text_inputs_use_existing_property_updates() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let width = Signal::new(Fixed::from_int(12));
    let source = width.clone();
    let entity = app.compose(
        root,
        numeric_inputs(true, Prop::new(move || source.get()), ColorToken::Primary),
    );
    assert_eq!(
        app.world.get::<Style>(entity).unwrap().layout.width,
        Dimension::px(12)
    );
    width.set(Fixed::from_int(24));
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Style>(entity).unwrap().layout.width,
        Dimension::px(24)
    );
}

ui!(compose StatusCard(text: prop!(String)) {
    Column() {
        Text(text: ${text})
        @@body { Text("fallback") }
    }
});

ui!(compose Envelope(text: prop!(String)) {
    Column() {
        StatusCard(text: text) { @body { Text("slot") } }
        StatusCard(text: text)
    }
});

#[compose]
fn cards(text: model!(Signal<String>)) -> Entity {
    ui! {
        Column () {
            Envelope (text: ${ text.get() })
            Envelope (text: ${ text.get() })
        }
    }
}

#[test]
fn niche_inputs_keep_live_sources_across_instances_layers_and_slots() {
    let mut app = App::headless(64, 64);
    app.with_widget(ui!(compose StatusCard))
        .with_widget(ui!(compose Envelope));
    let root = app.spawn_root().id();
    let source = Signal::new(String::from("before"));
    app.compose(root, cards(source.clone()));
    let entities = app.world.query::<Text>().collect();
    let slots: Vec<_> = app.world.query::<mirui::ui::NicheMap>().collect();
    assert_eq!(
        text_values(&app)
            .iter()
            .filter(|text| text.as_str() == "before")
            .count(),
        4
    );
    assert_eq!(
        text_values(&app)
            .iter()
            .filter(|text| text.as_str() == "slot")
            .count(),
        2
    );
    assert_eq!(
        text_values(&app)
            .iter()
            .filter(|text| text.as_str() == "fallback")
            .count(),
        2
    );
    source.set(String::from("after"));
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        text_values(&app)
            .iter()
            .filter(|text| text.as_str() == "after")
            .count(),
        4
    );
    assert_eq!(app.world.query::<Text>().collect(), entities);
    assert_eq!(app.world.query::<mirui::ui::NicheMap>().collect(), slots);
    for entity in app.world.query::<StatusCard>().collect() {
        assert!(!app.world.has::<Text>(entity));
    }
}

#[compose]
fn conditional_card(shown: model!(Signal<bool>), text: model!(Signal<String>)) -> Entity {
    ui! {
        Column () {
            if ${ shown.get() } {
                StatusCard (text: ${ text.get() })
            } else {
                Text ("other")
            }
        }
    }
}

#[test]
fn hidden_niche_inputs_resume_with_the_latest_value() {
    let mut app = App::headless(64, 64);
    app.with_widget(ui!(compose StatusCard));
    let root = app.spawn_root().id();
    let shown = Signal::new(true);
    let text = Signal::new(String::from("visible"));
    app.compose(root, conditional_card(shown.clone(), text.clone()));
    let entities = app.world.query::<Text>().collect();
    shown.set(false);
    flush_signal_dirty(&mut app.world);
    text.set(String::from("latest"));
    flush_signal_dirty(&mut app.world);
    assert!(text_values(&app).iter().any(|value| value == "visible"));
    shown.set(true);
    flush_signal_dirty(&mut app.world);
    assert!(text_values(&app).iter().any(|value| value == "latest"));
    assert_eq!(app.world.query::<Text>().collect(), entities);
}

#[test]
fn niche_retirement_releases_inputs_and_never_updates_reused_entities() {
    let mut app = App::headless(64, 64);
    app.with_widget(ui!(compose StatusCard));
    let root = app.spawn_root().id();
    let source = Signal::new(String::from("old"));
    let input_source = source.clone();
    let capture = Rc::new(Cell::new(0));
    let retired = Rc::downgrade(&capture);
    let input = Prop::new(move || {
        capture.set(capture.get() + 1);
        input_source.get()
    });
    app.compose(root, |cx| {
        ui! {
            StatusCard (text: input)
        }
    });
    let removed: Vec<_> = app
        .world
        .query::<Widget>()
        .collect()
        .into_iter()
        .filter(|entity| *entity != root)
        .collect();
    for entity in removed {
        app.world.despawn(entity);
    }
    flush_signal_dirty(&mut app.world);
    assert!(retired.upgrade().is_none());
    app.compose(root, caption("replacement"));
    source.set(String::from("stale"));
    flush_signal_dirty(&mut app.world);
    assert_eq!(text_values(&app), ["replacement"]);
}

#[compose]
fn shadowed(text: prop!(String)) {
    ui! {
        Text (text: ${ text })
    };
    {
        let text = "local";
        ui! {
            Text (text: ${ text })
        };
    }
    let mut create = |text: &'static str| {
        ui! {
            Text (text: ${ text })
        }
    };
    create("closure");
    ui! {
        Text (text: ${ text })
    };
}

#[test]
fn local_values_keep_their_rust_meaning_when_shadowing_prop_names() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    app.compose(root, shadowed("prop"));
    assert_eq!(text_values(&app), ["prop", "local", "closure", "prop"]);
}
