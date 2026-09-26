use mirui::core::model::{BindType, SharedValue};
use mirui::core::reactive::{Computed, Signal};
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
enum GenericState<T>
where
    T: Clone + 'static,
{
    Value(T),
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

type ModelAlias = LocalModel;

#[component(bind(model, optional))]
#[derive(::core::clone::Clone)]
struct AliasBoundDisplay {
    model: ModelAlias,
    optional: Option<Option<crate::LocalModel>>,
}

#[component(bind(model))]
#[derive(Clone)]
struct ConditionalBoundDisplay {
    #[cfg(any())]
    model: MissingWhenDisabled,
    label: u8,
}

#[cfg(unix)]
#[component(bind(model))]
#[derive(Clone)]
struct EnabledBoundDisplay {
    #[cfg(unix)]
    model: LocalModel,
}

#[cfg(unix)]
#[component(bind(model))]
#[derive(Clone)]
struct ConditionalGenericDisplay<M>
where
    M: BindType + 'static,
{
    #[cfg(unix)]
    model: M,
    marker: core::marker::PhantomData<M>,
}

#[test]
fn attribute_marks_structs_and_enums_as_components() {
    let mut world = World::new();
    let display = world.spawn(CounterDisplay { value: 3 });
    let state = world.spawn(State::Ready);
    let generic_state = world.spawn(GenericState::Value(11_u8));
    let generic = world.spawn(GenericDisplay { value: 8_u16 });

    assert_eq!(world.get::<CounterDisplay>(display).unwrap().value, 3);
    assert!(matches!(world.get::<State>(state), Some(State::Ready)));
    assert!(matches!(
        world.get::<GenericState<u8>>(generic_state),
        Some(GenericState::Value(11))
    ));
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

    let aliased = AliasBoundDisplay {
        model: model.clone(),
        optional: Some(Some(model.clone())),
    };
    aliased.clone().model.increment();
    assert!(aliased.optional.as_ref().unwrap().as_ref().is_some());

    let conditional = ConditionalBoundDisplay { label: 9 };
    assert_eq!(conditional.clone().label, 9);
    #[cfg(unix)]
    {
        let enabled = EnabledBoundDisplay {
            model: model.clone(),
        };
        enabled.clone().model.increment();
        assert_eq!(model.value(), 4);

        let generic_conditional = ConditionalGenericDisplay::<LocalModel> {
            model: model.clone(),
            marker: core::marker::PhantomData,
        };
        generic_conditional.clone().model.increment();
        assert_eq!(model.value(), 5);
    }
}

#[test]
fn reactive_sources_have_explicit_shared_type_mappings() {
    fn assert_shared<T: BindType<Shared = T> + SharedValue>() {}
    assert_shared::<Signal<u8>>();
    assert_shared::<Computed<u8>>();
    assert_shared::<Option<Signal<u8>>>();
    assert_shared::<Option<Option<Computed<u8>>>>();
}
