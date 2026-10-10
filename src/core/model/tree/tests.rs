use super::{ModelChildren, ModelEvents, ModelSnapshot};
use crate::core::model::{
    BindingOwner, EffectRoute, Model, ModelAccess, ModelHandle, ModelMetadata, ModelMethods,
    ModelWatch, Produces,
};
use crate::core::reactive::{Computed, ModelSource, with_world_scope};
use crate::test_support::tracked_allocations;
use alloc::rc::{Rc, Weak};
use core::cell::Cell;

#[derive(Clone, Copy)]
struct Notice(u8);

#[crate::model]
struct Leaf {
    #[observe]
    value: u16,
    notice: Option<Notice>,
    pending: bool,
    extract_increment: bool,
}

#[crate::model]
impl Leaf {
    #[effects]
    fn take_notice(&mut self) -> [Option<Notice>; 1] {
        if self.extract_increment {
            self.value += 1;
            self.pending = true;
            self.extract_increment = false;
        }
        [self.notice.take()]
    }
}

struct Pair {
    left: Leaf,
    right: Leaf,
    notice: Option<Notice>,
}

impl Pair {
    fn new() -> Self {
        Self {
            left: Leaf {
                value: 1,
                notice: None,
                pending: false,
                extract_increment: false,
            },
            right: Leaf {
                value: 10,
                notice: None,
                pending: false,
                extract_increment: false,
            },
            notice: None,
        }
    }

    fn total(&self) -> u16 {
        self.left.value + self.right.value
    }

    fn change(&mut self) {
        self.left.value += 1;
        self.right.value += 2;
        self.left.pending = true;
        self.right.pending = true;
        self.notice = Some(Notice(1));
        self.left.notice = Some(Notice(2));
        self.right.notice = Some(Notice(3));
    }
}

struct PairChildren {
    left: ModelMetadata<Leaf>,
    right: ModelMetadata<Leaf>,
    published: Cell<u32>,
}

impl ModelChildren<Pair> for PairChildren {
    type Snapshot = (ModelSnapshot<Leaf>, ModelSnapshot<Leaf>);
    type Events = (ModelEvents<Leaf>, ModelEvents<Leaf>);
    type Pending = (bool, bool);

    fn new(owner: BindingOwner) -> Self {
        Self {
            left: ModelMetadata::new(owner),
            right: ModelMetadata::new(owner),
            published: Cell::new(0),
        }
    }

    fn snapshot(value: &Pair) -> Self::Snapshot {
        (
            ModelMetadata::snapshot(&value.left),
            ModelMetadata::snapshot(&value.right),
        )
    }

    fn take_events(value: &mut Pair) -> Self::Events {
        (
            ModelMetadata::take_events(&mut value.left),
            ModelMetadata::take_events(&mut value.right),
        )
    }

    fn finish(&self, value: &mut Pair) -> Self::Pending {
        self.left.finish(&mut value.left);
        self.right.finish(&mut value.right);
        (
            core::mem::take(&mut value.left.pending),
            core::mem::take(&mut value.right.pending),
        )
    }

    fn publish(&self, before: Self::Snapshot, after: Self::Snapshot, pending: Self::Pending) {
        self.left.publish(before.0, after.0, (), ());
        self.right.publish(before.1, after.1, (), ());
        if pending.0 || pending.1 {
            self.published.set(self.published.get() + 1);
        }
    }

    fn deliver(&self, events: Self::Events) {
        self.left.deliver(events.0);
        self.right.deliver(events.1);
    }

    fn invalidate(&self) {
        self.left.invalidate_computeds();
        self.right.invalidate_computeds();
    }
}

struct PairHandle(Weak<dyn ModelAccess<Pair>>);

impl Clone for PairHandle {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl ModelHandle for PairHandle {
    type Data = Pair;
    fn access(&self) -> &Weak<dyn ModelAccess<Pair>> {
        &self.0
    }
}

impl PairHandle {
    fn total(&self) -> u16 {
        self.read_derived(0, Pair::total)
    }

    fn left(&self) -> u16 {
        super::super::read_model(self, |value, metadata| {
            metadata.children.left.sources[0].track();
            value.left.value
        })
    }
}

impl Model for Pair {
    type Handle = PairHandle;
    type Snapshot = ();
    type Sources = [ModelSource; 0];
    type Change = ();
    type Watches = [ModelWatch; 0];
    type Children = PairChildren;
    fn handle(access: Weak<dyn ModelAccess<Self>>) -> Self::Handle {
        PairHandle(access)
    }
    fn snapshot(&self) {}
    fn sources() -> Self::Sources {
        []
    }
    fn publish(_: &Self::Sources, _: (), _: ()) {}
    fn watches() -> Self::Watches {
        []
    }
    fn publish_change(_: &Self::Watches, _: ()) {}
}

impl ModelMethods for Pair {
    type DerivedSnapshot = u16;
    type DerivedSources = [ModelSource; 1];
    type Events = [Option<Notice>; 1];
    type Routes = EffectRoute<Notice>;
    fn derived_snapshot(&self) -> u16 {
        self.total()
    }
    fn derived_sources() -> Self::DerivedSources {
        [ModelSource::new()]
    }
    fn publish_derived(sources: &Self::DerivedSources, before: u16, after: u16) {
        if before != after {
            sources[0].notify();
        }
    }
    fn take_events(&mut self) -> Self::Events {
        [self.notice.take()]
    }
    fn routes() -> Self::Routes {
        EffectRoute::new()
    }
    fn deliver_events(routes: &Self::Routes, events: Self::Events) {
        for notice in events.into_iter().flatten() {
            routes.deliver(notice);
        }
    }
}

impl Produces<Notice> for Pair {
    fn route(routes: &Self::Routes) -> &EffectRoute<Notice> {
        routes
    }
}

#[test]
fn child_sources_publish_before_any_effect_and_extractor_writes_are_observed() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let total_read = model.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || total_read.total()));
    let left_read = model.clone();
    let left = with_world_scope(&mut app.world, || Computed::new(move || left_read.left()));
    assert_eq!((total.get(), left.get()), (11, 1));
    let order = Rc::new(Cell::new(0_u32));
    for node in 0..3 {
        let source = model.clone();
        let total = total.clone();
        let left = left.clone();
        let order = order.clone();
        let consumer = move |notice: Notice| {
            assert_eq!(total.get(), source.total());
            assert_eq!(left.get(), source.left());
            order.set(order.get() * 10 + u32::from(notice.0));
        };
        if node == 0 {
            app.on_effect(&model, consumer).unwrap();
        } else {
            let mut consumer = Some(consumer);
            model.access().upgrade().unwrap().metadata(&mut |metadata| {
                let child = if node == 1 {
                    &metadata.children.left
                } else {
                    &metadata.children.right
                };
                <Leaf as Produces<Notice>>::route(&child.routes)
                    .register(consumer.take().unwrap())
                    .unwrap();
            });
        }
    }

    model.update(|value| {
        value.change();
        value.left.extract_increment = true;
    });
    assert_eq!((order.get(), total.get(), left.get()), (123, 15, 3));
    assert!(!model.read(|value| value.left.pending || value.right.pending));
    order.set(0);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..32 {
                model.update(Pair::change);
                assert_eq!(order.get(), 123);
                order.set(0);
            }
        }),
        0
    );
}

#[test]
fn rejected_updates_clear_child_events_and_pending_without_publication() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let observed = model.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || observed.total()));
    let delivered = Rc::new(Cell::new(0));
    let count = delivered.clone();
    app.on_effect(&model, move |_: Notice| count.set(count.get() + 1))
        .unwrap();
    assert_eq!(total.get(), 11);
    let result = model.try_update(|value| {
        value.change();
        Err("rejected")
    });
    assert_eq!(result, Err("rejected"));
    assert_eq!(model.total(), 14);
    assert_eq!(total.get(), 11);
    assert_eq!(delivered.get(), 0);
    assert!(model.read(|value| value.notice.is_none()
        && value.left.notice.is_none()
        && value.right.notice.is_none()));
    assert!(!model.read(|value| value.left.pending || value.right.pending));
    model
        .access()
        .upgrade()
        .unwrap()
        .metadata(&mut |metadata| assert_eq!(metadata.children.published.get(), 0));
    model.update(|_| {});
    assert_eq!(delivered.get(), 0);
    model
        .access()
        .upgrade()
        .unwrap()
        .metadata(&mut |metadata| assert_eq!(metadata.children.published.get(), 0));
}

#[test]
fn registration_retirement_recursively_invalidates_child_sources() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let observed = model.clone();
    let child = with_world_scope(&mut app.world, || Computed::new(move || observed.left()));
    assert_eq!(child.get(), 1);
    let access = model.access().upgrade().unwrap();
    let registration = app
        .world
        .query::<super::super::RegisteredModel<Pair>>()
        .iter()
        .next()
        .unwrap()
        .0;
    assert!(app.world.despawn(registration));
    let stale = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child.get()));
    assert!(stale.is_err());
    assert!(matches!(
        access.binding_owner().0,
        super::super::OwnerState::Invalid(_)
    ));
}
