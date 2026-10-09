use mirui::prelude::*;
use mirui::ui::WidgetTexture as Pixels;
use mirui::ui::outputs::{HistoryOutput, Output, OutputSource};

type Source = Entity;
type MaybeSource = Option<Source>;

#[component]
#[depends_on(Pixels)]
#[retain_previous(Pixels)]
struct Consumer {
    #[depends_on(Pixels)]
    source: Source,
    #[depends_on(Pixels, Pixels)]
    alternate: MaybeSource,
    unrelated: Entity,
}

#[component]
struct Generic<T: Output, S: OutputSource> {
    #[depends_on(T)]
    source: S,
    marker: core::marker::PhantomData<T>,
}

#[component]
#[retain_previous(T)]
struct Retained<T: HistoryOutput>(core::marker::PhantomData<T>);

#[component]
struct Conditional {
    #[cfg(any())]
    #[depends_on(UnavailableOutput)]
    removed: UnavailableSource,
    #[cfg_attr(all(), cfg(any()), depends_on(UnavailableOutput))]
    removed_nested: UnavailableSource,
    #[cfg_attr(all(), cfg_attr(all(), depends_on(Pixels)), doc = "source")]
    source: Source,
}

#[component]
#[cfg(any())]
#[depends_on(UnavailableOutput)]
struct Disabled(UnavailableSource);

#[component]
#[cfg_attr(all(), retain_previous(Pixels))]
struct Tuple(
    #[depends_on(Pixels)] Source,
    #[cfg(all())]
    #[depends_on(Pixels)]
    MaybeSource,
    #[cfg(any())] Unavailable,
);

#[component]
enum Variant {
    Empty,
    Named {
        #[depends_on(Pixels)]
        source: Source,
        #[cfg(any())]
        #[depends_on(UnavailableOutput)]
        removed: UnavailableSource,
    },
    Tuple(#[depends_on(Pixels)] MaybeSource),
    #[cfg(any())]
    Disabled(#[depends_on(UnavailableOutput)] UnavailableSource),
}

#[component]
enum SingleVariant {
    Source(#[depends_on(Pixels)] Source),
}

fn assert_component<T: Component>() {}

fn main() {
    assert_component::<Consumer>();
    assert_component::<Generic<Pixels, Source>>();
    assert_component::<Generic<Pixels, MaybeSource>>();
    assert_component::<Retained<Pixels>>();
    assert_component::<Conditional>();
    assert_component::<Tuple>();
    assert_component::<Variant>();
    assert_component::<SingleVariant>();
    assert_eq!(Consumer::OUTPUT_DEPENDENCIES.len(), 5);
    assert_eq!(Conditional::OUTPUT_DEPENDENCIES.len(), 1);
    assert_eq!(Tuple::OUTPUT_DEPENDENCIES.len(), 3);
    assert_eq!(Variant::OUTPUT_DEPENDENCIES.len(), 2);
}
