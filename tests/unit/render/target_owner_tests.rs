use super::*;
use crate::render::{RenderResource, budget::MemoryBudget};
use crate::test_support::tracked_allocations;
use core::cell::Cell;

fn receipt(budget: &MemoryBudget, bytes: usize) -> TargetStorage {
    TargetStorage::new(
        budget.reserve(bytes).unwrap().commit(bytes).unwrap(),
        budget.reserve(24).unwrap(),
        budget
            .reserve(TargetStorage::OWNER_BYTES)
            .unwrap()
            .commit(TargetStorage::OWNER_BYTES)
            .unwrap(),
    )
}

struct Local {
    drops: Rc<Cell<usize>>,
    thread: ThreadId,
    budget: MemoryBudget,
}

impl Local {
    fn new(drops: &Rc<Cell<usize>>, budget: &MemoryBudget) -> Self {
        Self {
            drops: drops.clone(),
            thread: std::thread::current().id(),
            budget: budget.clone(),
        }
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        assert_eq!(self.thread, std::thread::current().id());
        assert_ne!(
            self.budget.allocated(),
            0,
            "raw destruction retains admission"
        );
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn tokens_transfer_without_transferring_native_values_or_collecting_other_owners() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ThreadResource<Local>>();
    send_sync::<TargetStorage>();
    trait Ambiguous<A> {
        fn check() {}
    }
    impl<T: ?Sized> Ambiguous<()> for T {}
    struct ThreadSafe;
    impl<T: Send + ?Sized> Ambiguous<ThreadSafe> for T {}
    let _ = <ThreadOwner as Ambiguous<_>>::check;

    let budget = MemoryBudget::new(8192);
    let drops = Rc::new(Cell::new(0));
    let mut first = ThreadOwner::new();
    let mut second = ThreadOwner::new();
    let a = first.insert(Local::new(&drops, &budget)).unwrap();
    let b = second.insert(Local::new(&drops, &budget)).unwrap();
    let a_bytes = a.metadata_bytes().unwrap();
    a.retain_storage(receipt(&budget, a_bytes + 32)).unwrap();
    b.retain_storage(receipt(&budget, b.metadata_bytes().unwrap() + 32))
        .unwrap();
    assert_eq!(a.metadata_bytes().unwrap(), a_bytes);
    assert_eq!(
        first.metadata_bytes::<Local>().unwrap(),
        entry_bytes::<Local>().unwrap()
    );
    let transfer = a.clone();
    assert_eq!(
        std::thread::spawn(move || {
            transfer.validate().unwrap();
            assert_eq!(transfer.with(|_| Ok(())), Err(RenderError::WrongThread));
            tracked_allocations(|| drop(transfer))
        })
        .join()
        .unwrap(),
        0
    );
    drop(b);
    first.collect().unwrap();
    assert_eq!(drops.get(), 0);
    assert_eq!(tracked_allocations(|| second.collect().unwrap()), 0);
    assert_eq!(drops.get(), 1);
    assert_eq!(tracked_allocations(|| drop(a)), 0);
    assert_eq!(tracked_allocations(|| first.collect().unwrap()), 0);
    assert_eq!(drops.get(), 2);
    drop(first);
    drop(second);
    assert_eq!(budget.allocated(), 0);
    assert_eq!(budget.available(), 8192);
}

#[test]
fn close_invalidates_tokens_and_keeps_metadata_charged_until_the_last_worker_release() {
    let budget = MemoryBudget::new(8192);
    let drops = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let token = owner.insert(Local::new(&drops, &budget)).unwrap();
    token
        .retain_storage(receipt(&budget, token.metadata_bytes().unwrap() + 32))
        .unwrap();
    assert_eq!(tracked_allocations(|| owner.close().unwrap()), 0);
    assert_eq!(drops.get(), 1);
    assert_eq!(token.with(|_| Ok(())), Err(RenderError::InvalidTexture));
    assert_eq!(token.validate(), Err(RenderError::InvalidTexture));
    assert_eq!(
        budget.allocated(),
        core::mem::size_of::<Owner>() + core::mem::size_of::<Control>() + 2 * ARC_HEADER
    );
    drop(owner);
    let available = budget.available();
    assert_ne!(available, 8192);
    assert_eq!(
        std::thread::spawn(move || {
            assert_eq!(token.with(|_| Ok(())), Err(RenderError::InvalidTexture));
            tracked_allocations(|| drop(token))
        })
        .join()
        .unwrap(),
        0
    );
    assert_eq!(budget.available(), 8192);
}

#[test]
fn scoped_access_restores_on_error_unwind_and_owner_close() {
    let budget = MemoryBudget::new(8192);
    let drops = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let token = owner.insert(Local::new(&drops, &budget)).unwrap();
    token
        .retain_storage(receipt(&budget, token.metadata_bytes().unwrap() + 32))
        .unwrap();
    assert_eq!(
        token.with::<()>(|_| Err(RenderError::BackendFailure)),
        Err(RenderError::BackendFailure)
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = token.with::<()>(|_| panic!("native callback"));
        }))
        .is_err()
    );
    token
        .with(|_| {
            assert_eq!(token.with(|_| Ok(())), Err(RenderError::InvalidScope));
            owner.collect()?;
            Ok(())
        })
        .unwrap();
    token
        .with(|_| {
            owner.close()?;
            assert_eq!(
                drops.get(),
                0,
                "active storage survives until callback return"
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(drops.get(), 1);
    drop(token);
    drop(owner);
    assert_eq!(budget.available(), 8192);
}

#[test]
fn rejected_or_duplicate_attachment_preserves_the_existing_admission() {
    let budget = MemoryBudget::new(8192);
    let drops = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let token = owner.insert(Local::new(&drops, &budget)).unwrap();
    let too_small = receipt(&budget, 1);
    let charged = budget.allocated();
    assert_eq!(
        token.retain_storage(too_small.clone()),
        Err(RenderError::ResourceLimit(RenderResource::Target))
    );
    assert_eq!(budget.allocated(), charged);
    let receipt = receipt(&budget, token.metadata_bytes().unwrap() + 32);
    token.retain_storage(receipt.clone()).unwrap();
    let charged = budget.allocated();
    assert_eq!(
        token.retain_storage(receipt.clone()),
        Err(RenderError::InvalidScope)
    );
    assert_eq!(budget.allocated(), charged);
    drop(token);
    owner.collect().unwrap();
    assert_eq!(drops.get(), 1);
    drop(receipt);
    drop(too_small);
    drop(owner);
    assert_eq!(budget.available(), 8192);
}

#[test]
fn thread_exit_invalidates_live_tokens_and_reclaims_local_storage_without_owner_drop() {
    let budget = MemoryBudget::new(8192);
    let token = std::thread::spawn({
        let budget = budget.clone();
        move || {
            let drops = Rc::new(Cell::new(0));
            let mut owner = ThreadOwner::new();
            let token = owner.insert(Local::new(&drops, &budget)).unwrap();
            token
                .retain_storage(receipt(&budget, token.metadata_bytes().unwrap() + 32))
                .unwrap();
            // Model a forgotten backend owner: TLS still closes its actual resources.
            core::mem::forget(owner);
            token
        }
    })
    .join()
    .unwrap();
    assert_eq!(token.validate(), Err(RenderError::InvalidTexture));
    let charged = budget.allocated();
    assert_eq!(
        charged,
        core::mem::size_of::<Owner>() + core::mem::size_of::<Control>() + 2 * ARC_HEADER
    );
    drop(token);
    // A deliberately forgotten owner keeps only its own metadata allocation alive.
    assert_eq!(
        budget.allocated(),
        core::mem::size_of::<Owner>() + ARC_HEADER
    );
}

#[test]
fn worker_retirement_racing_with_owner_close_retains_budget_during_raw_destruction() {
    use std::sync::Barrier;
    for _ in 0..128 {
        let budget = MemoryBudget::new(8192);
        let drops = Rc::new(Cell::new(0));
        let mut owner = ThreadOwner::new();
        let token = owner.insert(Local::new(&drops, &budget)).unwrap();
        token
            .retain_storage(receipt(&budget, token.metadata_bytes().unwrap() + 32))
            .unwrap();
        let barrier = Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                drop(token);
            });
            barrier.wait();
            owner.close().unwrap();
        });
        assert_eq!(drops.get(), 1);
        drop(owner);
        assert_eq!(budget.available(), 8192);
    }
}

#[test]
fn racing_final_worker_releases_leave_a_collectible_retirement() {
    use std::sync::Barrier;
    for _ in 0..128 {
        let budget = MemoryBudget::new(8192);
        let drops = Rc::new(Cell::new(0));
        let mut owner = ThreadOwner::new();
        let first = owner.insert(Local::new(&drops, &budget)).unwrap();
        first
            .retain_storage(receipt(&budget, first.metadata_bytes().unwrap() + 32))
            .unwrap();
        let second = first.clone();
        let barrier = Barrier::new(3);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                drop(first);
            });
            scope.spawn(|| {
                barrier.wait();
                drop(second);
            });
            barrier.wait();
            owner.collect().unwrap();
        });
        owner.collect().unwrap();
        assert_eq!(
            drops.get(),
            1,
            "a racing release cannot lose its retirement mark"
        );
        drop(owner);
        assert_eq!(budget.available(), 8192);
    }
}

#[test]
fn raw_destructors_run_outside_the_registry_borrow() {
    struct Reentrant {
        other: ThreadResource<usize>,
        observed: Rc<Cell<usize>>,
    }
    impl Drop for Reentrant {
        fn drop(&mut self) {
            self.other
                .with(|value| {
                    *value += 1;
                    self.observed.set(*value);
                    Ok(())
                })
                .unwrap();
        }
    }
    let budget = MemoryBudget::new(8192);
    let observed = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let other = owner.insert(41_usize).unwrap();
    other
        .retain_storage(receipt(&budget, other.metadata_bytes().unwrap()))
        .unwrap();
    let token = owner
        .insert(Reentrant {
            other: other.clone(),
            observed: observed.clone(),
        })
        .unwrap();
    token
        .retain_storage(receipt(&budget, token.metadata_bytes().unwrap()))
        .unwrap();
    drop(token);
    owner.collect().unwrap();
    assert_eq!(observed.get(), 42);
    drop(other);
    owner.collect().unwrap();
    drop(owner);
    assert_eq!(budget.available(), 8192);
}

#[test]
fn owner_access_and_collection_use_no_warm_allocations_and_report_actual_metadata() {
    let budget = MemoryBudget::new(64 * 1024);
    let drops = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let mut tokens = alloc::vec::Vec::with_capacity(64);
    for _ in 0..64 {
        let expected = owner.metadata_bytes::<Local>().unwrap();
        let token = owner.insert(Local::new(&drops, &budget)).unwrap();
        assert_eq!(token.metadata_bytes().unwrap(), expected);
        token
            .retain_storage(receipt(&budget, expected + 32))
            .unwrap();
        tokens.push(token);
    }
    assert_eq!(
        tracked_allocations(|| {
            for token in &tokens {
                token.with(|_| Ok(())).unwrap();
                owner.collect().unwrap();
            }
            tokens.clear();
            owner.collect().unwrap();
        }),
        0
    );
    assert_eq!(drops.get(), 64);
    drop(owner);
    assert_eq!(budget.available(), 64 * 1024);
    std::println!(
        "owner={} control={} entry={} token={} receipt_owner={}",
        core::mem::size_of::<Owner>() + ARC_HEADER,
        core::mem::size_of::<Control>() + ARC_HEADER,
        core::mem::size_of::<Entry>(),
        core::mem::size_of::<ThreadResource<Local>>(),
        TargetStorage::OWNER_BYTES
    );
}

#[test]
fn unpublished_owner_resources_retire_before_creation_reservations_are_released() {
    struct Unpublished {
        budget: MemoryBudget,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Unpublished {
        fn drop(&mut self) {
            assert_eq!(
                self.budget.available(),
                0,
                "creation admission spans physical destruction"
            );
            self.drops.set(self.drops.get() + 1);
        }
    }
    let budget = MemoryBudget::new(8192);
    let reserved = budget.reserve(8192).unwrap();
    let drops = Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let token = owner
        .insert(Unpublished {
            budget: budget.clone(),
            drops: drops.clone(),
        })
        .unwrap();
    assert_eq!(tracked_allocations(|| drop(token)), 0);
    assert_eq!(
        drops.get(),
        1,
        "unpublished failures do not need a later frame"
    );
    drop(reserved);
    drop(owner);
    assert_eq!(budget.available(), 8192);
}

#[test]
fn rejected_native_allocation_destroys_the_target_before_returning_its_reservation() {
    use crate::render::texture::{ColorFormat, Region, Texture, TexturePixelsMut};
    use crate::render::{
        BackendTarget, DrawRequest, PlaneLayout, PreparedDraw, RenderRoute, Renderer, TargetAlpha,
        TargetDescriptor, TargetPlan,
    };
    use core::any::Any;

    struct Raw {
        budget: MemoryBudget,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Raw {
        fn drop(&mut self) {
            assert_eq!(self.budget.available(), 0);
            self.drops.set(self.drops.get() + 1);
        }
    }
    struct Target {
        raw: ThreadResource<Raw>,
        plan: TargetPlan,
    }
    impl BackendTarget for Target {
        fn descriptor(&self) -> TargetDescriptor {
            self.plan.descriptor
        }
        fn allocated_bytes(&self) -> usize {
            self.plan.storage_bytes + 1
        }
        fn validate(&self) -> Result<(), RenderError> {
            self.raw.validate()
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn read_pixels_into(
            &self,
            _: Region,
            _: &mut TexturePixelsMut<'_>,
        ) -> Result<(), RenderError> {
            unreachable!()
        }
        fn retain_storage(&mut self, _: TargetStorage) -> Result<(), RenderError> {
            panic!("allocation must be rejected before retention")
        }
    }
    struct Producer<'a> {
        owner: &'a mut ThreadOwner,
        budget: MemoryBudget,
        drops: Rc<Cell<usize>>,
    }
    impl Renderer for Producer<'_> {
        type TargetState = ();
        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            unreachable!()
        }
        fn submit(&mut self, _: &PreparedDraw<'_>) -> Result<(), RenderError> {
            unreachable!()
        }
        fn flush(&mut self) -> Result<(), RenderError> {
            unreachable!()
        }
        fn can_reuse_target(
            &self,
            _: &dyn BackendTarget,
            _: &TargetPlan,
        ) -> Result<bool, RenderError> {
            Ok(true)
        }
        fn create_target(
            &mut self,
            plan: &TargetPlan,
        ) -> Result<Box<dyn BackendTarget>, RenderError> {
            Ok(Box::new(Target {
                raw: self.owner.insert(Raw {
                    budget: self.budget.clone(),
                    drops: self.drops.clone(),
                })?,
                plan: *plan,
            }))
        }
    }
    let mut owner = ThreadOwner::new();
    let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
    let plan = TargetPlan {
        descriptor: TargetDescriptor {
            layout,
            alpha: TargetAlpha::Straight,
        },
        storage_bytes: owner.metadata_bytes::<Raw>().unwrap() + 4,
        workspace_bytes: 0,
        readback_bytes: 0,
    };
    let limit = plan.storage_bytes + Texture::native_cpu_bytes(layout).unwrap();
    let budget = MemoryBudget::new(limit);
    let drops = Rc::new(Cell::new(0));
    let mut producer = Producer {
        owner: &mut owner,
        budget: budget.clone(),
        drops: drops.clone(),
    };
    assert!(matches!(
        Texture::native(&mut producer, plan, &budget),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert_eq!(drops.get(), 1);
    assert_eq!(budget.available(), limit);
}
