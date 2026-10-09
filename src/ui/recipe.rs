use core::marker::PhantomData;

use crate::ecs::{Component, Entity, World};
use crate::input::event::binding::EntityEventBindings;

use super::property::Property;

pub(crate) mod sealed {
    pub trait Recipe {}
}

/// Static widget data and stack-held bindings consumed during construction.
#[doc(hidden)]
pub trait WidgetRecipe: sealed::Recipe {
    type Properties: PropertyBindings;
    type Events: EntityEventBindings;

    fn insert(self, world: &mut World, entity: Entity) -> (Self::Properties, Self::Events);
}

impl<T: Component> sealed::Recipe for T {}

impl<T: Component> WidgetRecipe for T {
    type Properties = ();
    type Events = ();

    fn insert(self, world: &mut World, entity: Entity) -> ((), ()) {
        world.insert(entity, self);
        ((), ())
    }
}

/// Installation-order property bindings retained by typed builders.
#[doc(hidden)]
pub trait PropertyBindings {
    fn install(self, world: &mut World, entity: Entity);
}

impl PropertyBindings for () {
    fn install(self, _world: &mut World, _entity: Entity) {}
}

pub(crate) struct PropertyBinding<P, F> {
    read: F,
    property: PhantomData<fn() -> P>,
}

impl<P, F> PropertyBinding<P, F> {
    pub(crate) fn new(read: F) -> Self {
        Self {
            read,
            property: PhantomData,
        }
    }
}

impl<P: Property, F: Fn() -> P::Value + 'static> PropertyBindings for PropertyBinding<P, F> {
    fn install(self, world: &mut World, entity: Entity) {
        bind_property::<P>(world, entity, self.read);
    }
}

pub(crate) fn bind_property<P: Property>(
    world: &mut World,
    entity: Entity,
    read: impl Fn() -> P::Value + 'static,
) {
    crate::core::reactive::with_world_scope(world, || {
        crate::core::reactive::effect_with_widget(entity, move || {
            super::property::apply::<P>(entity, read());
        });
    });
}

macro_rules! recipe_tuple {
    ($($item:ident),+) => {
        impl<$($item: WidgetRecipe),+> sealed::Recipe for ($($item,)+) {}

        impl<$($item: WidgetRecipe),+> WidgetRecipe for ($($item,)+) {
            type Properties = ($($item::Properties,)+);
            type Events = ($($item::Events,)+);

            fn insert(self, world: &mut World, entity: Entity) -> (Self::Properties, Self::Events) {
                #[allow(non_snake_case)]
                let ($($item,)+) = self;
                #[allow(non_snake_case)]
                let ($($item,)+) = ($($item.insert(world, entity),)+);
                (($($item.0,)+), ($($item.1,)+))
            }
        }

        impl<$($item: PropertyBindings),+> PropertyBindings for ($($item,)+) {
            fn install(self, world: &mut World, entity: Entity) {
                #[allow(non_snake_case)]
                let ($($item,)+) = self;
                $($item.install(world, entity);)+
            }
        }

    };
}

recipe_tuple!(A);
recipe_tuple!(A, B);
recipe_tuple!(A, B, C);
recipe_tuple!(A, B, C, D);
recipe_tuple!(A, B, C, D, E);
recipe_tuple!(A, B, C, D, E, F);
recipe_tuple!(A, B, C, D, E, F, G);
recipe_tuple!(A, B, C, D, E, F, G, H);

macro_rules! widget_builder_bindings {
    ($builder:ident, $widget:ty, $($field:ident),+) => {
        impl<P, B> $builder<P, B> {
            /// Bind a reactive property after static components and View attachment.
            pub fn bind_property<Q: $crate::ui::property::Property>(
                self,
                read: impl Fn() -> Q::Value + 'static,
            ) -> $builder<impl $crate::__macro_support::PropertyBindings, B>
            where
                P: $crate::__macro_support::PropertyBindings,
            {
                $builder {
                    $($field: self.$field,)+
                    properties: (self.properties, $crate::ui::recipe::PropertyBinding::<Q, _>::new(read)),
                    bindings: self.bindings,
                }
            }

            /// Install a typed callback after the initial reactive properties are applied.
            pub fn on_event<E>(
                self,
                key: $crate::input::event::contract::EventKey<E>,
                callback: $crate::__macro_support::EventCallback<E, $crate::__macro_support::EntityContext<$widget>>,
            ) -> $builder<P, (B, $crate::__macro_support::EventBinding<E, $crate::__macro_support::EntityContext<$widget>>)>
            where
                E: $crate::input::event::contract::Event,
                E::Route: $crate::__macro_support::EntityInstallRoute<E>,
                E::Source: $crate::input::event::contract::EntitySourceFor<$widget>,
            {
                $builder {
                    $($field: self.$field,)+
                    properties: self.properties,
                    bindings: (self.bindings, $crate::__macro_support::EventBinding::new(key, callback)),
                }
            }
        }

        impl<P, B> $crate::ui::recipe::sealed::Recipe for $builder<P, B> {}
    };
}

pub(crate) use widget_builder_bindings;
