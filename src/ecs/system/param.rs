use core::any::type_name;
use core::marker::PhantomData;

use crate::ecs::query::system::{QueryData, QueryDriver, QueryFilter, SystemQuery};
use crate::ecs::world::{SystemWorldCell, World};

mod sealed {
    pub trait SystemParam {}
    pub trait SystemParamSet {}
}

pub struct Res<T>(PhantomData<fn() -> T>);
pub struct ResMut<T>(PhantomData<fn() -> T>);
pub struct OptionalRes<T>(PhantomData<fn() -> T>);
pub struct QueryParam<D, Q, F>(
    PhantomData<fn() -> D>,
    PhantomData<fn() -> Q>,
    PhantomData<fn() -> F>,
);

/// One generated system parameter.
///
/// # Safety
///
/// Implementations must only borrow data declared by the parameter's static
/// access metadata. Returned values may not expose structural World mutation.
pub unsafe trait SystemParam: sealed::SystemParam {
    type Item<'world>;

    /// Return the conservative component-change budget needed before this
    /// parameter is fetched. Most parameter kinds produce no changes.
    ///
    /// # Safety
    ///
    /// The complete system metadata must have been validated and `cell` must
    /// own the running system's exclusive World window.
    unsafe fn pending_changes<'world>(_cell: &SystemWorldCell<'world>) -> usize {
        0
    }

    /// # Safety
    ///
    /// The complete system metadata must have been validated before any
    /// parameter is fetched, and it must describe every access performed here.
    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        system_name: &'static str,
    ) -> Self::Item<'world>;
}

impl<T> sealed::SystemParam for Res<T> {}

unsafe impl<T: 'static> SystemParam for Res<T> {
    type Item<'world> = &'world T;

    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        system_name: &'static str,
    ) -> Self::Item<'world> {
        // SAFETY: delegated from SystemParam::fetch.
        unsafe { cell.resource::<T>() }.unwrap_or_else(|| {
            panic!(
                "callback `{system_name}` requires resource `{}`",
                type_name::<T>()
            )
        })
    }
}

impl<T> sealed::SystemParam for ResMut<T> {}

unsafe impl<T: 'static> SystemParam for ResMut<T> {
    type Item<'world> = &'world mut T;

    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        system_name: &'static str,
    ) -> Self::Item<'world> {
        // SAFETY: delegated from SystemParam::fetch.
        unsafe { cell.resource_mut::<T>() }.unwrap_or_else(|| {
            panic!(
                "callback `{system_name}` requires resource `{}`",
                type_name::<T>()
            )
        })
    }
}

impl<T> sealed::SystemParam for OptionalRes<T> {}

unsafe impl<T: 'static> SystemParam for OptionalRes<T> {
    type Item<'world> = Option<&'world T>;

    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        _system_name: &'static str,
    ) -> Self::Item<'world> {
        // SAFETY: delegated from SystemParam::fetch.
        unsafe { cell.resource::<T>() }
    }
}

impl<D, Q, F> sealed::SystemParam for QueryParam<D, Q, F> {}

unsafe impl<D, Q, F> SystemParam for QueryParam<D, Q, F>
where
    D: QueryDriver,
    Q: QueryData,
    F: QueryFilter,
{
    type Item<'world> = SystemQuery<'world, D, Q, F>;

    unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>) -> usize {
        // SAFETY: the aggregate access plan was validated before preflight.
        let driver = unsafe { D::init(cell) };
        // SAFETY: the same validated World window brands both states.
        unsafe { Q::pending_changes(cell, D::len(&driver)) }
    }

    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        _system_name: &'static str,
    ) -> Self::Item<'world> {
        // SAFETY: delegated from SystemParam::fetch for the complete parameter
        // set, not merely this individual query.
        unsafe { SystemQuery::new_unchecked(cell) }
    }
}

/// Complete generated parameter set for one system callback.
///
/// # Safety
///
/// Implementations must fetch every parameter exactly once from the same
/// `SystemWorldCell` and preserve source order.
pub unsafe trait SystemParamSet: sealed::SystemParamSet {
    type Item<'world>;

    /// # Safety
    ///
    /// The System descriptor must have validated the complete access metadata.
    unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>) -> usize;

    /// # Safety
    ///
    /// The System descriptor must have validated the complete access metadata
    /// before this method is called.
    unsafe fn fetch<'world>(
        cell: &SystemWorldCell<'world>,
        system_name: &'static str,
    ) -> Self::Item<'world>;
}

impl sealed::SystemParamSet for () {}

unsafe impl SystemParamSet for () {
    type Item<'world> = ();

    unsafe fn pending_changes<'world>(_cell: &SystemWorldCell<'world>) -> usize {
        0
    }

    unsafe fn fetch<'world>(
        _cell: &SystemWorldCell<'world>,
        _system_name: &'static str,
    ) -> Self::Item<'world> {
    }
}

macro_rules! impl_system_param_set {
    ($(($param:ident, $index:tt)),+ $(,)?) => {
        impl<$($param: SystemParam),+> sealed::SystemParamSet for ($($param,)+) {}

        unsafe impl<$($param: SystemParam),+> SystemParamSet for ($($param,)+) {
            type Item<'world> = ($($param::Item<'world>,)+);

            unsafe fn pending_changes<'world>(cell: &SystemWorldCell<'world>) -> usize {
                let mut pending = 0usize;
                $(
                    pending = pending.checked_add(unsafe {
                        $param::pending_changes(cell)
                    }).expect("component change budget overflow");
                )+
                pending
            }

            unsafe fn fetch<'world>(
                cell: &SystemWorldCell<'world>,
                system_name: &'static str,
            ) -> Self::Item<'world> {
                // SAFETY: the caller validated the aggregate metadata before
                // any tuple field is fetched. Tuple fields preserve source
                // order and each parameter is initialized exactly once.
                ($(unsafe { $param::fetch(cell, system_name) },)+)
            }
        }
    };
}

impl_system_param_set!((A, 0));
impl_system_param_set!((A, 0), (B, 1));
impl_system_param_set!((A, 0), (B, 1), (C, 2));
impl_system_param_set!((A, 0), (B, 1), (C, 2), (D, 3));
impl_system_param_set!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4));
impl_system_param_set!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5));
impl_system_param_set!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5), (G, 6));
impl_system_param_set!(
    (A, 0),
    (B, 1),
    (C, 2),
    (D, 3),
    (E, 4),
    (F, 5),
    (G, 6),
    (H, 7)
);

/// Build every generated parameter from one exclusive World window.
///
/// # Safety
///
/// `P` must exactly match metadata already validated for the running System.
pub unsafe fn fetch_system_params<'world, P: SystemParamSet>(
    world: &'world mut World,
    system_name: &'static str,
) -> P::Item<'world> {
    let cell = SystemWorldCell::new(world);
    // SAFETY: delegated from this function's caller. No parameter has been
    // fetched from this exclusive window yet.
    unsafe { reserve_param_budget::<P>(&cell) };
    // SAFETY: delegated from this function's caller.
    unsafe { P::fetch(&cell, system_name) }
}

/// Reserve the aggregate component-change budget without borrowing parameters.
///
/// # Safety
///
/// `P` must match a validated, immutable callback access plan.
pub(crate) unsafe fn reserve_system_params<P: SystemParamSet>(world: &mut World) {
    let cell = SystemWorldCell::new(world);
    // SAFETY: the caller validated the plan; no parameters have been fetched.
    unsafe { reserve_param_budget::<P>(&cell) };
}

unsafe fn reserve_param_budget<P: SystemParamSet>(cell: &SystemWorldCell<'_>) {
    // SAFETY: the caller validated the plan and has not fetched parameters.
    let pending = unsafe { P::pending_changes(cell) };
    // SAFETY: no parameter has been fetched from this exclusive window.
    unsafe { cell.reserve_component_change_records(pending) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::any::TypeId;

    use crate::ecs::Component;
    use crate::ecs::query::system::{ComponentDriver, EntityItem, OptionalRead, Read, Write};
    use crate::test_support::tracked_allocations;

    #[derive(Debug, PartialEq, Eq)]
    struct Position(i32);

    impl Component for Position {}

    #[derive(Debug, PartialEq, Eq)]
    struct Velocity(i32);

    impl Component for Velocity {}

    struct Clock(u32);
    struct Settings(u32);
    struct Missing;

    #[test]
    fn aggregate_fetch_combines_resources_and_multiple_queries() {
        let mut world = World::new();
        world.insert_resource(Clock(4));
        world.insert_resource(Settings(2));
        let first = world.spawn_empty();
        world.insert(first, Position(3));
        world.insert(first, Velocity(5));
        let second = world.spawn_empty();
        world.insert(second, Position(7));

        type Positions = QueryParam<
            ComponentDriver<Position>,
            (EntityItem, Read<Position>, OptionalRead<Velocity>),
            (),
        >;
        type Velocities = QueryParam<ComponentDriver<Velocity>, Write<Velocity>, ()>;
        type Params = (
            Res<Clock>,
            ResMut<Settings>,
            OptionalRes<Missing>,
            Positions,
            Velocities,
        );

        // SAFETY: this test's parameter set has no conflicting accesses and no
        // structural World operations occur while the returned values live.
        let (clock, settings, missing, positions, velocities) =
            unsafe { fetch_system_params::<Params>(&mut world, "aggregate") };
        settings.0 += clock.0;
        assert!(missing.is_none());
        let rows = positions
            .into_iter()
            .map(|(entity, position, velocity)| (entity, position.0, velocity.map(|v| v.0)))
            .collect::<alloc::vec::Vec<_>>();
        for velocity in velocities {
            velocity.0 += 1;
        }

        assert_eq!(settings.0, 6);
        assert_eq!(rows, [(first, 3, Some(5)), (second, 7, None)]);
        assert_eq!(world.get::<Velocity>(first), Some(&Velocity(6)));
    }

    #[test]
    #[should_panic(expected = "callback `required` requires resource")]
    fn required_resource_reports_callback_and_type() {
        let mut world = World::new();
        // SAFETY: the sole read is conflict-free; the test verifies the
        // required-resource failure before any reference is returned.
        let _ = unsafe { fetch_system_params::<(Res<Missing>,)>(&mut world, "required") };
    }

    #[test]
    fn aggregate_fetch_and_iteration_do_not_allocate() {
        let mut world = World::new();
        world.insert_resource(Clock(4));
        let entity = world.spawn_empty();
        world.insert(entity, Position(3));
        type Positions = QueryParam<ComponentDriver<Position>, (EntityItem, Read<Position>), ()>;
        type Params = (Res<Clock>, Positions);

        let allocations = tracked_allocations(|| {
            // SAFETY: both accesses are read-only and the body performs no
            // structural World operation.
            let (clock, positions) =
                unsafe { fetch_system_params::<Params>(&mut world, "allocation") };
            assert_eq!(clock.0, 4);
            assert_eq!(positions.into_iter().count(), 1);
        });

        assert_eq!(allocations, 0);
    }

    #[test]
    fn aggregate_preflight_reserves_all_mutable_query_changes_before_iteration() {
        const ROWS: usize = 65;

        let mut world = World::new();
        for value in 0..ROWS {
            let entity = world.spawn_empty();
            world.insert(entity, Position(value as i32));
            world.insert(entity, Velocity(value as i32));
        }

        // Establish an individual high-water mark for each watched type. The
        // combined callback still needs room for both write streams at once.
        world.watch_component_type(TypeId::of::<Position>());
        world.drain_component_changes(|_, _| {});
        world.watch_component_type(TypeId::of::<Velocity>());
        world.drain_component_changes(|_, _| {});

        type Positions = QueryParam<ComponentDriver<Position>, Write<Position>, ()>;
        type Velocities = QueryParam<ComponentDriver<Velocity>, Write<Velocity>, ()>;
        type Params = (Positions, Velocities);

        // SAFETY: the two mutable queries access distinct component types and
        // no structural World operation occurs while either query is alive.
        let (positions, velocities) =
            unsafe { fetch_system_params::<Params>(&mut world, "change-budget") };
        let allocations = tracked_allocations(|| {
            for position in positions {
                position.0 += 1;
            }
            for velocity in velocities {
                velocity.0 += 1;
            }
        });

        assert_eq!(allocations, 0);
    }
}
