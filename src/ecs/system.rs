use alloc::rc::Rc;
use alloc::vec::Vec;

use super::Component;
use super::World;
use super::commands::{CommandError, CommandQueue, DeferredCommands};
use super::world::WorldId;

pub(crate) mod param;

pub(crate) mod access {
    use core::any::{TypeId, type_name};

    use super::Component;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum SystemAccessKind {
        ExpectComponent,
        ReadComponent,
        WriteComponent,
        ReadResource,
        WriteResource,
    }

    #[derive(Clone, Copy)]
    pub(super) struct AccessType {
        pub(super) id: TypeId,
        pub(super) name: &'static str,
    }

    fn resolve_access_type<T: 'static>() -> AccessType {
        AccessType {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }

    /// Static component/resource access descriptor emitted by system macros.
    #[derive(Clone, Copy)]
    pub struct SystemAccess {
        pub(super) resolve: fn() -> AccessType,
        pub(super) kind: SystemAccessKind,
    }

    impl SystemAccess {
        pub const fn expect_component<T: Component>() -> Self {
            Self {
                resolve: resolve_access_type::<T>,
                kind: SystemAccessKind::ExpectComponent,
            }
        }

        pub const fn read_component<T: Component>() -> Self {
            Self {
                resolve: resolve_access_type::<T>,
                kind: SystemAccessKind::ReadComponent,
            }
        }

        pub const fn write_component<T: Component>() -> Self {
            Self {
                resolve: resolve_access_type::<T>,
                kind: SystemAccessKind::WriteComponent,
            }
        }

        pub const fn read_resource<T: 'static>() -> Self {
            Self {
                resolve: resolve_access_type::<T>,
                kind: SystemAccessKind::ReadResource,
            }
        }

        pub const fn write_resource<T: 'static>() -> Self {
            Self {
                resolve: resolve_access_type::<T>,
                kind: SystemAccessKind::WriteResource,
            }
        }

        pub(super) fn access(self) -> Option<(AccessDomain, AccessMode)> {
            match self.kind {
                SystemAccessKind::ExpectComponent => None,
                SystemAccessKind::ReadComponent => {
                    Some((AccessDomain::Component, AccessMode::Read))
                }
                SystemAccessKind::WriteComponent => {
                    Some((AccessDomain::Component, AccessMode::Write))
                }
                SystemAccessKind::ReadResource => Some((AccessDomain::Resource, AccessMode::Read)),
                SystemAccessKind::WriteResource => {
                    Some((AccessDomain::Resource, AccessMode::Write))
                }
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum AccessDomain {
        Component,
        Resource,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum AccessMode {
        Read,
        Write,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum AccessError {
        ExpectAfterAccess,
        Conflict { type_name: &'static str },
    }

    pub(crate) fn check(metadata: &[SystemAccess]) -> Result<(), AccessError> {
        let mut access_started = false;
        for descriptor in metadata {
            if descriptor.kind == SystemAccessKind::ExpectComponent {
                if access_started {
                    return Err(AccessError::ExpectAfterAccess);
                }
            } else {
                access_started = true;
            }
        }

        for (left_index, left) in metadata.iter().copied().enumerate() {
            let Some((left_domain, left_mode)) = left.access() else {
                continue;
            };
            let left_type = (left.resolve)();
            for right in metadata[left_index + 1..].iter().copied() {
                let Some((right_domain, right_mode)) = right.access() else {
                    continue;
                };
                let right_type = (right.resolve)();
                if left_domain == right_domain
                    && left_type.id == right_type.id
                    && (left_mode != AccessMode::Read || right_mode != AccessMode::Read)
                {
                    return Err(AccessError::Conflict {
                        type_name: left_type.name,
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate(kind: &str, name: &str, metadata: &[SystemAccess]) {
        match check(metadata) {
            Ok(()) => {}
            Err(AccessError::ExpectAfterAccess) => {
                panic!("{kind} `{name}` has expect metadata after access metadata")
            }
            Err(AccessError::Conflict { type_name }) => {
                panic!("{kind} `{name}` has conflicting access to `{type_name}`")
            }
        }
    }
}

use access::{SystemAccess, SystemAccessKind};

type BoundCallback = dyn Fn(&mut World) -> Result<(), CommandError>;
type BoundDeferredCallback =
    dyn for<'commands> Fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>;

/// A static system function or a callback captured at registration.
enum SystemCallback {
    Static(fn(&mut World) -> Result<(), CommandError>),
    Bound(Rc<BoundCallback>),
    Deferred(
        for<'commands> fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>,
    ),
    BoundDeferred(Rc<BoundDeferredCallback>),
}

impl Clone for SystemCallback {
    fn clone(&self) -> Self {
        match self {
            Self::Static(run) => Self::Static(*run),
            Self::Bound(run) => Self::Bound(Rc::clone(run)),
            Self::Deferred(run) => Self::Deferred(*run),
            Self::BoundDeferred(run) => Self::BoundDeferred(Rc::clone(run)),
        }
    }
}

impl SystemCallback {
    fn uses_commands(&self) -> bool {
        matches!(self, Self::Deferred(_) | Self::BoundDeferred(_))
    }

    fn invoke_plain(&self, world: &mut World) -> Result<(), CommandError> {
        match self {
            Self::Static(run) => run(world),
            Self::Bound(run) => run(world),
            Self::Deferred(_) | Self::BoundDeferred(_) => {
                unreachable!("deferred callback requires a command batch")
            }
        }
    }

    fn invoke_deferred(
        &self,
        world: &mut World,
        commands: DeferredCommands<'_>,
    ) -> Result<(), CommandError> {
        match self {
            Self::Deferred(run) => run(world, commands),
            Self::BoundDeferred(run) => run(world, commands),
            Self::Static(_) | Self::Bound(_) => {
                unreachable!("plain callback does not accept deferred commands")
            }
        }
    }
}

/// Lower `priority` runs first; [`SystemSlot`] defines the named slots used by
/// `#[system(order = Animation)]`.
///
/// `last_us` / `total_us` / `call_count` are populated by
/// `SystemScheduler::run_all` when a `MonoClock` resource is present.
/// Without a clock all three stay zero and the scheduler doesn't pay
/// the timing overhead.
pub struct System {
    pub name: &'static str,
    priority: i32,
    callback: SystemCallback,
    owner: Option<WorldId>,
    metadata: &'static [SystemAccess],
    pub last_us: u32,
    pub total_us: u64,
    pub call_count: u32,
}

impl System {
    /// Numeric scheduling priority. Lower values run first.
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    pub(crate) const fn new(
        name: &'static str,
        priority: i32,
        run: fn(&mut World) -> Result<(), CommandError>,
    ) -> Self {
        Self {
            name,
            priority,
            callback: SystemCallback::Static(run),
            owner: None,
            metadata: &[],
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }

    #[doc(hidden)]
    pub(crate) const fn deferred(
        name: &'static str,
        priority: i32,
        run: for<'commands> fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>,
    ) -> Self {
        Self {
            name,
            priority,
            callback: SystemCallback::Deferred(run),
            owner: None,
            metadata: &[],
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }

    /// Capture a registered model instance for a system callback.
    /// The callback is allocated once at registration, not on each tick.
    pub(crate) fn bound<H: crate::core::model::ModelHandle>(
        name: &'static str,
        priority: i32,
        model: &H,
        run: impl Fn(&mut World) -> Result<(), CommandError> + 'static,
    ) -> Self {
        let owner = crate::core::model::registered_owner(model);
        Self {
            name,
            priority,
            callback: SystemCallback::Bound(Rc::new(run)),
            owner: Some(owner),
            metadata: &[],
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }

    /// Capture registered model instances for a generated system that also
    /// records deferred structural changes.
    #[doc(hidden)]
    pub(crate) fn bound_deferred<H: crate::core::model::ModelHandle>(
        name: &'static str,
        priority: i32,
        model: &H,
        run: impl for<'commands> Fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>
        + 'static,
    ) -> Self {
        let owner = crate::core::model::registered_owner(model);
        Self {
            name,
            priority,
            callback: SystemCallback::BoundDeferred(Rc::new(run)),
            owner: Some(owner),
            metadata: &[],
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }

    /// Require another bound model to belong to the same application.
    pub(crate) fn with_model<H: crate::core::model::ModelHandle>(self, model: &H) -> Self {
        assert_eq!(
            self.owner,
            Some(crate::core::model::registered_owner(model)),
            "bound models belong to different Apps"
        );
        self
    }

    pub(crate) fn assert_owner(&self, world: &World) {
        if let Some(owner) = self.owner {
            assert_eq!(owner, world.id(), "bound system belongs to a different App");
        }
    }

    #[doc(hidden)]
    /// # Safety
    ///
    /// `metadata` must describe every resource and component access performed
    /// by the callback. Generated system code supplies this invariant.
    pub(crate) const unsafe fn with_metadata(mut self, metadata: &'static [SystemAccess]) -> Self {
        self.metadata = metadata;
        self
    }

    #[cfg(test)]
    pub(crate) fn run_once(&self, world: &mut World) -> Result<(), CommandError> {
        self.assert_owner(world);
        self.validate_metadata();
        if self.should_run(world) {
            if self.callback.uses_commands() {
                let mut queue = CommandQueue::default();
                let mut batch = queue.begin();
                self.callback.invoke_deferred(world, batch.commands())?;
                batch.finish(world)?;
            } else {
                self.callback.invoke_plain(world)?;
            }
        }
        Ok(())
    }

    fn validate_metadata(&self) {
        access::validate("system", self.name, self.metadata);
    }

    fn should_run(&self, world: &World) -> bool {
        let mut has_expect = false;
        for descriptor in self.metadata {
            if descriptor.kind != SystemAccessKind::ExpectComponent {
                break;
            }
            has_expect = true;
            if world.has_any_by_id((descriptor.resolve)().id) {
                return true;
            }
        }
        !has_expect
    }

    #[cfg(test)]
    pub(crate) fn metadata(&self) -> &'static [SystemAccess] {
        self.metadata
    }
}

impl From<&System> for System {
    /// Copy a system template with fresh execution statistics.
    fn from(template: &System) -> Self {
        Self {
            name: template.name,
            priority: template.priority,
            callback: template.callback.clone(),
            owner: template.owner,
            metadata: template.metadata,
            last_us: 0,
            total_us: 0,
            call_count: 0,
        }
    }
}

/// Constructor seam used by generated authoring macros.
#[doc(hidden)]
pub(crate) mod generated {
    use super::{CommandError, DeferredCommands, System, SystemAccess, World};

    /// # Safety
    ///
    /// `run` may access the whole [`World`]. The caller must ensure that any
    /// attached metadata fully describes every alias-relevant access performed
    /// by the callback.
    pub const unsafe fn static_system(
        name: &'static str,
        priority: i32,
        run: fn(&mut World) -> Result<(), CommandError>,
    ) -> System {
        System::new(name, priority, run)
    }

    /// # Safety
    ///
    /// `run` may access the whole [`World`]. The caller must ensure that any
    /// attached metadata fully describes every alias-relevant access performed
    /// by the callback.
    pub const unsafe fn deferred_system(
        name: &'static str,
        priority: i32,
        run: for<'commands> fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>,
    ) -> System {
        System::deferred(name, priority, run)
    }

    /// # Safety
    ///
    /// `run` may access the whole [`World`]. The caller must ensure that any
    /// attached metadata fully describes every alias-relevant access performed
    /// by the callback and that captured model bindings match that plan.
    pub unsafe fn bound_system<H: crate::core::model::ModelHandle>(
        name: &'static str,
        priority: i32,
        model: &H,
        run: impl Fn(&mut World) -> Result<(), CommandError> + 'static,
    ) -> System {
        System::bound(name, priority, model, run)
    }

    /// # Safety
    ///
    /// `run` may access the whole [`World`]. The caller must ensure that any
    /// attached metadata fully describes every alias-relevant access performed
    /// by the callback and that captured model bindings match that plan.
    pub unsafe fn bound_deferred_system<H: crate::core::model::ModelHandle>(
        name: &'static str,
        priority: i32,
        model: &H,
        run: impl for<'commands> Fn(&mut World, DeferredCommands<'commands>) -> Result<(), CommandError>
        + 'static,
    ) -> System {
        System::bound_deferred(name, priority, model, run)
    }

    pub fn require_same_owner<H: crate::core::model::ModelHandle>(
        system: System,
        model: &H,
    ) -> System {
        system.with_model(model)
    }

    /// # Safety
    ///
    /// `metadata` must describe every resource and component access performed
    /// by the generated callback.
    pub const unsafe fn with_metadata(system: System, metadata: &'static [SystemAccess]) -> System {
        // SAFETY: delegated from this generated-code seam.
        unsafe { system.with_metadata(metadata) }
    }
}

/// Named scheduling slots. Lower priorities run first.
///
/// `#[system]` accepts the bare variant name, for example
/// `#[system(order = Animation)]`. Omitting `order` selects `Normal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemSlot {
    /// Synthetic input emitters (sim, replay).
    SimInput,
    /// Clock / `DeltaTimeMs` sync. Anything reading `dt` runs after.
    DeltaTime,
    /// hover_system / press_system — read PointerCursor + hit_test, write `InteractionState`.
    InteractionState,
    /// Animation tickers that consume `dt`.
    Animation,
    /// Declarative timers — same band as animation.
    Timer,
    /// Inertia spring tick. Runs after pointer events, before `ScrollOffset` observers.
    ScrollInertia,
    /// Re-bind/reposition pool slots from current `ScrollOffset`.
    LazyList,
    /// `TabBar.selected` → page visibility.
    TabPages,
    /// Default for user systems — observes fully-settled state.
    Normal,
    /// Final reconciliation just before render (e.g. consumer dirty
    /// propagation that must see all user-system writes).
    PreRender,
}

impl SystemSlot {
    /// Numeric priority used internally by the scheduler. Lower runs earlier.
    pub const fn priority(self) -> i32 {
        match self {
            Self::SimInput => 50,
            Self::DeltaTime => 60,
            Self::InteractionState => 80,
            Self::Animation => 150,
            Self::Timer => 150,
            Self::ScrollInertia => 250,
            Self::LazyList => 350,
            Self::TabPages => 350,
            Self::Normal => 500,
            Self::PreRender => 700,
        }
    }
}

impl From<SystemSlot> for i32 {
    fn from(slot: SystemSlot) -> i32 {
        slot.priority()
    }
}

/// Declare the scheduler descriptor for a framework-only raw World callback.
///
/// Ordinary application systems are generated by `#[system]`. This private
/// seam is reserved for runtime code that performs immediate event dispatch,
/// dynamic tree traversal, or another operation the typed authoring facade
/// intentionally cannot express.
macro_rules! framework_system {
    ($visibility:vis $name:ident, $slot:ident $(,)?) => {
        #[allow(non_snake_case)]
        $visibility mod $name {
            #[allow(unused_imports)]
            use super::*;

            pub const fn system() -> $crate::ecs::System {
                $crate::ecs::System::new(
                    stringify!($name),
                    $crate::ecs::SystemSlot::$slot.priority(),
                    super::$name,
                )
            }
        }
    };
    ($visibility:vis $name:ident, $slot:ident, expect = [$($expected:ty),+ $(,)?] $(,)?) => {
        #[allow(non_snake_case)]
        $visibility mod $name {
            #[allow(unused_imports)]
            use super::*;

            const ACCESS: &[$crate::__macro_support::SystemAccess] = &[
                $(
                    $crate::__macro_support::SystemAccess::expect_component::<$expected>()
                ),+
            ];

            pub const fn system() -> $crate::ecs::System {
                // SAFETY: ACCESS contains only the expect prefix; the raw
                // callback owns the entire World and has no typed alias plan.
                unsafe {
                    $crate::ecs::System::new(
                        stringify!($name),
                        $crate::ecs::SystemSlot::$slot.priority(),
                        super::$name,
                    )
                    .with_metadata(ACCESS)
                }
            }
        }
    };
}

pub(crate) use framework_system;

struct PreparedSystem {
    system: System,
    commands: Option<CommandQueue>,
}

impl PreparedSystem {
    fn new(system: System) -> Self {
        system.validate_metadata();
        let commands = system.callback.uses_commands().then(CommandQueue::default);
        Self { system, commands }
    }

    fn invoke(&mut self, world: &mut World) -> Result<(), CommandError> {
        if let Some(queue) = &mut self.commands {
            let mut batch = queue.begin();
            self.system
                .callback
                .invoke_deferred(world, batch.commands())?;
            batch.finish(world)?;
        } else {
            self.system.callback.invoke_plain(world)?;
        }
        Ok(())
    }
}

/// A deferred command failure from one scheduled system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemRunError {
    pub system: &'static str,
    pub cause: CommandError,
}

#[derive(Default)]
pub struct SystemScheduler {
    systems: Vec<PreparedSystem>,
}

impl SystemScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stable on equal priority — registration order breaks ties.
    pub fn add(&mut self, system: System) {
        let system = PreparedSystem::new(system);
        let pos = self
            .systems
            .iter()
            .position(|prepared| prepared.system.priority > system.system.priority)
            .unwrap_or(self.systems.len());
        self.systems.insert(pos, system);
    }

    /// Run eligible systems in priority and registration order.
    ///
    /// The first command failure stops this batch. Completed updates remain; unexecuted systems are not counted or timed as calls.
    pub fn run_all(&mut self, world: &mut World) -> Result<(), SystemRunError> {
        let clock_fn = world.resource::<super::MonoClock>().map(|c| c.clock);
        for index in 0..self.systems.len() {
            let prepared = &mut self.systems[index];
            prepared.system.assert_owner(world);
            if !prepared.system.should_run(world) {
                prepared.system.last_us = 0;
                continue;
            }
            let start_ns = clock_fn.map(|f| f()).unwrap_or(0);
            let result = prepared.invoke(world);
            if let Some(f) = clock_fn {
                let elapsed_ns = f().saturating_sub(start_ns);
                let elapsed_us = (elapsed_ns / 1_000) as u32;
                let system = &mut prepared.system;
                system.last_us = elapsed_us;
                system.total_us = system.total_us.saturating_add(elapsed_us as u64);
                system.call_count = system.call_count.saturating_add(1);
            }
            if let Err(cause) = result {
                let error = SystemRunError {
                    system: prepared.system.name,
                    cause,
                };
                for skipped in &mut self.systems[index + 1..] {
                    skipped.system.last_us = 0;
                }
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn reset_perf(&mut self) {
        for prepared in &mut self.systems {
            let system = &mut prepared.system;
            system.last_us = 0;
            system.total_us = 0;
            system.call_count = 0;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &System> {
        self.systems.iter().map(|prepared| &prepared.system)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

    fn dummy(_: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DeferredMarker(u32);

    impl Component for DeferredMarker {}

    #[derive(Clone, Copy)]
    struct DeferredTarget(super::super::Entity);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct RawRuns(u32);

    fn framework_raw(world: &mut World) -> Result<(), CommandError> {
        let next = world.resource::<RawRuns>().map_or(1, |runs| runs.0 + 1);
        world.insert_resource(RawRuns(next));
        Ok(())
    }

    super::framework_system!(framework_raw, Normal, expect = [DeferredMarker]);

    fn framework_raw_always(_world: &mut World) -> Result<(), CommandError> {
        Ok(())
    }

    super::framework_system!(framework_raw_always, PreRender);

    fn replace_marker_after_return(
        world: &mut World,
        mut commands: DeferredCommands<'_>,
    ) -> Result<(), CommandError> {
        let entity = world.resource::<DeferredTarget>().unwrap().0;
        let before = world.get::<DeferredMarker>(entity).unwrap().0;
        commands.insert(entity, DeferredMarker(1));
        commands.insert(entity, DeferredMarker(2));
        assert_eq!(world.get::<DeferredMarker>(entity).unwrap().0, before);
        Ok(())
    }

    #[test]
    fn scheduler_applies_deferred_commands_after_system_body() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, DeferredMarker(0));
        world.insert_resource(DeferredTarget(entity));

        let mut scheduler = SystemScheduler::new();
        scheduler.add(System::deferred(
            "replace-marker",
            100,
            replace_marker_after_return,
        ));
        scheduler.run_all(&mut world).unwrap();

        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(2))
        );
    }

    fn reject_before_submission(
        world: &mut World,
        commands: DeferredCommands<'_>,
    ) -> Result<(), CommandError> {
        replace_marker_after_return(world, commands)?;
        Err(CommandError::Busy)
    }

    #[test]
    fn callback_failure_discards_unsubmitted_commands_and_reuses_storage() {
        let mut world = World::new();
        let entity = world.spawn(DeferredMarker(0));
        world.insert_resource(DeferredTarget(entity));
        let mut scheduler = SystemScheduler::new();
        scheduler.add(System::deferred("rejected", 100, reject_before_submission));
        let expected = SystemRunError {
            system: "rejected",
            cause: CommandError::Busy,
        };
        assert_eq!(scheduler.run_all(&mut world), Err(expected));
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(scheduler.run_all(&mut world), Err(expected));
            }),
            0
        );
        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(0))
        );
    }

    #[test]
    fn framework_raw_descriptor_preserves_slot_and_expect_gate() {
        assert_eq!(
            framework_raw_always::system().priority,
            SystemSlot::PreRender.priority()
        );

        let mut world = World::new();
        let system = framework_raw::system();
        system.run_once(&mut world).unwrap();
        assert_eq!(world.resource::<RawRuns>(), None);

        let entity = world.spawn_empty();
        world.insert(entity, DeferredMarker(0));
        system.run_once(&mut world).unwrap();
        assert_eq!(world.resource::<RawRuns>(), Some(&RawRuns(1)));
    }

    #[test]
    fn warmed_deferred_system_run_does_not_allocate() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, DeferredMarker(0));
        world.insert_resource(DeferredTarget(entity));

        let mut scheduler = SystemScheduler::new();
        scheduler.add(System::deferred(
            "replace-marker",
            100,
            replace_marker_after_return,
        ));
        scheduler.run_all(&mut world).unwrap();

        let allocations = tracked_allocations(|| scheduler.run_all(&mut world).unwrap());

        assert_eq!(allocations, 0);
        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(2))
        );
    }

    struct MarkerChanged;

    impl crate::input::event::contract::Event for MarkerChanged {
        type Route = crate::input::event::contract::EntityNotification;
        type Source = crate::input::event::contract::WidgetSource<DeferredMarker>;
        type Options = ();
    }

    #[crate::system]
    fn submit_marker(target: res!(DeferredTarget), commands: commands!()) {
        commands.insert(target.0, DeferredMarker(1));
        commands.emit(target.0, MarkerChanged);
        commands.insert(target.0, DeferredMarker(2));
    }

    fn failure_clock() -> u64 {
        use core::sync::atomic::{AtomicU64, Ordering};
        static TICKS: AtomicU64 = AtomicU64::new(0);
        TICKS.fetch_add(1_000, Ordering::Relaxed)
    }

    #[test]
    fn submission_failure_stops_the_batch_preserves_updates_and_reuses_storage() {
        use crate::ecs::commands::CommandCommitError;
        use crate::input::event::emission::{flush_notifications, prepare_notifications};
        use crate::input::event::listeners::EventListeners;
        use crate::input::event::notification::NotificationCapacity;

        let mut world = World::new();
        let entity = world.spawn(DeferredMarker(0));
        world.insert_resource(DeferredTarget(entity));
        world.insert_resource(RawRuns(0));
        world.insert_resource(super::super::MonoClock::new(failure_clock));
        prepare_notifications(
            &mut world,
            NotificationCapacity {
                records: 1,
                payload_bytes: 0,
                payload_alignment: 1,
            },
        )
        .unwrap();
        let listeners = EventListeners::new(&mut world);
        let mut scheduler = SystemScheduler::new();
        scheduler.add(framework_raw::system());
        scheduler.add(submit_marker::system());
        scheduler.add(framework_raw_always::system());
        scheduler.run_all(&mut world).unwrap();
        assert!(scheduler.iter().all(|system| system.call_count == 1));

        let expected = SystemRunError {
            system: "submit_marker",
            cause: CommandError::Commit(CommandCommitError::RecordCapacity { available: 1 }),
        };
        assert_eq!(
            tracked_allocations(|| assert_eq!(scheduler.run_all(&mut world), Err(expected))),
            0
        );
        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(1))
        );
        assert_eq!(world.resource::<RawRuns>(), Some(&RawRuns(2)));
        let stats: Vec<_> = scheduler
            .iter()
            .map(|system| (system.last_us, system.total_us, system.call_count))
            .collect();
        assert_eq!(stats, [(1, 2, 2), (1, 2, 2), (0, 1, 1)]);

        assert_eq!(flush_notifications(&mut world, &listeners, 1), Some(Ok(1)));
        assert_eq!(
            tracked_allocations(|| scheduler.run_all(&mut world).unwrap()),
            0
        );
        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(2))
        );
        assert_eq!(flush_notifications(&mut world, &listeners, 1), Some(Ok(1)));
        assert_eq!(world.resource::<RawRuns>(), Some(&RawRuns(3)));
        assert_eq!(scheduler.iter().last().unwrap().call_count, 2);
    }

    #[test]
    fn direct_execution_returns_submission_errors() {
        let mut world = World::new();
        let entity = world.spawn(DeferredMarker(0));
        world.insert_resource(DeferredTarget(entity));
        assert!(matches!(
            submit_marker::system().run_once(&mut world),
            Err(CommandError::Commit(
                super::super::CommandCommitError::MissingService { .. }
            ))
        ));
        assert_eq!(
            world.get::<DeferredMarker>(entity),
            Some(&DeferredMarker(1))
        );
    }

    #[test]
    fn run_order_orders_ascending() {
        let mut s = SystemScheduler::new();
        s.add(System::new("late", 300, dummy));
        s.add(System::new("early", 100, dummy));
        s.add(System::new("mid", 200, dummy));
        let names: Vec<&str> = s.iter().map(|s| s.name).collect();
        assert_eq!(names, ["early", "mid", "late"]);
    }

    #[test]
    fn expect_skips_system_when_no_entity_has_component() {
        struct Marker;
        impl Component for Marker {}
        struct Counter(u32);

        fn ticking(world: &mut World) -> Result<(), CommandError> {
            if let Some(c) = world.resource_mut::<Counter>() {
                c.0 += 1;
            }
            Ok(())
        }

        let mut world = World::new();
        world.insert_resource(Counter(0));

        let mut s = SystemScheduler::new();
        const METADATA: &[SystemAccess] = &[SystemAccess::expect_component::<Marker>()];
        // SAFETY: ticking only touches Counter; METADATA adds an expect gate.
        s.add(unsafe { System::new("ticking", 100, ticking).with_metadata(METADATA) });

        // No entity carries `Marker` → system must not run.
        s.run_all(&mut world).unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 0);

        // Spawn an entity with `Marker` → system runs.
        let e = world.spawn_empty();
        world.insert(e, Marker);
        s.run_all(&mut world).unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 1);

        // Despawn it → system stops running again.
        world.despawn(e);
        s.run_all(&mut world).unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 1);
    }

    #[test]
    fn equal_run_order_preserves_insertion_order() {
        let mut s = SystemScheduler::new();
        s.add(System::new("a", 200, dummy));
        s.add(System::new("b", 200, dummy));
        s.add(System::new("c", 100, dummy));
        let names: Vec<&str> = s.iter().map(|s| s.name).collect();
        assert_eq!(names, ["c", "a", "b"]);
    }

    #[test]
    fn system_slot_priorities_are_stable() {
        assert_eq!(SystemSlot::SimInput.priority(), 50);
        assert_eq!(SystemSlot::DeltaTime.priority(), 60);
        assert_eq!(SystemSlot::InteractionState.priority(), 80);
        assert_eq!(SystemSlot::Animation.priority(), 150);
        assert_eq!(SystemSlot::Timer.priority(), 150);
        assert_eq!(SystemSlot::ScrollInertia.priority(), 250);
        assert_eq!(SystemSlot::LazyList.priority(), 350);
        assert_eq!(SystemSlot::TabPages.priority(), 350);
        assert_eq!(SystemSlot::Normal.priority(), 500);
        assert_eq!(SystemSlot::PreRender.priority(), 700);
    }

    #[test]
    fn system_slot_into_i32_returns_priority() {
        let p: i32 = SystemSlot::Animation.into();
        assert_eq!(p, 150);
    }

    /// Architecture invariant: every plugin in src/plugins/*.rs must
    /// have a `**Inserts**` section in a docstring that survives into
    /// the source file (i.e. the docstring directly above the struct).
    /// This is the v0.17.1 plugin documentation contract; if a plugin
    /// adds resources / systems / views silently, this test reminds the
    /// author to document them.
    #[test]
    fn plugins_declare_inserts_section() {
        let plugins = [
            ("perf_report", include_str!("../app/plugins/perf_report.rs")),
            ("fps_summary", include_str!("../app/plugins/fps_summary.rs")),
            (
                "input_feedback",
                include_str!("../app/plugins/input_feedback.rs"),
            ),
            #[cfg(feature = "std")]
            ("std_clock", include_str!("../app/plugins/std_clock.rs")),
        ];
        for (name, src) in plugins {
            assert!(
                src.contains("**Inserts**"),
                "plugin {name} missing **Inserts** doc section",
            );
        }
    }

    #[test]
    #[should_panic(expected = "conflicting access")]
    fn scheduler_rejects_component_read_write_alias() {
        struct Marker;
        impl Component for Marker {}

        const METADATA: &[SystemAccess] = &[
            SystemAccess::read_component::<Marker>(),
            SystemAccess::write_component::<Marker>(),
        ];
        let mut scheduler = SystemScheduler::new();
        // SAFETY: deliberately invalid metadata exercises registration checks.
        scheduler.add(unsafe { System::new("alias", 100, dummy).with_metadata(METADATA) });
    }

    #[test]
    #[should_panic(expected = "conflicting access")]
    fn direct_run_validates_metadata() {
        struct Settings;

        const METADATA: &[SystemAccess] = &[
            SystemAccess::read_resource::<Settings>(),
            SystemAccess::write_resource::<Settings>(),
        ];
        let mut world = World::new();
        // SAFETY: deliberately invalid metadata exercises direct-run checks.
        unsafe { System::new("alias", 100, dummy).with_metadata(METADATA) }
            .run_once(&mut world)
            .unwrap();
    }

    #[test]
    fn same_type_in_component_and_resource_domains_is_allowed() {
        struct Marker;
        impl Component for Marker {}

        const METADATA: &[SystemAccess] = &[
            SystemAccess::write_component::<Marker>(),
            SystemAccess::write_resource::<Marker>(),
        ];
        let mut scheduler = SystemScheduler::new();
        // SAFETY: dummy performs no access; the metadata is conservative.
        scheduler
            .add(unsafe { System::new("separate-domains", 100, dummy).with_metadata(METADATA) });
    }

    #[test]
    #[should_panic(expected = "expect metadata after access metadata")]
    fn expect_metadata_must_form_a_prefix() {
        struct Marker;
        impl Component for Marker {}

        const METADATA: &[SystemAccess] = &[
            SystemAccess::read_component::<Marker>(),
            SystemAccess::expect_component::<Marker>(),
        ];
        let mut scheduler = SystemScheduler::new();
        // SAFETY: deliberately invalid ordering exercises registration checks.
        scheduler.add(unsafe { System::new("bad-order", 100, dummy).with_metadata(METADATA) });
    }
}
