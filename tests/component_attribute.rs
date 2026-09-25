use mirui::prelude::*;

#[component]
struct CounterDisplay {
    value: u32,
}

#[component]
enum State {
    Ready,
}

#[component]
struct GenericDisplay<T>
where
    T: Clone + 'static,
{
    value: T,
}

#[model]
struct LocalModel {
    value: u32,
}

#[model]
impl LocalModel {
    fn increment(&mut self) {
        self.value += 1;
    }

    fn value(&self) -> u32 {
        self.value
    }
}

#[component(bind(model, optional))]
#[derive(Clone)]
struct BoundDisplay {
    model: LocalModel,
    optional: Option<LocalModel>,
    label: u8,
}

#[component(bind(model))]
#[derive(Clone)]
struct GenericBoundDisplay<M> {
    model: M,
}

#[test]
fn attribute_marks_structs_and_enums_as_components() {
    let mut world = World::new();
    let display = world.spawn(CounterDisplay { value: 3 });
    let state = world.spawn(State::Ready);
    let generic = world.spawn(GenericDisplay { value: 8_u16 });

    assert_eq!(world.get::<CounterDisplay>(display).unwrap().value, 3);
    assert!(matches!(world.get::<State>(state), Some(State::Ready)));
    assert_eq!(world.get::<GenericDisplay<u16>>(generic).unwrap().value, 8);
}

#[test]
fn bound_fields_hold_shared_instances_without_requiring_model_clone() {
    let mut app = App::headless(32, 32);
    let model = app.add_model(LocalModel { value: 0 });
    let board = BoundDisplay {
        model: model.clone(),
        optional: Some(model.clone()),
        label: 7,
    };
    let copy = board.clone();
    copy.model.increment();
    assert_eq!(board.label, 7);
    assert!(board.optional.is_some());
    assert_eq!(model.value(), 1);

    let generic = GenericBoundDisplay::<LocalModel> {
        model: model.clone(),
    };
    generic.clone().model.increment();
    assert_eq!(model.value(), 2);
}
