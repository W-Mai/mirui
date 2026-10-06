use core::any::{TypeId, type_name};
use core::marker::PhantomData;

use crate::ecs::bundle::Component;
use crate::ecs::entity::Entity;
use crate::ecs::sparse_set::SparseSetParts;
use crate::ecs::world::{ComponentChangeSink, SystemWorldCell, World};

mod sealed {
    pub trait QueryElement {}
    pub trait QueryData {}
    pub trait FilterElement {}
    pub trait QueryFilter {}
    pub trait QueryDriver {}
    pub trait QueryMutability {}
}

pub struct ReadOnly;
pub struct ReadWrite;

pub trait QueryMutability: sealed::QueryMutability {
    type Combine<Other: QueryMutability>: QueryMutability;
}

impl sealed::QueryMutability for ReadOnly {}
impl sealed::QueryMutability for ReadWrite {}

impl QueryMutability for ReadOnly {
    type Combine<Other: QueryMutability> = Other;
}

impl QueryMutability for ReadWrite {
    type Combine<Other: QueryMutability> = Self;
}

pub struct EntityItem;
pub struct Read<T>(PhantomData<fn() -> T>);
pub struct Write<T>(PhantomData<fn() -> T>);
pub struct OptionalRead<T>(PhantomData<fn() -> T>);
pub struct With<T>(PhantomData<fn() -> T>);
pub struct Without<T>(PhantomData<fn() -> T>);
pub struct ComponentDriver<T>(PhantomData<fn() -> T>);

/// Opaque component-storage state carried by generated system queries.
pub struct ComponentState<'world, T>(Option<SparseSetParts<'world, T>>);

impl<'world, T: Component> ComponentState<'world, T> {
    fn from_shared(world: &'world World) -> Self {
        Self(world.storage::<T>().map(|storage| {
            // SAFETY: the shared World borrow prevents structural mutation for
            // 'world. Shared initialization is only exposed to read-only data
            // and presence-only drivers/filters, so all live value references
            // remain shared.
            unsafe { storage.parts() }
        }))
    }
}

pub struct WriteState<'world, T> {
    components: Option<SparseSetParts<'world, T>>,
    changes: ComponentChangeSink<'world>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Clone, Copy)]
pub struct QueryAccess {
    type_id: TypeId,
    type_name: &'static str,
    mode: AccessMode,
}

impl QueryAccess {
    fn read<T: Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            mode: AccessMode::Read,
        }
    }

    fn write<T: Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            mode: AccessMode::Write,
        }
    }

    fn conflicts_with(self, other: Self) -> bool {
        self.type_id == other.type_id
            && (self.mode == AccessMode::Write || other.mode == AccessMode::Write)
    }
}

/// One value in the flat row yielded by a typed system query.
///
/// # Safety
///
/// Implementations must only create references into their captured sparse-set
/// parts. Mutable items must remain exclusive for their returned lifetime,
/// and every write must be reported before any parts are captured.
pub unsafe trait QueryElement: sealed::QueryElement {
    type State<'world>;
    type Item<'w>;
    type Mutability: QueryMutability;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess));

    /// Return the conservative number of component-change records that may be
    /// produced while yielding at most `max_rows` rows. Read-only elements use
    /// the default zero budget.
    ///
    /// # Safety
    ///
    /// `cell` must own the same exclusive World window used by `init`.
    unsafe fn pending_changes<'world>(_cell: &SystemWorldCell<'world>, _max_rows: usize) -> usize {
        0
    }

    /// # Safety
    ///
    /// All query accesses must have been checked for conflicts first. No World
    /// structure may change until the returned state is dropped.
    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world>;

    /// # Safety
    ///
    /// `state` must still refer to structurally unchanged sparse sets branded
    /// with the same `'world` lifetime.
    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool;

    /// # Safety
    ///
    /// The row must have passed `matches`. All references previously yielded
    /// from the same query must remain compatible with this element's access.
    unsafe fn fetch<'item, 'world>(
        state: &Self::State<'world>,
        entity: Entity,
    ) -> Self::Item<'item>
    where
        'world: 'item;
}

pub(crate) trait SharedQueryElement: QueryElement<Mutability = ReadOnly> {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryElement>::State<'world>;
}

impl sealed::QueryElement for EntityItem {}

impl SharedQueryElement for EntityItem {
    fn init_shared<'world>(_world: &'world World) -> <Self as QueryElement>::State<'world> {}
}

unsafe impl QueryElement for EntityItem {
    type State<'world> = ();
    type Item<'w> = Entity;
    type Mutability = ReadOnly;

    fn visit_access(_visitor: &mut dyn FnMut(QueryAccess)) {}

    unsafe fn init<'world>(_cell: &SystemWorldCell<'world>) -> Self::State<'world> {}

    unsafe fn matches<'world>(_state: &Self::State<'world>, _entity: Entity) -> bool {
        true
    }

    unsafe fn fetch<'item, 'world>(
        _state: &Self::State<'world>,
        entity: Entity,
    ) -> Self::Item<'item>
    where
        'world: 'item,
    {
        entity
    }
}

impl<T> sealed::QueryElement for Read<T> {}

impl<T: Component> SharedQueryElement for Read<T> {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryElement>::State<'world> {
        ComponentState::from_shared(world)
    }
}

unsafe impl<T: Component> QueryElement for Read<T> {
    type State<'world> = ComponentState<'world, T>;
    type Item<'w> = &'w T;
    type Mutability = ReadOnly;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess)) {
        visitor(QueryAccess::read::<T>());
    }

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: delegated from QueryElement::init.
        ComponentState(unsafe { cell.component_parts::<T>() })
    }

    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool {
        state.0.is_some_and(|parts| parts.contains(entity))
    }

    unsafe fn fetch<'item, 'world>(state: &Self::State<'world>, entity: Entity) -> Self::Item<'item>
    where
        'world: 'item,
    {
        // SAFETY: the required component matched and query validation excludes
        // a simultaneous write to T.
        unsafe {
            state
                .0
                .expect("required query storage")
                .get(entity)
                .expect("required query component")
        }
    }
}

impl<T> sealed::QueryElement for Write<T> {}

unsafe impl<T: Component> QueryElement for Write<T> {
    type State<'world> = WriteState<'world, T>;
    type Item<'w> = &'w mut T;
    type Mutability = ReadWrite;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess)) {
        visitor(QueryAccess::write::<T>());
    }

    unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>, max_rows: usize) -> usize {
        // SAFETY: delegated from QueryElement::pending_changes.
        if unsafe { cell.watches_component::<T>() } {
            max_rows
        } else {
            0
        }
    }

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: delegated from QueryElement::init.
        WriteState {
            components: unsafe { cell.component_parts::<T>() },
            changes: unsafe { cell.component_change_sink() },
        }
    }

    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool {
        state.components.is_some_and(|parts| parts.contains(entity))
    }

    unsafe fn fetch<'item, 'world>(state: &Self::State<'world>, entity: Entity) -> Self::Item<'item>
    where
        'world: 'item,
    {
        // SAFETY: this state and component reference originate from the same
        // SystemWorldCell; the change queue is disjoint from component storage.
        unsafe { state.changes.record::<T>(entity) };
        // SAFETY: the row matched and the caller guarantees exclusive access
        // to this slot for the returned lifetime.
        unsafe {
            state
                .components
                .expect("required query storage")
                .get_mut(entity)
                .expect("required query component")
        }
    }
}

impl<T> sealed::QueryElement for OptionalRead<T> {}

impl<T: Component> SharedQueryElement for OptionalRead<T> {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryElement>::State<'world> {
        ComponentState::from_shared(world)
    }
}

unsafe impl<T: Component> QueryElement for OptionalRead<T> {
    type State<'world> = ComponentState<'world, T>;
    type Item<'w> = Option<&'w T>;
    type Mutability = ReadOnly;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess)) {
        visitor(QueryAccess::read::<T>());
    }

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: delegated from QueryElement::init.
        ComponentState(unsafe { cell.component_parts::<T>() })
    }

    unsafe fn matches<'world>(_state: &Self::State<'world>, _entity: Entity) -> bool {
        true
    }

    unsafe fn fetch<'item, 'world>(state: &Self::State<'world>, entity: Entity) -> Self::Item<'item>
    where
        'world: 'item,
    {
        // SAFETY: query validation excludes a simultaneous write to T.
        unsafe { state.0.and_then(|parts| parts.get(entity)) }
    }
}

/// Produces one flat tuple row from a set of query elements.
///
/// # Safety
///
/// Implementations must initialize, match, and fetch every element exactly
/// once and preserve the access declarations reported by those elements.
pub unsafe trait QueryData: sealed::QueryData {
    type State<'world>;
    type Item<'w>;
    type Mutability: QueryMutability;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess));

    /// # Safety
    ///
    /// `cell` must own the same exclusive World window used by `init`.
    unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>, max_rows: usize) -> usize;

    /// # Safety
    ///
    /// All element accesses must have been checked for conflicts first.
    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world>;

    /// # Safety
    ///
    /// `state` must still refer to structurally unchanged sparse sets.
    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool;

    /// # Safety
    ///
    /// The row must match and all previously yielded references must remain
    /// compatible with the declared element accesses.
    unsafe fn fetch<'item, 'world>(
        state: &Self::State<'world>,
        entity: Entity,
    ) -> Self::Item<'item>
    where
        'world: 'item;
}

pub(crate) trait SharedQueryData: QueryData<Mutability = ReadOnly> {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryData>::State<'world>;
}

impl<E: SharedQueryElement> SharedQueryData for E {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryData>::State<'world> {
        <E as SharedQueryElement>::init_shared(world)
    }
}

impl<E: QueryElement> sealed::QueryData for E {}

unsafe impl<E: QueryElement> QueryData for E {
    type State<'world> = E::State<'world>;
    type Item<'w> = E::Item<'w>;
    type Mutability = E::Mutability;

    fn visit_access(visitor: &mut dyn FnMut(QueryAccess)) {
        E::visit_access(visitor);
    }

    unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>, max_rows: usize) -> usize {
        // SAFETY: delegated to the single query element.
        unsafe { E::pending_changes(cell, max_rows) }
    }

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: delegated from QueryData::init to the single element.
        unsafe { E::init(cell) }
    }

    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool {
        // SAFETY: delegated from QueryData::matches to the single element.
        unsafe { E::matches(state, entity) }
    }

    unsafe fn fetch<'item, 'world>(state: &Self::State<'world>, entity: Entity) -> Self::Item<'item>
    where
        'world: 'item,
    {
        // SAFETY: delegated from QueryData::fetch to the single element.
        unsafe { E::fetch(state, entity) }
    }
}

macro_rules! query_mutability {
    ($element:ident) => { $element::Mutability };
    ($first:ident, $($rest:ident),+) => {
        <$first::Mutability as QueryMutability>::Combine<query_mutability!($($rest),+)>
    };
}

macro_rules! impl_query_data {
    ($(($element:ident, $index:tt)),+ $(,)?) => {
        impl<$($element: QueryElement),+> sealed::QueryData for ($($element,)+) {}

        impl<$($element: SharedQueryElement),+> SharedQueryData for ($($element,)+) {
            fn init_shared<'world>(
                world: &'world World,
            ) -> <Self as QueryData>::State<'world> {
                ($(<$element as SharedQueryElement>::init_shared(world),)+)
            }
        }

        unsafe impl<$($element: QueryElement),+> QueryData for ($($element,)+) {
            type State<'world> = ($($element::State<'world>,)+);
            type Item<'w> = ($($element::Item<'w>,)+);
            type Mutability = query_mutability!($($element),+);

            fn visit_access(visitor: &mut dyn FnMut(QueryAccess)) {
                $($element::visit_access(visitor);)+
            }

            unsafe fn pending_changes<'world>(
                cell: &SystemWorldCell<'world>,
                max_rows: usize,
            ) -> usize {
                let mut pending = 0usize;
                // SAFETY: delegated to each query element. Each write type has
                // a distinct access after aggregate validation.
                $(
                    pending = pending.checked_add(unsafe {
                        $element::pending_changes(cell, max_rows)
                    }).expect("component change budget overflow");
                )+
                pending
            }

            unsafe fn init<'world>(
                cell: &SystemWorldCell<'world>,
            ) -> Self::State<'world> {
                // SAFETY: delegated from QueryData::init for every element.
                ($(unsafe { $element::init(cell) },)+)
            }

            unsafe fn matches<'world>(
                state: &Self::State<'world>,
                entity: Entity,
            ) -> bool {
                // SAFETY: delegated from QueryData::matches for every element.
                true $(&& unsafe { $element::matches(&state.$index, entity) })+
            }

            unsafe fn fetch<'item, 'world>(
                state: &Self::State<'world>,
                entity: Entity,
            ) -> Self::Item<'item>
            where
                'world: 'item,
            {
                // SAFETY: delegated from QueryData::fetch for every element;
                // tuple fields are disjoint and access validation already ran.
                ($(unsafe { $element::fetch(&state.$index, entity) },)+)
            }
        }
    };
}

impl_query_data!((A, 0), (B, 1));
impl_query_data!((A, 0), (B, 1), (C, 2));
impl_query_data!((A, 0), (B, 1), (C, 2), (D, 3));
impl_query_data!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4));
impl_query_data!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5));
impl_query_data!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5), (G, 6));
impl_query_data!(
    (A, 0),
    (B, 1),
    (C, 2),
    (D, 3),
    (E, 4),
    (F, 5),
    (G, 6),
    (H, 7)
);

/// One presence-only query filter.
///
/// # Safety
///
/// Implementations may inspect only captured structural metadata; they must
/// never create a component value reference.
pub unsafe trait FilterElement: sealed::FilterElement {
    type State<'world>;

    /// # Safety
    ///
    /// No World structure may change until the returned state is dropped.
    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world>;

    /// # Safety
    ///
    /// `state` must still refer to structurally unchanged sparse sets.
    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool;
}

pub(crate) trait SharedFilterElement: FilterElement {
    fn init_shared<'world>(world: &'world World) -> <Self as FilterElement>::State<'world>;
}

impl<T> sealed::FilterElement for With<T> {}

impl<T: Component> SharedFilterElement for With<T> {
    fn init_shared<'world>(world: &'world World) -> <Self as FilterElement>::State<'world> {
        ComponentState::from_shared(world)
    }
}

unsafe impl<T: Component> FilterElement for With<T> {
    type State<'world> = ComponentState<'world, T>;

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: filters retain only structural raw parts.
        ComponentState(unsafe { cell.component_parts::<T>() })
    }

    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool {
        state.0.is_some_and(|parts| parts.contains(entity))
    }
}

impl<T> sealed::FilterElement for Without<T> {}

impl<T: Component> SharedFilterElement for Without<T> {
    fn init_shared<'world>(world: &'world World) -> <Self as FilterElement>::State<'world> {
        ComponentState::from_shared(world)
    }
}

unsafe impl<T: Component> FilterElement for Without<T> {
    type State<'world> = ComponentState<'world, T>;

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: filters retain only structural raw parts.
        ComponentState(unsafe { cell.component_parts::<T>() })
    }

    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool {
        state.0.is_none_or(|parts| !parts.contains(entity))
    }
}

/// Conjunction of all presence-only filters for one query.
///
/// # Safety
///
/// Implementations must preserve every filter's structural-only contract.
pub unsafe trait QueryFilter: sealed::QueryFilter {
    type State<'world>;

    /// # Safety
    ///
    /// No World structure may change until the returned state is dropped.
    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world>;

    /// # Safety
    ///
    /// `state` must still refer to structurally unchanged sparse sets.
    unsafe fn matches<'world>(state: &Self::State<'world>, entity: Entity) -> bool;
}

pub(crate) trait SharedQueryFilter: QueryFilter {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryFilter>::State<'world>;
}

impl sealed::QueryFilter for () {}

impl SharedQueryFilter for () {
    fn init_shared<'world>(_world: &'world World) -> <Self as QueryFilter>::State<'world> {}
}

unsafe impl QueryFilter for () {
    type State<'world> = ();

    unsafe fn init<'world>(_cell: &SystemWorldCell<'world>) -> Self::State<'world> {}

    unsafe fn matches<'world>(_state: &Self::State<'world>, _entity: Entity) -> bool {
        true
    }
}

macro_rules! impl_query_filter {
    ($(($filter:ident, $index:tt)),+ $(,)?) => {
        impl<$($filter: FilterElement),+> sealed::QueryFilter for ($($filter,)+) {}

        impl<$($filter: SharedFilterElement),+> SharedQueryFilter for ($($filter,)+) {
            fn init_shared<'world>(
                world: &'world World,
            ) -> <Self as QueryFilter>::State<'world> {
                ($(<$filter as SharedFilterElement>::init_shared(world),)+)
            }
        }

        unsafe impl<$($filter: FilterElement),+> QueryFilter for ($($filter,)+) {
            type State<'world> = ($($filter::State<'world>,)+);

            unsafe fn init<'world>(
                cell: &SystemWorldCell<'world>,
            ) -> Self::State<'world> {
                // SAFETY: delegated from QueryFilter::init for every filter.
                ($(unsafe { $filter::init(cell) },)+)
            }

            unsafe fn matches<'world>(
                state: &Self::State<'world>,
                entity: Entity,
            ) -> bool {
                // SAFETY: delegated from QueryFilter::matches for every filter.
                true $(&& unsafe { $filter::matches(&state.$index, entity) })+
            }
        }
    };
}

impl_query_filter!((A, 0));
impl_query_filter!((A, 0), (B, 1));
impl_query_filter!((A, 0), (B, 1), (C, 2));
impl_query_filter!((A, 0), (B, 1), (C, 2), (D, 3));
impl_query_filter!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4));
impl_query_filter!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5));
impl_query_filter!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5), (G, 6));
impl_query_filter!(
    (A, 0),
    (B, 1),
    (C, 2),
    (D, 3),
    (E, 4),
    (F, 5),
    (G, 6),
    (H, 7)
);

/// Dense entity source chosen from a required query item or `with` filter.
///
/// # Safety
///
/// The driver must yield each captured entity at most once and may not expose
/// a component value reference.
pub unsafe trait QueryDriver: sealed::QueryDriver {
    type State<'world>;

    /// # Safety
    ///
    /// No World structure may change until the returned state is dropped.
    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world>;
    fn len(state: &Self::State<'_>) -> usize;
    fn contains(state: &Self::State<'_>, entity: Entity) -> bool;

    /// # Safety
    ///
    /// `index` must be below `len(state)`, and `state` must remain structurally
    /// valid.
    unsafe fn entity(state: &Self::State<'_>, index: usize) -> Entity;
}

pub(crate) trait SharedQueryDriver: QueryDriver {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryDriver>::State<'world>;
}

impl<T> sealed::QueryDriver for ComponentDriver<T> {}

impl<T: Component> SharedQueryDriver for ComponentDriver<T> {
    fn init_shared<'world>(world: &'world World) -> <Self as QueryDriver>::State<'world> {
        ComponentState::from_shared(world)
    }
}

unsafe impl<T: Component> QueryDriver for ComponentDriver<T> {
    type State<'world> = ComponentState<'world, T>;

    unsafe fn init<'world>(cell: &SystemWorldCell<'world>) -> Self::State<'world> {
        // SAFETY: drivers retain only the dense entity raw parts.
        ComponentState(unsafe { cell.component_parts::<T>() })
    }

    fn len(state: &Self::State<'_>) -> usize {
        state.0.map_or(0, SparseSetParts::len)
    }

    fn contains(state: &Self::State<'_>, entity: Entity) -> bool {
        state.0.is_some_and(|parts| parts.contains(entity))
    }

    unsafe fn entity(state: &Self::State<'_>, index: usize) -> Entity {
        // SAFETY: SystemQueryIter checks index against QueryDriver::len.
        unsafe {
            state
                .0
                .expect("query driver storage")
                .entity_unchecked(index)
        }
    }
}

fn assert_compatible_access<Q: QueryData>() {
    let mut left_index = 0;
    Q::visit_access(&mut |left| {
        let current_left = left_index;
        left_index += 1;
        let mut right_index = 0;
        Q::visit_access(&mut |right| {
            if right_index > current_left && left.conflicts_with(right) {
                panic!("conflicting query access to component `{}`", left.type_name);
            }
            right_index += 1;
        });
    });
}

/// Internal typed query used by generated systems.
///
/// It retains a stable World borrow for `'world`; only pointer snapshots live in its state. Shared construction accepts only sealed read-only data. Mutable construction holds an exclusive window through its input World borrow. Iteration cannot revisit World or structurally modify storage.
pub struct SystemQuery<
    'world,
    D: QueryDriver,
    Q: QueryData,
    F: QueryFilter,
    M: QueryMutability = <Q as QueryData>::Mutability,
> {
    driver: D::State<'world>,
    data: Q::State<'world>,
    filters: F::State<'world>,
    _world: PhantomData<(&'world World, M)>,
}

impl<'world, D: QueryDriver, Q: QueryData, F: QueryFilter> SystemQuery<'world, D, Q, F> {
    pub(crate) fn new(world: &'world mut World) -> Self {
        assert_compatible_access::<Q>();
        let cell = SystemWorldCell::new(world);
        // SAFETY: access compatibility was checked before any raw parts were
        // captured. The returned query exclusively borrows World for `'world`.
        unsafe { Self::new_unchecked(&cell) }
    }

    /// # Safety
    ///
    /// The complete system parameter set must have been checked for component
    /// access conflicts. No path available to the system body may structurally
    /// mutate World until the query has been dropped.
    pub(crate) unsafe fn new_unchecked(cell: &SystemWorldCell<'world>) -> Self {
        // SAFETY: guaranteed by the caller for the complete query state.
        let driver = unsafe { D::init(cell) };
        // SAFETY: the change queue is disjoint from captured component storage.
        let pending = unsafe { Q::pending_changes(cell, D::len(&driver)) };
        // SAFETY: this query owns the exclusive World window.
        unsafe { cell.reserve_component_change_records(pending) };
        Self {
            driver,
            // SAFETY: guaranteed by the caller for the complete query state.
            data: unsafe { Q::init(cell) },
            // SAFETY: filters only inspect structural metadata.
            filters: unsafe { F::init(cell) },
            _world: PhantomData,
        }
    }

    fn matches(&self, entity: Entity) -> bool {
        // SAFETY: all captured states share the query's stable World window.
        D::contains(&self.driver, entity)
            && unsafe { F::matches(&self.filters, entity) && Q::matches(&self.data, entity) }
    }
}

impl<'world, D, Q, F> SystemQuery<'world, D, Q, F, ReadOnly>
where
    D: QueryDriver,
    Q: QueryData<Mutability = ReadOnly>,
    F: QueryFilter,
{
    /// Capture existing read-only state without opening a system mutation window.
    pub(crate) fn from_shared(world: &'world World) -> Self
    where
        D: SharedQueryDriver,
        Q: SharedQueryData,
        F: SharedQueryFilter,
    {
        // Sealed read-only data cannot contain conflicting writes.
        Self {
            driver: <D as SharedQueryDriver>::init_shared(world),
            data: <Q as SharedQueryData>::init_shared(world),
            filters: <F as SharedQueryFilter>::init_shared(world),
            _world: PhantomData,
        }
    }

    /// Read one matching row for the lifetime of this query borrow.
    pub fn get(&self, entity: Entity) -> Option<Q::Item<'_>> {
        if !self.matches(entity) {
            return None;
        }
        // SAFETY: the row matches; sealed read-only data cannot yield writes.
        Some(unsafe { Q::fetch(&self.data, entity) })
    }
}

impl<'world, D, Q, F> SystemQuery<'world, D, Q, F, ReadWrite>
where
    D: QueryDriver,
    Q: QueryData<Mutability = ReadWrite>,
    F: QueryFilter,
{
    /// Borrow one matching row exclusively until the returned item is released.
    pub fn get(&mut self, entity: Entity) -> Option<Q::Item<'_>> {
        if !self.matches(entity) {
            return None;
        }
        // SAFETY: the exclusive query reborrow protects every returned write.
        Some(unsafe { Q::fetch(&self.data, entity) })
    }
}

pub struct SystemQueryIter<'world, D: QueryDriver, Q: QueryData, F: QueryFilter> {
    query: SystemQuery<'world, D, Q, F>,
    cursor: usize,
}

impl<'world, D: QueryDriver, Q: QueryData, F: QueryFilter> Iterator
    for SystemQueryIter<'world, D, Q, F>
{
    type Item = Q::Item<'world>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor < D::len(&self.query.driver) {
            let index = self.cursor;
            self.cursor += 1;
            // SAFETY: index was checked against the captured driver length.
            let entity = unsafe { D::entity(&self.query.driver, index) };
            // SAFETY: all states were captured from the same structurally
            // stable World and remain alive for the iterator.
            let matches = unsafe {
                F::matches(&self.query.filters, entity) && Q::matches(&self.query.data, entity)
            };
            if matches {
                // SAFETY: the row matched every required element; driver
                // uniqueness plus access validation protects yielded writes.
                return Some(unsafe { Q::fetch(&self.query.data, entity) });
            }
        }
        None
    }
}

impl<'world, D: QueryDriver, Q: QueryData, F: QueryFilter> IntoIterator
    for SystemQuery<'world, D, Q, F>
{
    type Item = Q::Item<'world>;
    type IntoIter = SystemQueryIter<'world, D, Q, F>;

    fn into_iter(self) -> Self::IntoIter {
        SystemQueryIter {
            query: self,
            cursor: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::bundle::Component;
    use crate::test_support::tracked_allocations;

    #[derive(Debug, PartialEq, Eq)]
    struct Position(i32);

    impl Component for Position {}

    #[derive(Debug, PartialEq, Eq)]
    struct Velocity(i32);

    impl Component for Velocity {}

    #[derive(Debug, PartialEq, Eq)]
    struct Label(&'static str);

    impl Component for Label {}

    struct Active;

    impl Component for Active {}

    struct Hidden;

    impl Component for Hidden {}

    struct Marker;

    impl Component for Marker {}

    #[test]
    fn shared_queries_coexist_and_preserve_pointer_optional_filter_and_generation_contracts() {
        let mut world = World::new();
        let expired = world.spawn_empty();
        world.insert(expired, Position(-1));
        assert!(world.despawn(expired));
        let first = world.spawn_empty();
        assert_eq!(first.id, expired.id);
        assert_ne!(first.generation, expired.generation);
        world.insert(first, Position(1));
        world.insert(first, Velocity(10));
        world.insert(first, Active);
        let second = world.spawn_empty();
        world.insert(second, Position(2));
        world.insert(second, Label("two"));
        world.insert(second, Active);
        let hidden = world.spawn_empty();
        world.insert(hidden, Position(3));
        world.insert(hidden, Active);
        world.insert(hidden, Hidden);
        let inactive = world.spawn_empty();
        world.insert(inactive, Position(4));
        let outside_driver = world.spawn_empty();
        world.insert(outside_driver, Velocity(50));
        world.insert(outside_driver, Active);
        world.watch_component_type(TypeId::of::<Position>());
        world.drain_component_changes(|_, _| {});

        type Rows<'world> = SystemQuery<
            'world,
            ComponentDriver<Position>,
            (
                EntityItem,
                Read<Position>,
                Read<Position>,
                OptionalRead<Velocity>,
                OptionalRead<Label>,
                OptionalRead<Marker>,
            ),
            (With<Active>, Without<Hidden>),
        >;
        type Other<'world> =
            SystemQuery<'world, ComponentDriver<Active>, Read<Position>, (Without<Hidden>,)>;
        type Only<'world> = SystemQuery<'world, ComponentDriver<Active>, EntityItem, ()>;
        type Missing<'world> =
            SystemQuery<'world, ComponentDriver<Marker>, OptionalRead<Position>, ()>;
        {
            let rows = Rows::from_shared(&world);
            let other = Other::from_shared(&world);
            let only = Only::from_shared(&world);
            let missing = Missing::from_shared(&world);
            {
                let (entity, left, right, velocity, label, marker) = rows.get(first).unwrap();
                let concurrent = other.get(first).unwrap();
                let repeated = rows.get(first).unwrap().1;
                assert_eq!(entity, first);
                assert!(core::ptr::eq(left, right));
                assert!(core::ptr::eq(left, concurrent));
                assert!(core::ptr::eq(left, repeated));
                assert!(core::ptr::eq(left, world.get::<Position>(first).unwrap()));
                assert!(core::ptr::eq(
                    velocity.unwrap(),
                    world.get::<Velocity>(first).unwrap()
                ));
                assert!(label.is_none());
                assert!(marker.is_none());
            }
            let (_, _, _, velocity, label, marker) = rows.get(second).unwrap();
            assert!(velocity.is_none());
            assert!(core::ptr::eq(
                label.unwrap(),
                world.get::<Label>(second).unwrap()
            ));
            assert!(marker.is_none());
            assert!(rows.get(hidden).is_none());
            assert!(rows.get(inactive).is_none());
            assert!(rows.get(outside_driver).is_none());
            assert!(rows.get(expired).is_none());
            assert!(other.get(outside_driver).is_none());
            assert!(other.get(expired).is_none());
            assert_eq!(only.get(outside_driver), Some(outside_driver));
            assert!(missing.get(first).is_none());
            assert_eq!(missing.into_iter().count(), 0);
            let mut iter = rows.into_iter();
            assert_eq!(iter.next().map(|row| row.0), Some(first));
            assert_eq!(iter.next().map(|row| row.0), Some(second));
            assert!(iter.next().is_none());
            assert_eq!(other.get(first).unwrap(), &Position(1));
        }
        let mut changes = 0;
        world.drain_component_changes(|_, _| changes += 1);
        assert_eq!(changes, 0);
    }

    #[test]
    fn shared_query_first_and_repeated_construction_get_and_iteration_do_not_allocate() {
        let mut world = World::new();
        let first = world.spawn_empty();
        world.insert(first, Position(1));
        world.insert(first, Velocity(10));
        world.insert(first, Active);
        let second = world.spawn_empty();
        world.insert(second, Position(2));
        world.insert(second, Active);
        type Rows<'world> = SystemQuery<
            'world,
            ComponentDriver<Position>,
            (EntityItem, Read<Position>, OptionalRead<Velocity>),
            (With<Active>, Without<Hidden>),
        >;
        for repetitions in [1, 64] {
            assert_eq!(
                tracked_allocations(|| {
                    for _ in 0..repetitions {
                        let first_query = Rows::from_shared(&world);
                        let second_query = Rows::from_shared(&world);
                        {
                            let left = first_query.get(first).unwrap();
                            let right = second_query.get(first).unwrap();
                            assert!(core::ptr::eq(left.1, right.1));
                            assert!(core::ptr::eq(left.2.unwrap(), right.2.unwrap()));
                        }
                        assert!(first_query.get(second).unwrap().2.is_none());
                        assert_eq!(first_query.into_iter().count(), 2);
                        assert_eq!(second_query.into_iter().count(), 2);
                    }
                }),
                0
            );
        }
    }

    #[crate::animate(component = GeneratedInventoryMotion)]
    fn apply_generated_inventory_motion(_value: motion!()) {}

    fn assert_component<T: Component>() {}

    #[test]
    fn framework_system_query_inventory_is_component_typed() {
        assert_component::<crate::core::timer::Timer>();
        assert_component::<crate::ui::InteractionState>();
        assert_component::<crate::ui::layout_binding::LayoutBinding>();
        assert_component::<crate::ui::layout_binding::SharedLayoutBinding>();
        assert_component::<crate::ui::layout_binding::LayoutContainer>();
        assert_component::<crate::ui::offscreen::OffscreenRender>();
        assert_component::<crate::ui::offscreen::OffscreenGeneration>();
        assert_component::<crate::ui::offscreen::WidgetTextureRef>();
        assert_component::<crate::ui::offscreen::OffscreenAutoAdded>();
        assert_component::<crate::ui::offscreen::WidgetTextureRefPrevGen>();
        assert_component::<crate::ui::widgets::tabbar::TabIndicatorTween>();
        assert_component::<crate::ui::widgets::tabbar::TabBarPrev>();
        assert_component::<crate::ui::widgets::TabBar>();
        assert_component::<crate::ui::widgets::TabContent>();
        assert_component::<crate::ui::Widget>();
        assert_component::<crate::ui::Children>();
        assert_component::<crate::ui::Parent>();
        assert_component::<crate::ui::ComputedRect>();
        assert_component::<crate::ui::Style>();
        assert_component::<crate::ui::dirty::Dirty>();
        assert_component::<crate::ui::Hidden>();
        assert_component::<crate::ui::IgnoreHitTest>();
        assert_component::<crate::ui::InteractionFeedback>();
        assert_component::<crate::ui::widgets::switch::SwitchBgT>();
        assert_component::<crate::ui::widgets::switch::AnimatedThumbX>();
        assert_component::<crate::ui::widgets::switch::AnimateSwitchBgT>();
        assert_component::<crate::ui::widgets::switch::AnimateThumbX>();
        assert_component::<crate::ui::widgets::Switch>();
        assert_component::<crate::ui::widgets::LazyList>();
        assert_component::<crate::ui::widgets::LazyListPool>();
        assert_component::<crate::ui::widgets::LazyListBinder>();
        assert_component::<crate::input::event::scroll::ScrollOffset>();
        assert_component::<crate::input::event::scroll::ScrollDelta>();
        assert_component::<crate::input::event::scroll::ScrollConfig>();
        assert_component::<crate::ui::widgets::WidgetTransform>();
        assert_component::<crate::input::feedback::OverlayCursor>();
        assert_component::<crate::input::feedback::OverlayRotary>();
        assert_component::<crate::ui::widgets::TextInput>();
        assert_component::<crate::ui::widgets::Text>();
        assert_component::<GeneratedInventoryMotion>();
        let _ = GeneratedInventoryMotion::system();
    }

    #[test]
    fn whole_query_yields_flat_rows_and_mutates_in_place() {
        let mut world = World::new();
        let first = world.spawn_empty();
        world.insert(first, Position(1));
        world.insert(first, Velocity(2));
        let second = world.spawn_empty();
        world.insert(second, Position(10));
        world.insert(second, Velocity(20));

        type Rows<'w> = SystemQuery<
            'w,
            ComponentDriver<Position>,
            (EntityItem, Read<Position>, Write<Velocity>),
            (),
        >;
        let rows = Rows::new(&mut world);
        let mut seen = 0;
        for (entity, position, velocity) in rows {
            assert!(entity == first || entity == second);
            velocity.0 += position.0;
            seen += 1;
        }

        assert_eq!(seen, 2);
        assert_eq!(world.get::<Velocity>(first), Some(&Velocity(3)));
        assert_eq!(world.get::<Velocity>(second), Some(&Velocity(30)));
    }

    #[test]
    fn get_uses_driver_required_components_filters_and_generation() {
        let mut world = World::new();
        let old = world.spawn_empty();
        world.insert(old, Position(0));
        assert!(world.despawn(old));
        let selected = world.spawn_empty();
        world.insert(selected, Position(1));
        world.insert(selected, Velocity(2));
        world.insert(selected, Active);
        let missing_driver = world.spawn_empty();
        world.insert(missing_driver, Velocity(3));
        world.insert(missing_driver, Active);
        let missing_required = world.spawn_empty();
        world.insert(missing_required, Position(4));
        world.insert(missing_required, Active);
        let inactive = world.spawn_empty();
        world.insert(inactive, Position(5));
        world.insert(inactive, Velocity(6));
        let hidden = world.spawn_empty();
        world.insert(hidden, Position(7));
        world.insert(hidden, Velocity(8));
        world.insert(hidden, Active);
        world.insert(hidden, Hidden);

        type Rows<'w> = SystemQuery<
            'w,
            ComponentDriver<Position>,
            (EntityItem, Read<Velocity>, OptionalRead<Label>),
            (With<Active>, Without<Hidden>),
        >;
        let rows = Rows::new(&mut world);
        let (entity, velocity, label) = rows.get(selected).expect("matching row");
        let (_, same_velocity, _) = rows.get(selected).expect("shared read");
        assert_eq!(entity, selected);
        assert_eq!(velocity.0, 2);
        assert!(core::ptr::eq(velocity, same_velocity));
        assert!(label.is_none());
        for excluded in [old, missing_driver, missing_required, inactive, hidden] {
            assert!(rows.get(excluded).is_none());
        }
    }

    #[test]
    fn mutable_get_reborrows_and_coalesces_changes_without_allocating() {
        let mut world = World::new();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        for entity in [first, second] {
            world.insert(entity, Position(1));
            world.insert(entity, Velocity(2));
        }
        world.watch_component_type(TypeId::of::<Velocity>());
        world.drain_component_changes(|_, _| {});

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (Write<Velocity>, Read<Position>), ()>;
        drop(Rows::new(&mut world));
        let allocations = tracked_allocations(|| {
            let mut rows = Rows::new(&mut world);
            for _ in 0..64 {
                let (velocity, position) = rows.get(first).expect("first row");
                velocity.0 += position.0;
            }
            for (velocity, position) in rows {
                velocity.0 += position.0;
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(world.get::<Velocity>(first), Some(&Velocity(67)));
        assert_eq!(world.get::<Velocity>(second), Some(&Velocity(3)));
        let mut changes = alloc::vec::Vec::new();
        world.drain_component_changes(|_, change| changes.push(change.entity));
        assert_eq!(changes, [first, second]);
    }

    #[test]
    fn mutable_rows_record_only_entities_that_are_yielded() {
        let mut world = World::new();
        let active = world.spawn_empty();
        world.insert(active, Position(1));
        world.insert(active, Velocity(2));
        world.insert(active, Active);
        let inactive = world.spawn_empty();
        world.insert(inactive, Position(3));
        world.insert(inactive, Velocity(4));
        world.watch_component_type(TypeId::of::<Velocity>());
        world.drain_component_changes(|_, _| {});

        type Rows<'w> = SystemQuery<
            'w,
            ComponentDriver<Position>,
            (EntityItem, Write<Velocity>),
            (With<Active>,),
        >;
        for (_, velocity) in Rows::new(&mut world) {
            velocity.0 += 1;
        }

        let mut changes = alloc::vec::Vec::new();
        world.drain_component_changes(|_, change| changes.push(change));
        assert_eq!(
            changes,
            [crate::ecs::world::ComponentChange {
                entity: active,
                type_id: TypeId::of::<Velocity>(),
            }]
        );
        assert_eq!(world.get::<Velocity>(active), Some(&Velocity(3)));
        assert_eq!(world.get::<Velocity>(inactive), Some(&Velocity(4)));
    }

    #[test]
    fn mutable_query_records_only_rows_requested_from_the_iterator() {
        let mut world = World::new();
        let first = world.spawn_empty();
        world.insert(first, Velocity(1));
        let second = world.spawn_empty();
        world.insert(second, Velocity(2));
        world.watch_component_type(TypeId::of::<Velocity>());
        world.drain_component_changes(|_, _| {});

        type Rows<'world> =
            SystemQuery<'world, ComponentDriver<Velocity>, (EntityItem, Write<Velocity>), ()>;

        drop(Rows::new(&mut world));
        let mut changes = alloc::vec::Vec::new();
        world.drain_component_changes(|_, change| changes.push(change));
        assert!(changes.is_empty());

        let mut rows = Rows::new(&mut world).into_iter();
        let (yielded, _) = rows.next().expect("first mutable row");
        drop(rows);
        world.drain_component_changes(|_, change| changes.push(change));

        assert_eq!(yielded, first);
        assert_eq!(
            changes,
            [crate::ecs::world::ComponentChange {
                entity: first,
                type_id: TypeId::of::<Velocity>(),
            }]
        );
        assert_ne!(yielded, second);
    }

    #[test]
    fn mutable_query_reuses_change_queue_after_warmup() {
        let mut world = World::new();
        world.watch_component_type(TypeId::of::<Velocity>());
        for value in 0..32 {
            let entity = world.spawn_empty();
            world.insert(entity, Position(value));
            world.insert(entity, Velocity(value));
            world.drain_component_changes(|_, _| {});
        }
        type Rows<'w> = SystemQuery<'w, ComponentDriver<Position>, Write<Velocity>, ()>;

        for velocity in Rows::new(&mut world) {
            velocity.0 += 1;
        }
        world.drain_component_changes(|_, _| {});

        let allocations = tracked_allocations(|| {
            for velocity in Rows::new(&mut world) {
                velocity.0 += 1;
            }
            world.drain_component_changes(|_, _| {});
        });

        assert_eq!(allocations, 0);
    }

    #[test]
    fn with_and_without_filters_select_rows() {
        let mut world = World::new();
        let visible = world.spawn_empty();
        world.insert(visible, Position(1));
        world.insert(visible, Active);
        let hidden = world.spawn_empty();
        world.insert(hidden, Position(2));
        world.insert(hidden, Active);
        world.insert(hidden, Hidden);
        let inactive = world.spawn_empty();
        world.insert(inactive, Position(3));

        type Rows<'w> = SystemQuery<
            'w,
            ComponentDriver<Position>,
            (EntityItem, Read<Position>),
            (With<Active>, Without<Hidden>),
        >;
        let mut rows = Rows::new(&mut world).into_iter();
        let (entity, position) = rows.next().expect("visible row");
        assert_eq!((entity, position.0), (visible, 1));
        assert!(rows.next().is_none());
    }

    #[test]
    fn optional_read_returns_some_and_none() {
        let mut world = World::new();
        let labeled = world.spawn_empty();
        world.insert(labeled, Position(1));
        world.insert(labeled, Label("one"));
        let unlabeled = world.spawn_empty();
        world.insert(unlabeled, Position(2));

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (EntityItem, OptionalRead<Label>), ()>;
        let mut labeled_value = None;
        let mut saw_unlabeled = false;
        for (entity, label) in Rows::new(&mut world) {
            if entity == labeled {
                labeled_value = label.map(|label| label.0);
            } else if entity == unlabeled {
                saw_unlabeled = label.is_none();
            }
        }

        assert_eq!(labeled_value, Some("one"));
        assert!(saw_unlabeled);
    }

    #[test]
    fn missing_required_component_skips_row() {
        let mut world = World::new();
        let complete = world.spawn_empty();
        world.insert(complete, Position(1));
        world.insert(complete, Velocity(2));
        let incomplete = world.spawn_empty();
        world.insert(incomplete, Position(3));

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Read<Velocity>), ()>;
        let mut rows = Rows::new(&mut world).into_iter();
        assert_eq!(rows.next().map(|(entity, _)| entity), Some(complete));
        assert!(rows.next().is_none());
    }

    #[test]
    fn missing_driver_yields_empty_query() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Velocity(2));

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (EntityItem, OptionalRead<Velocity>), ()>;
        assert_eq!(Rows::new(&mut world).into_iter().count(), 0);
    }

    #[test]
    fn with_filter_can_drive_entity_only_query() {
        let mut world = World::new();
        let active = world.spawn_empty();
        world.insert(active, Active);
        let inactive = world.spawn_empty();
        world.insert(inactive, Position(2));

        type Rows<'w> = SystemQuery<'w, ComponentDriver<Active>, EntityItem, (With<Active>,)>;
        let mut rows = Rows::new(&mut world).into_iter();
        assert_eq!(rows.next(), Some(active));
        assert!(rows.next().is_none());
    }

    #[test]
    fn entity_generation_is_preserved() {
        let mut world = World::new();
        let old = world.spawn_empty();
        world.insert(old, Position(1));
        assert!(world.despawn(old));
        let current = world.spawn_empty();
        world.insert(current, Position(2));
        assert_eq!(old.id, current.id);
        assert_ne!(old.generation, current.generation);

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (EntityItem, Read<Position>), ()>;
        let mut rows = Rows::new(&mut world).into_iter();
        assert_eq!(rows.next().map(|(entity, _)| entity), Some(current));
        assert!(rows.next().is_none());
    }

    #[test]
    fn duplicate_read_is_allowed() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Position(4));

        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Read<Position>), ()>;
        let mut rows = Rows::new(&mut world).into_iter();
        let (left, right) = rows.next().expect("row");
        assert_eq!(left, &Position(4));
        assert!(core::ptr::eq(left, right));
    }

    #[test]
    #[should_panic(expected = "conflicting query access")]
    fn read_write_alias_is_rejected_before_fetch() {
        let mut world = World::new();
        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Write<Position>), ()>;
        let _ = Rows::new(&mut world);
    }

    #[test]
    #[should_panic(expected = "conflicting query access")]
    fn write_write_alias_is_rejected_before_fetch() {
        let mut world = World::new();
        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (Write<Position>, Write<Position>), ()>;
        let _ = Rows::new(&mut world);
    }

    #[test]
    #[should_panic(expected = "conflicting query access")]
    fn type_alias_conflict_is_rejected() {
        type PositionAlias = Position;
        let mut world = World::new();
        type Rows<'w> =
            SystemQuery<'w, ComponentDriver<Position>, (Read<Position>, Write<PositionAlias>), ()>;
        let _ = Rows::new(&mut world);
    }

    #[test]
    fn writable_zero_sized_components_have_one_row_per_entity() {
        let mut world = World::new();
        let first = world.spawn_empty();
        world.insert(first, Marker);
        let second = world.spawn_empty();
        world.insert(second, Marker);

        type Rows<'w> = SystemQuery<'w, ComponentDriver<Marker>, (EntityItem, Write<Marker>), ()>;
        let mut rows = Rows::new(&mut world).into_iter();
        assert_eq!(rows.next().map(|(entity, _)| entity), Some(first));
        assert_eq!(rows.next().map(|(entity, _)| entity), Some(second));
        assert!(rows.next().is_none());
    }

    #[test]
    fn mutable_driver_rows_keep_distinct_slots_exclusive() {
        let mut world = World::new();
        let first = world.spawn_empty();
        world.insert(first, Velocity(1));
        let second = world.spawn_empty();
        world.insert(second, Velocity(2));

        type Rows<'world> =
            SystemQuery<'world, ComponentDriver<Velocity>, (EntityItem, Write<Velocity>), ()>;
        let mut rows = Rows::new(&mut world).into_iter();
        let (first_entity, first_value) = rows.next().expect("first row");
        let (second_entity, second_value) = rows.next().expect("second row");
        first_value.0 += 10;
        second_value.0 += 20;
        assert!(rows.next().is_none());
        drop(rows);

        assert_eq!((first_entity, first_value.0), (first, 11));
        assert_eq!((second_entity, second_value.0), (second, 22));
    }

    #[test]
    fn warmed_query_construction_and_iteration_allocate_nothing() {
        let mut world = World::new();
        for value in 1..=8 {
            let entity = world.spawn_empty();
            world.insert(entity, Position(value));
            world.insert(entity, Velocity(value * 10));
            world.insert(entity, Active);
            if value % 2 == 0 {
                world.insert(entity, Label("label"));
            }
            if value == 8 {
                world.insert(entity, Hidden);
            }
        }

        type Rows<'world> = SystemQuery<
            'world,
            ComponentDriver<Position>,
            (
                EntityItem,
                Read<Position>,
                Write<Velocity>,
                OptionalRead<Label>,
            ),
            (With<Active>, Without<Hidden>),
        >;

        let mut visited = 0;
        let allocations = tracked_allocations(|| {
            for (_entity, position, velocity, label) in Rows::new(&mut world) {
                velocity.0 += position.0;
                visited += 1 + usize::from(label.is_some());
            }
        });

        assert_eq!(allocations, 0);
        assert_eq!(visited, 10);
    }
}
