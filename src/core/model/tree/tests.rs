use crate::core::model::{Model, ModelMetadata, ModelPath, ModelUpdate};
use crate::core::model::{ModelHandle, OwnedModel};
use crate::core::reactive::{Computed, with_world_scope};
use crate::test_support::tracked_allocations;
use alloc::rc::Rc;
use core::cell::Cell;

#[derive(Clone, Copy)]
struct Notice(u8);

#[derive(Clone, Copy)]
struct Change(bool);

impl Change {
    const VALUE: Self = Self(true);

    fn contains(self, other: Self) -> bool {
        self.0 && other.0
    }
}

#[crate::model(change = Change, watch(value = Change::VALUE))]
struct Leaf {
    #[observe]
    value: u16,
    notice: Option<Notice>,
    extract_increment: bool,
}

#[crate::model]
impl Leaf {
    fn new(value: u16) -> Self {
        Self {
            value,
            notice: None,
            extract_increment: false,
        }
    }

    fn change(&mut self, amount: u16, notice: u8) -> Change {
        self.value += amount;
        self.notice = Some(Notice(notice));
        Change::VALUE
    }

    fn finish_on_extract(&mut self) -> Change {
        self.extract_increment = true;
        Change(false)
    }

    #[effects]
    fn take_notice(&mut self) -> [Option<Notice>; 1] {
        if core::mem::take(&mut self.extract_increment) {
            self.value += 1;
        }
        [self.notice.take()]
    }
}

#[crate::model]
struct Pair {
    left: model!(owned Leaf),
    right: model!(owned Leaf),
    notice: Option<Notice>,
}

#[crate::model]
impl Pair {
    fn new() -> Self {
        Self {
            left: Leaf::new(1).into(),
            right: Leaf::new(10).into(),
            notice: None,
        }
    }

    #[observe]
    fn total(&self) -> u16 {
        self.left.value() + self.right.value()
    }

    fn change(&mut self) {
        self.left.change(1, 2);
        self.right.change(2, 3);
        self.notice = Some(Notice(1));
    }

    #[effects]
    fn take_notice(&mut self) -> [Option<Notice>; 1] {
        [self.notice.take()]
    }
}

#[test]
fn child_sources_publish_before_any_effect_and_extractor_writes_are_observed() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let total_read = model.clone();
    let total = with_world_scope(&mut app.world, || Computed::new(move || total_read.total()));
    let left_read = model.left();
    let left = with_world_scope(&mut app.world, || Computed::new(move || left_read.value()));
    assert_eq!((total.get(), left.get()), (11, 1));
    let order = Rc::new(Cell::new(0_u32));
    for node in 0..3 {
        let source = model.clone();
        let total = total.clone();
        let left = left.clone();
        let order = order.clone();
        let consumer = move |notice: Notice| {
            assert_eq!(total.get(), source.total());
            assert_eq!(left.get(), source.left().value());
            order.set(order.get() * 10 + u32::from(notice.0));
        };
        if node == 0 {
            app.on_effect(&model, consumer).unwrap();
        } else {
            let child = if node == 1 {
                model.left()
            } else {
                model.right()
            };
            app.on_effect(&child, consumer).unwrap();
        }
    }

    model.update(|value| {
        value.change();
        value.left.finish_on_extract();
    });
    assert_eq!((order.get(), total.get(), left.get()), (123, 15, 3));
    assert_eq!(model.left().value_revision(), 1);
    assert!(model.read(|value| value.left.storage().pending.iter().all(|pending| !pending)));
    order.set(0);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..32 {
                model.change();
                assert_eq!(order.get(), 123);
                order.set(0);
            }
        }),
        0
    );
}

#[derive(Clone, Copy)]
struct LeafPath(bool);

impl ModelPath<Pair> for LeafPath {
    type Target = Leaf;

    fn read<'a>(&self, value: &'a Pair) -> &'a Leaf {
        if self.0 {
            &value.right.storage().value
        } else {
            &value.left.storage().value
        }
    }

    fn write<'a>(&self, value: &'a mut Pair) -> &'a mut Leaf {
        if self.0 {
            &mut value.right.storage_mut().value
        } else {
            &mut value.left.storage_mut().value
        }
    }

    fn metadata<'a>(&self, root: &'a ModelMetadata<Pair>) -> &'a ModelMetadata<Leaf> {
        if self.0 {
            root.children().right.metadata()
        } else {
            root.children().left.metadata()
        }
    }

    fn update(
        &self,
        value: &mut Pair,
        _: &mut <Pair as Model>::PendingWatches,
        update: &mut ModelUpdate<'_, Leaf>,
    ) -> bool {
        let storage = if self.0 {
            value.right.storage_mut()
        } else {
            value.left.storage_mut()
        };
        let Some(change) = update(&mut storage.value) else {
            return false;
        };
        Leaf::record_change(&mut storage.pending, change);
        true
    }
}

#[test]
fn path_values_select_data_observations_and_effect_routes_without_hot_allocation() {
    use crate::core::model::{ModelAccess, projection::Projection};

    let mut world = crate::ecs::World::new();
    let (registration, parent) = super::super::register(&mut world, Pair::new());
    let accesses: [Rc<dyn ModelAccess<Leaf>>; 2] = [false, true].map(|right| {
        Rc::new(Projection {
            root: Rc::downgrade(&registration.root),
            path: LeafPath(right),
        }) as Rc<dyn ModelAccess<Leaf>>
    });
    let [left, right] = accesses
        .each_ref()
        .map(|access| Leaf::handle(Rc::downgrade(access)));
    let tracked = right.clone();
    let right_value = with_world_scope(&mut world, || Computed::new(move || tracked.value()));
    let delivered = Rc::new(Cell::new(0));
    let counter = delivered.clone();
    super::super::register_effect(world.id(), &right, move |notice: Notice| {
        counter.set(counter.get() + u16::from(notice.0));
    })
    .unwrap();
    assert_eq!((left.value(), right_value.get()), (1, 10));
    assert_eq!(
        tracked_allocations(|| {
            left.change(1, 7);
            right.change(2, 3);
            assert_eq!(
                (left.value(), right_value.get(), parent.total()),
                (2, 12, 14)
            );
            assert_eq!((left.value_revision(), right.value_revision()), (1, 1));
        }),
        0
    );
    assert_eq!(delivered.get(), 3);
    drop(registration);
    for access in &accesses {
        assert!(matches!(
            access.binding_owner().0,
            super::super::OwnerState::Invalid(_)
        ));
    }
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
        && value.left.read(|value| value.notice.is_none())
        && value.right.read(|value| value.notice.is_none())));
    assert!(model.read(|value| value.left.storage().pending.iter().all(|pending| !pending)));
    assert_eq!(
        (
            model.left().value_revision(),
            model.right().value_revision()
        ),
        (0, 0)
    );
    model.update(|_| {});
    assert_eq!(delivered.get(), 0);
    assert_eq!(
        (
            model.left().value_revision(),
            model.right().value_revision()
        ),
        (0, 0)
    );
}

#[test]
fn interrupted_effect_delivery_discards_all_retained_transaction_state() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let panic_once = Cell::new(true);
    app.on_effect(&model.left(), move |_: Notice| {
        assert!(!panic_once.replace(false), "consumer interrupted");
    })
    .unwrap();
    let delivered = Rc::new(Cell::new(0));
    let counter = delivered.clone();
    app.on_effect(&model.right(), move |notice: Notice| {
        counter.set(counter.get() + u16::from(notice.0));
    })
    .unwrap();

    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.change()));
    assert!(interrupted.is_err());
    assert_eq!(model.total(), 14);
    assert_eq!(delivered.get(), 0);
    model.access().upgrade().unwrap().metadata(&mut |metadata| {
        let transaction = metadata.transaction.borrow();
        assert!(transaction.before.is_none());
        assert!(transaction.after.is_none());
        assert!(transaction.events.is_none());
        for child in [&metadata.children().left, &metadata.children().right] {
            let transaction = child.metadata().transaction.borrow();
            assert!(transaction.before.is_none());
            assert!(transaction.after.is_none());
            assert!(transaction.events.is_none());
            assert!(transaction.pending.iter().all(|pending| !pending));
        }
    });
    model.update(|_| {});
    assert_eq!(delivered.get(), 0);
    model.change();
    assert_eq!(delivered.get(), 3);
}

#[test]
fn registration_retirement_recursively_invalidates_child_sources() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let observed = model.left();
    let child = with_world_scope(&mut app.world, || Computed::new(move || observed.value()));
    assert_eq!(child.get(), 1);
    let access = model.access().upgrade().unwrap();
    let child_access = model.left().access().upgrade().unwrap();
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
    assert!(matches!(
        child_access.binding_owner().0,
        super::super::OwnerState::Invalid(_)
    ));
}

#[test]
fn initial_binding_discards_pending_without_extracting_or_publishing() {
    let mut value = Pair::new();
    value.change();
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(value);
    let left = model.left();
    assert_eq!((left.value(), left.value_revision()), (2, 0));
    assert!(model.read(|value| value.left.storage().identity.is_some()));
    let delivered = Rc::new(Cell::new(0));
    let count = delivered.clone();
    app.on_effect(&left, move |notice: Notice| count.set(notice.0))
        .unwrap();
    model.update(|_| {});
    assert_eq!(left.value_revision(), 0);
    assert_eq!(delivered.get(), 2);
}

#[test]
fn metadata_preparation_does_not_require_default_or_live_business_data() {
    use crate::core::model::{BindingOwner, OwnerState, projection::RootPath};
    let world = crate::ecs::World::new();
    let metadata = ModelMetadata::<Pair>::register::<Pair, RootPath>(
        &alloc::rc::Weak::new(),
        BindingOwner(OwnerState::Bound(world.id())),
        RootPath,
        None,
    );
    for child in [&metadata.children().left, &metadata.children().right] {
        assert!(matches!(
            child.access.binding_owner().0,
            OwnerState::Invalid(_)
        ));
        let transaction = child.metadata().transaction.borrow();
        assert!(transaction.before.is_none());
        assert!(transaction.after.is_none());
        assert!(transaction.events.is_none());
    }
}

#[test]
fn child_adapters_do_not_retain_their_root() {
    let mut app = crate::app::App::headless(32, 32);
    let model = app.add_model(Pair::new());
    let child = model.left();
    let retained_child = child.access().upgrade().unwrap();
    assert_eq!(model.access().strong_count(), 1);
    drop(app);
    assert!(model.access().upgrade().is_none());
    assert!(matches!(
        retained_child.binding_owner().0,
        super::super::OwnerState::Invalid(_)
    ));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child.value())).is_err());
}
