use super::*;
use crate::ui::dirty::{Dirty, VisualDirty};
use crate::ui::retained;

// std test threads are pooled and reused, so the per-thread runtime can
// carry residue between tests on the same thread — reset at entry.
fn reset() {
    with_reactive(|r| {
        r.scope = None;
        r.world_window = None;
        r.world_id = None;
        r.world_lifetime = None;
        r.borrowed_worlds = [None; 8];
        r.dirty_widgets.clear();
        r.dirty_visual_widgets.clear();
        r.dirty_effects.clear();
        r.computeds.clear();
    });
    while let Some(id) = with_reactive(|r| r.effects.keys().next().copied()) {
        unregister_effect(id);
    }
    with_reactive(|r| {
        assert_eq!(r.unbound_effects, 0);
        r.graph = ReactiveGraph::new();
        r.effect_index = EffectIndex::new();
    });
}

fn entity(id: u32) -> Entity {
    Entity { id, generation: 0 }
}

fn assert_unbound_effects(expected: usize) {
    with_reactive(|runtime| {
        assert_eq!(runtime.unbound_effect_count(), expected);
        assert_eq!(
            runtime.unbound_effect_count(),
            runtime
                .effects
                .values()
                .filter(|effect| effect.borrow().owner_world.is_none())
                .count()
        );
    });
}

#[test]
fn unbound_count_tracks_binding_disposal_and_failed_initialization() {
    reset();
    assert_unbound_effects(0);
    let shared = Effect::new(|| {});
    assert_unbound_effects(1);
    let mut world = World::new();
    let owned = with_world_scope(&mut world, || Effect::new(|| {}));
    assert_unbound_effects(1);
    let model = Rc::new(ModelSource::new());
    model.register_owner(world.id());
    let gate = Signal::new(false);
    let (read_gate, read_model) = (gate.clone(), Rc::clone(&model));
    let bound_later = Effect::new(move || {
        if read_gate.get() {
            read_model.track();
        }
    });
    assert_unbound_effects(2);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            gate.set(true);
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_unbound_effects(1);
    drop(bound_later);
    drop(owned);
    assert_unbound_effects(1);
    drop(shared);
    assert_unbound_effects(0);
    for bind_before_failure in [false, true] {
        let read = Rc::clone(&model);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _effect = Effect::new(move || {
                if bind_before_failure {
                    read.track();
                }
                panic!("initial effect failed");
            });
        }));
        assert!(failed.is_err());
        assert_unbound_effects(0);
    }
}

#[test]
fn unbound_count_is_updated_before_reentrant_capture_destruction() {
    struct SpawnOnDrop(Rc<RefCell<Option<Effect>>>);
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            assert_unbound_effects(0);
            *self.0.borrow_mut() = Some(Effect::new(|| {}));
        }
    }
    reset();
    let spawned = Rc::new(RefCell::new(None));
    let capture = SpawnOnDrop(Rc::clone(&spawned));
    let effect = Effect::new(move || {
        let _ = &capture;
    });
    assert_unbound_effects(1);
    drop(effect);
    assert_unbound_effects(1);
    drop(spawned.borrow_mut().take());
    assert_unbound_effects(0);
}

#[test]
fn indexed_entity_cleanup_rechecks_effects_registered_by_capture_drop() {
    struct Record(Rc<RefCell<Vec<u8>>>, u8);
    impl Drop for Record {
        fn drop(&mut self) {
            self.0.borrow_mut().push(self.1);
        }
    }
    struct ReplaceOnDrop {
        entity: Entity,
        replace: Rc<Cell<bool>>,
        seen: Rc<RefCell<Vec<u8>>>,
    }
    impl Drop for ReplaceOnDrop {
        fn drop(&mut self) {
            self.seen
                .borrow_mut()
                .push(if self.replace.get() { 1 } else { 3 });
            if self.replace.get() {
                let capture = Record(Rc::clone(&self.seen), 2);
                effect_with_widget(self.entity, move || {
                    let _ = &capture;
                });
            }
        }
    }
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let seen = Rc::new(RefCell::new(Vec::with_capacity(3)));
    let first_replace = Rc::new(Cell::new(false));
    let second_replace = Rc::new(Cell::new(false));
    let spawn = |replace: Rc<Cell<bool>>| {
        let capture = ReplaceOnDrop {
            entity,
            replace,
            seen: Rc::clone(&seen),
        };
        Effect::spawn(
            move || {
                let _ = &capture;
            },
            Some(entity),
            BindingRole::Ordinary,
        )
    };
    let (first, second) = with_world_scope(&mut world, || {
        (
            spawn(Rc::clone(&first_replace)),
            spawn(Rc::clone(&second_replace)),
        )
    });
    if first.id < second.id {
        first_replace.set(true);
    } else {
        second_replace.set(true);
    }
    with_world_scope(&mut world, || {
        with_world(|world| cleanup_effects_for(world, entity))
    });
    assert_eq!(&*seen.borrow(), &[1, 2, 3]);
    assert_eq!(effect_count(), 0);
    with_reactive(|runtime| {
        assert_eq!(
            runtime
                .effect_index
                .first_entity(world.id(), entity, |_| false),
            None
        )
    });
    drop((first, second));
}

#[test]
fn indexed_world_cleanup_keeps_registry_order_across_entities() {
    struct Record(Rc<RefCell<Vec<u8>>>, u8);
    impl Drop for Record {
        fn drop(&mut self) {
            self.0.borrow_mut().push(self.1);
        }
    }
    reset();
    let mut world = World::new();
    let a = world.spawn_empty();
    let b = world.spawn_empty();
    let seen = Rc::new(RefCell::new(Vec::with_capacity(3)));
    let mut expected = Vec::new();
    let effects = with_world_scope(&mut world, || {
        [Some(b), Some(a), None]
            .into_iter()
            .enumerate()
            .map(|(tag, entity)| {
                let capture = Record(Rc::clone(&seen), tag as u8);
                let effect = Effect::spawn(
                    move || {
                        let _ = &capture;
                    },
                    entity,
                    BindingRole::Ordinary,
                );
                expected.push((effect.id, tag as u8));
                effect
            })
            .collect::<Vec<_>>()
    });
    expected.sort_by_key(|(id, _)| *id);
    drop(world);
    assert_eq!(
        &*seen.borrow(),
        &expected.into_iter().map(|(_, tag)| tag).collect::<Vec<_>>()
    );
    assert_eq!(effect_count(), 0);
    drop(effects);
}

#[test]
fn empty_entity_cleanup_never_borrows_an_unrelated_effect() {
    reset();
    let mut world = World::new();
    let owned = world.spawn_empty();
    let empty = world.spawn_empty();
    let effect = with_world_scope(&mut world, || {
        Effect::spawn(|| {}, Some(owned), BindingRole::Ordinary)
    });
    let inner = with_reactive(|runtime| Rc::clone(&runtime.effects[&effect.id]));
    let unrelated = inner.borrow_mut();
    assert_eq!(
        crate::test_support::tracked_allocations(|| cleanup_effects_for(&world, empty)),
        0
    );
    drop(unrelated);
    drop(effect);
}

#[test]
fn world_release_preserves_entity_cleanup_requested_by_capture_drop() {
    struct Record(Rc<RefCell<Vec<u8>>>);
    impl Drop for Record {
        fn drop(&mut self) {
            self.0.borrow_mut().push(2);
        }
    }
    struct RegisterAndClean(Entity, Rc<RefCell<Vec<u8>>>);
    impl Drop for RegisterAndClean {
        fn drop(&mut self) {
            self.1.borrow_mut().push(1);
            let capture = Record(Rc::clone(&self.1));
            effect_with_widget(self.0, move || {
                let _ = &capture;
            });
            with_world(|world| cleanup_effects_for(world, self.0));
        }
    }
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let owner = world.id();
    let seen = Rc::new(RefCell::new(Vec::with_capacity(2)));
    let capture = RegisterAndClean(entity, Rc::clone(&seen));
    with_world_scope(&mut world, || {
        effect_with_widget(entity, move || {
            let _ = &capture;
        });
        release_graph_owner(owner);
    });
    assert_eq!(&*seen.borrow(), &[1, 2]);
    assert_eq!(effect_count(), 0);
    assert!(!has_graph_owner(owner));
}

mod paint_requests {
    use super::*;

    #[test]
    fn resource_waits_survive_success_and_failure_without_requesting_busy_frames() {
        reset();
        let mut world = World::new();
        let wake = Rc::new(PaintWake::new());
        world.insert_resource(wake.clone());
        let fallback = world.spawn_empty();
        let strict = world.spawn_empty();
        assert!(reserve_paint_requests(2));
        let completed = PaintRequestScope::new(&world);
        assert!(completed.wait_for_resource(fallback));
        assert!(completed.wait_for_resource(fallback));
        completed.commit();
        let failed = PaintRequestScope::new(&world);
        assert!(failed.wait_for_resource(strict));
        drop(failed);
        flush_signal_dirty(&mut world);
        assert!(!world.has::<VisualDirty>(fallback));
        assert!(!world.has::<VisualDirty>(strict));
        wake.0.waker().wake_by_ref();
        assert!(!world.has::<VisualDirty>(fallback));
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(fallback));
        assert!(world.has::<VisualDirty>(strict));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn resource_waits_do_not_cross_worlds_or_revive_removed_entities() {
        reset();
        let mut first = World::new();
        let mut second = World::new();
        let first_wake = Rc::new(PaintWake::new());
        let second_wake = Rc::new(PaintWake::new());
        first.insert_resource(first_wake.clone());
        second.insert_resource(second_wake.clone());
        let removed = first.spawn_empty();
        let survivor = second.spawn_empty();
        assert!(reserve_paint_requests(2));
        assert!(PaintRequestScope::new(&first).wait_for_resource(removed));
        assert!(PaintRequestScope::new(&second).wait_for_resource(survivor));
        first.despawn(removed);
        let replacement = first.spawn_empty();
        first_wake.0.waker().wake_by_ref();
        flush_signal_dirty(&mut first);
        flush_signal_dirty(&mut second);
        assert!(!first.has::<VisualDirty>(replacement));
        assert!(!second.has::<VisualDirty>(survivor));
        second_wake.0.waker().wake_by_ref();
        flush_signal_dirty(&mut second);
        assert!(second.has::<VisualDirty>(survivor));
    }

    #[test]
    fn pending_requests_are_deduplicated_and_flush_only_after_commit() {
        reset();
        let mut world = World::new();
        let entity = world.spawn_empty();
        assert!(reserve_paint_requests(1));
        let scope = PaintRequestScope::new(&world);
        assert!(scope.request(entity));
        assert!(scope.request(entity));
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);
        flush_signal_dirty(&mut world);
        assert!(!world.has::<VisualDirty>(entity));
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);

        scope.commit();
        assert!(!world.has::<VisualDirty>(entity));
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(entity));
        assert!(!world.has::<Dirty>(entity));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn ordinary_requests_upgrade_conditional_requests_without_consuming_slots() {
        reset();
        let mut world = World::new();
        let first_entity = world.spawn_empty();
        let second_entity = world.spawn_empty();
        assert!(reserve_paint_requests(2));
        let scope = PaintRequestScope::new(&world);
        assert!(!scope.has_change_requests());
        assert!(scope.request_on_change(first_entity));
        assert!(scope.request_on_change(first_entity));
        assert!(scope.has_change_requests());
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);

        assert!(scope.request(first_entity));
        assert!(!scope.has_change_requests());
        assert!(scope.request_on_change(first_entity));
        assert!(!scope.has_change_requests());
        assert!(scope.request(second_entity));
        assert!(scope.request_on_change(second_entity));
        assert!(!scope.has_change_requests());
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 2);

        scope.commit_output(false);
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(first_entity));
        assert!(world.has::<VisualDirty>(second_entity));
    }

    #[test]
    fn unchanged_outputs_discard_only_their_own_conditional_requests() {
        reset();
        let mut world = World::new();
        let ordinary_entity = world.spawn_empty();
        let unchanged_entity = world.spawn_empty();
        let successful_entity = world.spawn_empty();
        assert!(reserve_paint_requests(6));
        let parent = PaintRequestScope::new(&world);
        assert!(parent.request(ordinary_entity));
        assert!(parent.request_on_change(unchanged_entity));
        assert!(parent.request_on_change(successful_entity));
        let child = PaintRequestScope::new(&world);
        assert!(!child.has_change_requests());
        assert!(child.request_on_change(successful_entity));
        assert!(child.has_change_requests());
        child.commit_output(true);
        let failed = PaintRequestScope::new(&world);
        assert!(failed.request_on_change(unchanged_entity));
        assert!(failed.request_on_change(successful_entity));
        drop(failed);
        parent.commit_output(false);

        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 2);
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(ordinary_entity));
        assert!(!world.has::<VisualDirty>(unchanged_entity));
        assert!(world.has::<VisualDirty>(successful_entity));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn ordinary_commit_does_not_publish_conditional_requests() {
        reset();
        let mut world = World::new();
        let ordinary_entity = world.spawn_empty();
        let conditional_entity = world.spawn_empty();
        assert!(reserve_paint_requests(2));
        let scope = PaintRequestScope::new(&world);
        assert!(scope.request(ordinary_entity));
        assert!(scope.request_on_change(conditional_entity));
        scope.commit();
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(ordinary_entity));
        assert!(!world.has::<VisualDirty>(conditional_entity));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn successful_child_survives_parent_and_sibling_failure() {
        reset();
        let mut world = World::new();
        let parent_entity = world.spawn_empty();
        let child_entity = world.spawn_empty();
        let sibling_entity = world.spawn_empty();
        assert!(reserve_paint_requests(5));
        let parent = PaintRequestScope::new(&world);
        assert!(parent.request(parent_entity));
        assert!(parent.request(child_entity));
        let child = PaintRequestScope::new(&world);
        assert!(child.request(child_entity));
        child.commit();
        let sibling = PaintRequestScope::new(&world);
        assert!(sibling.request(sibling_entity));
        assert!(sibling.request(child_entity));
        drop(sibling);
        drop(parent);

        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(child_entity));
        assert!(!world.has::<VisualDirty>(parent_entity));
        assert!(!world.has::<VisualDirty>(sibling_entity));
    }

    #[test]
    fn signal_notifications_remain_ready_and_independent_from_paint_requests() {
        reset();
        let mut world = World::new();
        let entity = world.spawn_empty();
        let signal = Signal::new(0);
        let _subscription = signal.subscribe_visual_widget(world.id(), entity);
        assert!(reserve_paint_requests(2));
        let failed = PaintRequestScope::new(&world);
        assert!(failed.request(entity));
        signal.set(1);
        drop(failed);
        flush_signal_dirty(&mut world);
        assert!(world.remove::<VisualDirty>(entity).is_some());

        signal.set(2);
        let successful = PaintRequestScope::new(&world);
        assert!(successful.request(entity));
        flush_signal_dirty(&mut world);
        assert!(world.remove::<VisualDirty>(entity).is_some());
        successful.commit();
        flush_signal_dirty(&mut world);
        assert!(world.remove::<VisualDirty>(entity).is_some());

        let first = PaintRequestScope::new(&world);
        let second = PaintRequestScope::new(&world);
        assert!(first.request(entity));
        assert!(second.request(entity));
        signal.set(3);
        first.commit();
        second.commit();
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);
        flush_signal_dirty(&mut world);
        assert!(world.has::<VisualDirty>(entity));
    }

    #[test]
    fn scopes_preserve_world_and_entity_generation_boundaries() {
        reset();
        let mut first_world = World::new();
        let mut second_world = World::new();
        let first_entity = first_world.spawn_empty();
        let second_entity = second_world.spawn_empty();
        assert_eq!(first_entity, second_entity);
        assert!(reserve_paint_requests(2));
        let first = PaintRequestScope::new(&first_world);
        let second = PaintRequestScope::new(&second_world);
        assert!(first.request(first_entity));
        assert!(second.request(second_entity));
        first.commit();
        flush_signal_dirty(&mut second_world);
        assert!(!second_world.has::<VisualDirty>(second_entity));
        flush_signal_dirty(&mut first_world);
        assert!(first_world.remove::<VisualDirty>(first_entity).is_some());
        second.commit();
        flush_signal_dirty(&mut second_world);
        assert!(second_world.has::<VisualDirty>(second_entity));

        let retired = PaintRequestScope::new(&first_world);
        assert!(retired.request(first_entity));
        assert!(first_world.despawn(first_entity));
        let replacement = first_world.spawn_empty();
        assert_eq!(replacement.id, first_entity.id);
        assert_ne!(replacement.generation, first_entity.generation);
        retired.commit();
        flush_signal_dirty(&mut first_world);
        assert!(!first_world.has::<VisualDirty>(replacement));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn owner_cleanup_removes_ready_and_pending_requests_without_touching_other_worlds() {
        reset();
        let mut world = World::new();
        let mut other_world = World::new();
        let entity = world.spawn_empty();
        let other_entity = other_world.spawn_empty();
        assert!(reserve_paint_requests(3));
        let pending = PaintRequestScope::new(&world);
        assert!(pending.request(entity));
        let committed = PaintRequestScope::new(&world);
        assert!(committed.request(entity));
        committed.commit();
        let other = PaintRequestScope::new(&other_world);
        assert!(other.request(other_entity));
        other.commit();
        assert!(!has_graph_owner(world.id()));
        assert!(!has_graph_owner(other_world.id()));
        drop(world);
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), 1);
        pending.commit();
        flush_signal_dirty(&mut other_world);
        assert!(other_world.has::<VisualDirty>(other_entity));
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn capacity_exhaustion_rejects_requests_without_allocating_or_losing_entries() {
        reset();
        let world = World::new();
        assert!(reserve_paint_requests(1));
        let capacity = with_reactive(|r| r.dirty_visual_widgets.capacity());
        let scope = PaintRequestScope::new(&world);
        let allocations = crate::test_support::tracked_allocations(|| {
            for index in 0..capacity {
                assert!(scope.request(entity(index as u32)));
            }
            assert!(scope.request(entity(0)));
            assert!(!scope.request(entity(capacity as u32)));
        });
        assert_eq!(allocations, 0);
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), capacity);
        assert!(!reserve_paint_requests(usize::MAX));
        assert_eq!(with_reactive(|r| r.dirty_visual_widgets.len()), capacity);
        drop(scope);
        assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    }

    #[test]
    fn warmed_scopes_commit_cancel_and_flush_without_allocating() {
        reset();
        let mut world = World::new();
        let first_entity = world.spawn_empty();
        let second_entity = world.spawn_empty();
        drop(PaintRequestScope::new(&world));
        world.reserve_component_storage::<VisualDirty>(second_entity.id, 2);
        assert!(reserve_paint_requests(4));
        flush_signal_dirty(&mut world);
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                let parent = PaintRequestScope::new(&world);
                assert!(parent.request_on_change(first_entity));
                assert!(parent.request(second_entity));
                let child = PaintRequestScope::new(&world);
                assert!(child.request_on_change(second_entity));
                assert!(child.has_change_requests());
                assert!(child.request(second_entity));
                assert!(!child.has_change_requests());
                child.commit_output(true);
                drop(parent);
                flush_signal_dirty(&mut world);
                assert!(!world.has::<VisualDirty>(first_entity));
                assert!(world.remove::<VisualDirty>(second_entity).is_some());

                let unchanged = PaintRequestScope::new(&world);
                assert!(unchanged.request_on_change(first_entity));
                assert!(unchanged.request(second_entity));
                unchanged.commit_output(false);
                flush_signal_dirty(&mut world);
                assert!(!world.has::<VisualDirty>(first_entity));
                assert!(world.remove::<VisualDirty>(second_entity).is_some());
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn unwinding_a_scope_discards_only_its_unpublished_requests() {
        reset();
        let mut world = World::new();
        let failed_entity = world.spawn_empty();
        let successful_entity = world.spawn_empty();
        assert!(reserve_paint_requests(2));
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let failed = PaintRequestScope::new(&world);
            assert!(failed.request(failed_entity));
            let successful = PaintRequestScope::new(&world);
            assert!(successful.request(successful_entity));
            successful.commit();
            panic!("paint operation failed");
        }));
        assert!(failure.is_err());
        flush_signal_dirty(&mut world);
        assert!(!world.has::<VisualDirty>(failed_entity));
        assert!(world.has::<VisualDirty>(successful_entity));
    }
}

fn drain_effects() {
    loop {
        let batch: Vec<EffectId> = with_reactive(|r| {
            r.dirty_effects
                .drain(..)
                .map(|entry| {
                    if let Some(effect) = r.effects.get(&entry.id) {
                        effect.borrow().queued.set(false);
                    }
                    entry.id
                })
                .collect()
        });
        if batch.is_empty() {
            break;
        }
        for id in batch {
            run_effect(id);
        }
    }
}

#[test]
fn prepared_widget_effect_delays_execution_without_allocating() {
    reset();
    let mut world = World::default();
    let entity = world.spawn_empty();
    let signal = Signal::new(0_u8);
    let observed = signal.clone();
    let calls = Rc::new(core::cell::Cell::new(0_u8));
    let counted = Rc::clone(&calls);
    let mut prepared = None;
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            prepared = Some(widget_binding::prepare_widget_effect(
                entity,
                BindingRole::Ordinary,
                move || {
                    let _ = observed.get();
                    counted.set(counted.get() + 1);
                },
            ));
        }),
        0
    );
    assert_eq!(calls.get(), 0);
    prepared.unwrap().install(&mut world);
    assert_eq!(calls.get(), 1);
    signal.set(1);
    flush_signal_dirty(&mut world);
    assert_eq!(calls.get(), 2);
}

#[test]
fn static_widget_input_runs_once_without_retaining_an_effect() {
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let calls = Rc::new(core::cell::Cell::new(0));
    let counted = calls.clone();
    widget_binding::prepare_widget_effect(entity, BindingRole::Display, move || {
        counted.set(counted.get() + 1);
    })
    .install_once(&mut world);
    assert_eq!(calls.get(), 1);
    assert_eq!(effect_count(), 0);
    assert_eq!(Rc::strong_count(&calls), 1);
    flush_signal_dirty(&mut world);
    assert_eq!(calls.get(), 1);
}

#[test]
fn composition_inputs_register_only_dynamic_leaf_consumers() {
    use crate::ui::Prop;
    use crate::ui::widgets::Text;

    #[crate::compose]
    fn leaf(text: prop!(alloc::string::String)) -> Entity {
        crate::ui! {
            Text (text: ${ text })
        }
    }

    #[crate::compose]
    fn forwarded(text: prop!(alloc::string::String)) -> Entity {
        crate::ui! {
            Column () {
                leaf (text)
                leaf (text)
            }
        }
    }

    reset();
    let mut app = crate::app::App::headless(32, 32);
    let root = app.spawn_root().id();
    app.compose(root, forwarded("static"));
    assert_eq!(effect_count(), 0);
    let source = Signal::new(alloc::string::String::from("before"));
    let observed = source.clone();
    app.compose(root, forwarded(Prop::new(move || observed.get())));
    assert_eq!(effect_count(), 2);
    assert_eq!(with_reactive(|r| r.computeds.len()), 0);
    source.set(alloc::string::String::from("after"));
    flush_signal_dirty(&mut app.world);
    assert_eq!(effect_count(), 2);
}

#[test]
fn stale_explicit_view_binding_cannot_unsubscribe_a_reused_consumer_slot() {
    reset();
    let mut app = crate::app::App::headless(32, 32);
    let source = ModelSource::new();
    source.register_owner(app.world.id());
    let entity = app.world.spawn_empty();

    let first = source.subscribe_visual_widget(app.world.id(), entity);
    let stale_key = first.consumer;
    drop(first);
    let second = source.subscribe_visual_widget(app.world.id(), entity);
    assert_eq!(second.consumer.slot.slot, stale_key.slot.slot);
    assert_ne!(second.consumer.slot.generation, stale_key.slot.generation);

    drop(ViewSubscription {
        consumer: stale_key,
    });
    source.notify();
    flush_signal_dirty(&mut app.world);
    assert!(app.world.has::<VisualDirty>(entity));
}

#[test]
fn get_set_update_roundtrip() {
    let s = Signal::new(1i32);
    assert_eq!(s.get(), 1);
    s.set(5);
    assert_eq!(s.get(), 5);
    s.update(|n| *n += 3);
    assert_eq!(s.get(), 8);
}

#[test]
fn with_reads_without_clone() {
    let s = Signal::new(alloc::string::String::from("hi"));
    let len = s.with(|v| v.len());
    assert_eq!(len, 2);
}

#[test]
fn clone_shares_state() {
    let a = Signal::new(0i32);
    let b = a.clone();
    a.set(42);
    assert_eq!(b.get(), 42);
}

#[test]
fn read_in_scope_subscribes_and_set_enqueues() {
    reset();
    let s = Signal::new(0i32);
    let w = entity(1);
    with_scope(Subscriber::Widget(w), || {
        let _ = s.get();
    });
    s.set(1);
    with_reactive(|r| {
        assert_eq!(r.dirty_widgets.len(), 1);
        assert_eq!(r.dirty_widgets[0], (None, w));
        r.dirty_widgets.clear();
    });
}

#[test]
fn get_untracked_does_not_subscribe() {
    reset();
    let s = Signal::new(0i32);
    let w = entity(2);
    with_scope(Subscriber::Widget(w), || {
        let _ = s.get_untracked();
    });
    s.set(1);
    with_reactive(|r| {
        assert!(r.dirty_widgets.is_empty());
    });
}

#[test]
fn repeated_reads_dedup_subscriber() {
    reset();
    let s = Signal::new(0i32);
    let w = entity(3);
    with_scope(Subscriber::Widget(w), || {
        let _ = s.get();
        let _ = s.get();
        let _ = s.get();
    });
    assert_eq!(s.inner.borrow().subscribers.len(), 1);
    s.set(1);
    with_reactive(|r| {
        assert_eq!(r.dirty_widgets.len(), 1);
        r.dirty_widgets.clear();
    });
}

#[test]
fn repeated_notifications_enqueue_a_widget_once_until_consumed() {
    reset();
    let signal = Signal::new(0_u8);
    let widget = entity(4);
    with_scope(Subscriber::Widget(widget), || {
        let _ = signal.get();
    });
    signal.set(1);
    signal.set(2);
    with_reactive(|r| {
        assert_eq!(r.dirty_widgets.len(), 1);
        assert_eq!(r.dirty_widgets.pop_front(), Some((None, widget)));
    });
    signal.set(3);
    with_reactive(|r| {
        assert_eq!(r.dirty_widgets.len(), 1);
        r.dirty_widgets.clear();
    });
}

#[test]
fn read_outside_scope_no_subscribe() {
    reset();
    let s = Signal::new(0i32);
    let _ = s.get();
    s.set(1);
    with_reactive(|r| assert!(r.dirty_widgets.is_empty()));
}

#[test]
fn nested_scope_restores_previous() {
    let outer = entity(10);
    let inner = entity(11);
    let s_outer = Signal::new(0i32);
    let s_inner = Signal::new(0i32);
    with_scope(Subscriber::Widget(outer), || {
        let _ = s_outer.get();
        with_scope(Subscriber::Widget(inner), || {
            let _ = s_inner.get();
        });
        let _ = s_outer.get();
    });
    assert_eq!(
        s_outer.inner.borrow().subscribers,
        alloc::vec![OwnedSubscriber {
            world: None,
            subscriber: Subscriber::Widget(outer),
        }]
    );
    assert_eq!(
        s_inner.inner.borrow().subscribers,
        alloc::vec![OwnedSubscriber {
            world: None,
            subscriber: Subscriber::Widget(inner),
        }]
    );
}

#[test]
fn tracking_scope_restores_after_unwind() {
    reset();
    let outer = Subscriber::Widget(entity(10));
    let inner = Subscriber::Widget(entity(11));
    with_scope(outer, || {
        let failed = std::panic::catch_unwind(|| {
            with_scope(inner, || panic!("scope unwind"));
        });
        assert!(failed.is_err());
        assert_eq!(current_scope(), Some(outer));
    });
    assert_eq!(current_scope(), None);
}

#[test]
fn effect_runs_once_on_creation() {
    reset();
    let runs = Rc::new(RefCell::new(0));
    let r = Rc::clone(&runs);
    let _e = Effect::new(move || *r.borrow_mut() += 1);
    assert_eq!(*runs.borrow(), 1);
}

#[test]
fn panicking_initial_effect_run_unregisters_its_consumer() {
    reset();
    let signal = Signal::new(0u8);
    let source = signal.clone();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _effect = Effect::new(move || {
            let _ = source.get();
            panic!("initial evaluation failed");
        });
    }));
    assert!(failure.is_err());
    with_reactive(|r| assert!(r.effects.is_empty()));
    let source = signal.inner.borrow().graph_source;
    let mut subscribers = 0;
    with_reactive(|r| {
        r.graph
            .for_each_subscriber(source, |_, _| subscribers += 1)
            .unwrap();
    });
    assert_eq!(subscribers, 0);
    signal.set(1);
    with_reactive(|r| assert!(r.dirty_effects.is_empty()));
}

#[test]
fn effect_reruns_on_dependency_change() {
    reset();
    let s = Signal::new(0i32);
    let seen = Rc::new(RefCell::new(alloc::vec::Vec::<i32>::new()));
    let (sc, seenc) = (s.clone(), Rc::clone(&seen));
    let _e = Effect::new(move || seenc.borrow_mut().push(sc.get()));
    assert_eq!(*seen.borrow(), alloc::vec![0]);

    s.set(7);
    drain_effects();
    assert_eq!(*seen.borrow(), alloc::vec![0, 7]);
}

#[test]
fn effect_switches_signal_dependencies_without_retaining_old_edges() {
    reset();
    let choose_right = Signal::new(false);
    let left = Signal::new(1u8);
    let right = Signal::new(10u8);
    let runs = Rc::new(Cell::new(0u32));
    let (choose_read, left_read, right_read, run_count) = (
        choose_right.clone(),
        left.clone(),
        right.clone(),
        Rc::clone(&runs),
    );
    let _effect = Effect::new(move || {
        if choose_read.get() {
            let _ = right_read.get();
        } else {
            let _ = left_read.get();
        }
        run_count.set(run_count.get() + 1);
    });
    assert_eq!(runs.get(), 1);

    left.set(2);
    drain_effects();
    assert_eq!(runs.get(), 2);
    right.set(11);
    drain_effects();
    assert_eq!(runs.get(), 2);

    choose_right.set(true);
    drain_effects();
    assert_eq!(runs.get(), 3);
    left.set(3);
    drain_effects();
    assert_eq!(runs.get(), 3, "the old branch must be unlinked");
    right.set(12);
    drain_effects();
    assert_eq!(runs.get(), 4);
}

#[test]
fn ownerless_effect_switches_registered_model_sources_without_allocating() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let owner = world.id();
    let first = Rc::new(ModelSource::new());
    let second = Rc::new(ModelSource::new());
    first.register_owner(owner);
    second.register_owner(owner);
    let use_first = Signal::new(true);
    let runs = Rc::new(Cell::new(0u32));
    let (branch, first_read, second_read, run_count) = (
        use_first.clone(),
        Rc::clone(&first),
        Rc::clone(&second),
        Rc::clone(&runs),
    );
    let _effect = Effect::new(move || {
        if branch.get() {
            first_read.track();
        } else {
            second_read.track();
        }
        run_count.set(run_count.get() + 1);
    });
    assert_eq!(runs.get(), 1);

    let allocations = graph::tests::tracked_allocations(|| {
        use_first.set(false);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(
        allocations, 0,
        "registered branch switch must reuse graph storage"
    );
    assert_eq!(runs.get(), 2);
    first.notify();
    flush_signal_dirty(&mut world);
    assert_eq!(runs.get(), 2, "the old model source must be unlinked");
    second.notify();
    flush_signal_dirty(&mut world);
    assert_eq!(runs.get(), 3);
}

#[test]
fn ownerless_effect_first_model_read_reuses_structural_consumer_capacity() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let model = Rc::new(ModelSource::new());
    model.register_owner(world.id());
    let use_model = Signal::new(false);
    let runs = Rc::new(Cell::new(0u32));
    let (branch, model_read, run_count) = (use_model.clone(), Rc::clone(&model), Rc::clone(&runs));
    let _effect = Effect::new(move || {
        if branch.get() {
            model_read.track();
        }
        run_count.set(run_count.get() + 1);
    });
    assert_eq!(runs.get(), 1);

    let allocations = graph::tests::tracked_allocations(|| {
        use_model.set(true);
        flush_signal_dirty(&mut world);
    });
    assert_eq!(
        allocations, 0,
        "first model read must use reserved capacity"
    );
    assert_eq!(runs.get(), 2);
}

#[test]
fn unbound_effect_retains_headroom_when_owner_and_widget_arrive_later() {
    reset();
    let gate = Signal::new(false);
    let model = Rc::new(ModelSource::new());
    let runs = Rc::new(Cell::new(0u32));
    let (branch, observed, count) = (gate.clone(), Rc::clone(&model), Rc::clone(&runs));
    let _effect = Effect::new(move || {
        if branch.get() {
            observed.track();
        }
        count.set(count.get() + 1);
    });
    let mut world = World::new();
    let widget = world.spawn_empty();
    with_world_scope(&mut world, || {
        with_scope(Subscriber::Widget(widget), || ());
    });
    model.register_owner(world.id());

    assert_eq!(
        graph::tests::tracked_allocations(|| {
            gate.set(true);
            flush_signal_dirty(&mut world);
        }),
        0
    );
    assert_eq!(runs.get(), 2);
}

#[test]
fn ownerless_effect_cannot_subscribe_to_models_from_two_apps() {
    reset();
    let mut first_world = World::new();
    let mut second_world = World::new();
    drop(OwnerGuard::enter(&mut first_world));
    drop(OwnerGuard::enter(&mut second_world));
    let first = Rc::new(ModelSource::new());
    let second = Rc::new(ModelSource::new());
    first.register_owner(first_world.id());
    second.register_owner(second_world.id());
    let use_second = Signal::new(false);
    let (branch, first_read, second_read) =
        (use_second.clone(), Rc::clone(&first), Rc::clone(&second));
    let effect = Effect::new(move || {
        if branch.get() {
            second_read.track();
        } else {
            first_read.track();
        }
    });
    use_second.set(true);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        flush_signal_dirty(&mut first_world);
    }));
    assert!(failure.is_err());
    drop(effect);
}

#[test]
fn disposed_effect_stops_rerunning() {
    reset();
    let s = Signal::new(0i32);
    let runs = Rc::new(RefCell::new(0));
    let (sc, rc) = (s.clone(), Rc::clone(&runs));
    let e = Effect::new(move || {
        let _ = sc.get();
        *rc.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    e.dispose();
    s.set(1);
    drain_effects();
    assert_eq!(*runs.borrow(), 1, "disposed effect must not re-run");
}

#[test]
fn spawn_records_owner_entity() {
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let owned = with_world_scope(&mut world, || {
        Effect::spawn(|| {}, Some(entity), BindingRole::Ordinary)
    });
    assert_eq!(owned.owner(), Some(entity));
    let standalone = Effect::new(|| {});
    assert_eq!(standalone.owner(), None);
}

#[test]
fn effect_and_computed_slot_reuse_changes_generation() {
    reset();
    let effect = Effect::new(|| {});
    let first_effect = effect.id;
    drop(effect);
    let effect = Effect::new(|| {});
    assert_eq!(first_effect.0.slot, effect.id.0.slot);
    assert_ne!(first_effect.0.generation, effect.id.0.generation);
    let computed = Computed::new(|| 1u8);
    let first_computed = computed.id();
    drop(computed);
    let computed = Computed::new(|| 2u8);
    assert_eq!(first_computed.0.slot, computed.id().0.slot);
    assert_ne!(first_computed.0.generation, computed.id().0.generation);
}

#[test]
fn source_slot_reuse_rejects_stale_signal_identity() {
    reset();
    let signal = Signal::new(1u8);
    let first = signal.inner.borrow().id;
    drop(signal);
    let signal = Signal::new(2u8);
    let second = signal.inner.borrow().id;
    assert_eq!(first.0.slot, second.0.slot);
    assert_ne!(first.0.generation, second.0.generation);
    assert!(!with_reactive(|r| r.source_slots.is_live(first.0)));
    assert!(with_reactive(|r| r.source_slots.is_live(second.0)));
}

#[test]
fn disposing_effect_releases_captured_signal_after_registry_borrow() {
    reset();
    let signal = Signal::new(1u8);
    let captured = signal.clone();
    let effect = Effect::new(move || {
        let _ = captured.get();
    });
    drop(signal);
    effect.dispose();
    assert_eq!(effect_count(), 0);
}

#[test]
fn owned_effects_release_nested_members_while_the_parent_remains_live() {
    reset();
    let mut world = World::new();
    let parent = world.spawn_empty();
    let (_, owned) = with_world_scope(&mut world, || {
        with_owned_effects(|| {
            effect_with_role(parent, BindingRole::Maintenance, || {});
            let (_, nested) = with_owned_effects(|| {
                effect_with_role(parent, BindingRole::Maintenance, || {});
            });
            effect_with_role(parent, BindingRole::Maintenance, move || {
                let _ = &nested;
            });
        })
    });
    assert_eq!(effect_count(), 3);
    assert_eq!(crate::test_support::tracked_allocations(|| drop(owned)), 0);
    assert!(world.is_alive(parent));
    assert_eq!(effect_count(), 0);
    assert!(with_reactive(|runtime| runtime.captured_effects.is_none()));
}

#[test]
fn owned_effect_capture_unwinds_without_retiring_its_outer_capture() {
    reset();
    let mut world = World::new();
    let parent = world.spawn_empty();
    let (_, owned) = with_world_scope(&mut world, || {
        with_owned_effects(|| {
            effect_with_role(parent, BindingRole::Maintenance, || {});
            let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_owned_effects(|| {
                    effect_with_role(parent, BindingRole::Maintenance, || {});
                    panic!("failed member construction");
                });
            }));
            assert!(failed.is_err());
            effect_with_role(parent, BindingRole::Maintenance, || {});
        })
    });
    assert_eq!(effect_count(), 2);
    drop(owned);
    assert_eq!(effect_count(), 0);
    assert!(with_reactive(|runtime| runtime.captured_effects.is_none()));
}

#[test]
fn matching_entities_from_different_worlds_keep_distinct_effect_owners() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    assert_ne!(a.id(), b.id());
    let entity_a = a.spawn_empty();
    let entity_b = b.spawn_empty();
    assert_eq!(entity_a, entity_b);
    with_world_scope(&mut a, || effect_with_widget(entity_a, || {}));
    with_world_scope(&mut b, || effect_with_widget(entity_b, || {}));
    assert_eq!(effect_count(), 2);
    cleanup_effects_for(&a, entity_a);
    assert_eq!(effect_count(), 1);
    with_world_scope(&mut a, || effect_with_widget(entity_a, || {}));
    a.despawn(entity_a);
    flush_signal_dirty(&mut a);
    assert_eq!(effect_count(), 1);
    flush_signal_dirty(&mut b);
    assert_eq!(effect_count(), 1);
    b.despawn(entity_b);
    flush_signal_dirty(&mut b);
    assert_eq!(effect_count(), 0);
}

#[test]
fn widget_effect_retirement_releases_captures_without_allocating_or_borrowing_registry() {
    reset();
    let mut world = World::new();
    let widget = world.spawn_empty();
    let survivor = world.spawn_empty();
    let source = Signal::new(1u8);
    with_world_scope(&mut world, || {
        for _ in 0..8 {
            let captured = source.clone();
            effect_with_widget(widget, move || {
                let _ = captured.get();
            });
        }
        effect_with_widget(survivor, || {});
    });
    drop(source);
    assert_eq!(effect_count(), 9);

    assert_eq!(
        crate::test_support::tracked_allocations(|| cleanup_effects_for(&world, widget)),
        0,
    );
    assert_eq!(effect_count(), 1);
    cleanup_effects_for(&world, survivor);
    assert_eq!(effect_count(), 0);
}

#[test]
fn effect_setting_signal_during_flush_does_not_deadlock() {
    reset();
    let trigger = Signal::new(0i32);
    let target = Signal::new(0i32);
    let (tc, gc) = (trigger.clone(), target.clone());
    let _e = Effect::new(move || {
        let v = tc.get();
        if v > 0 {
            gc.set(v * 2);
        }
    });
    trigger.set(5);
    drain_effects();
    assert_eq!(target.get_untracked(), 10);
}

#[test]
fn flushing_effects_can_retire_queued_successors_without_consuming_other_worlds() {
    for foreign_queued in [false, true] {
        reset();
        let mut world = World::new();
        let mut other = World::new();
        let trigger = Signal::new(0u8);
        let victim = Rc::new(RefCell::new(None::<Effect>));
        let victim_runs = Rc::new(Cell::new(0));
        let other_runs = Rc::new(Cell::new(0));
        let source = trigger.clone();
        let retiring = Rc::clone(&victim);
        let _retirement = with_world_scope(&mut world, || {
            Effect::new(move || {
                if source.get() != 0
                    && let Some(effect) = retiring.borrow_mut().take()
                {
                    effect.dispose();
                }
            })
        });
        let source = trigger.clone();
        let runs = Rc::clone(&victim_runs);
        *victim.borrow_mut() = Some(with_world_scope(&mut world, || {
            Effect::new(move || {
                let _ = source.get();
                runs.set(runs.get() + 1);
            })
        }));
        let _foreign = foreign_queued.then(|| {
            let source = trigger.clone();
            let runs = Rc::clone(&other_runs);
            with_world_scope(&mut other, || {
                Effect::new(move || {
                    let _ = source.get();
                    runs.set(runs.get() + 1);
                })
            })
        });
        trigger.set(1);
        assert_eq!(
            crate::test_support::tracked_allocations(|| flush_signal_dirty(&mut world)),
            0
        );
        assert_eq!(victim_runs.get(), 1);
        assert_eq!(other_runs.get(), u32::from(foreign_queued));
        flush_signal_dirty(&mut other);
        assert_eq!(other_runs.get(), 2 * u32::from(foreign_queued));
    }
}

#[test]
fn subtree_wake_appends_only_matching_effects_in_registry_order() {
    reset();
    let mut world = World::new();
    let mut other = World::new();
    let root = world.spawn_empty();
    let child = world.spawn_empty();
    let outside = world.spawn_empty();
    world.insert(child, crate::ui::Parent(root));
    let (child_display, outside_display, root_display, ordinary) =
        with_world_scope(&mut world, || {
            (
                Effect::spawn(|| {}, Some(child), BindingRole::Display),
                Effect::spawn(|| {}, Some(outside), BindingRole::Display),
                Effect::spawn(|| {}, Some(root), BindingRole::Display),
                Effect::spawn(|| {}, Some(root), BindingRole::Ordinary),
            )
        });
    let foreign_root = other.spawn_empty();
    assert_eq!(foreign_root, root);
    let _foreign = with_world_scope(&mut other, || {
        Effect::spawn(|| {}, Some(foreign_root), BindingRole::Display)
    });
    enqueue_effect(outside_display.id);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            retained::wake_display(&world, root);
            retained::wake_display(&world, root);
            retained::wake(&world, root);
        }),
        0
    );
    with_reactive(|runtime| {
        let expected = [
            outside_display.id,
            child_display.id,
            root_display.id,
            ordinary.id,
        ];
        assert!(
            runtime
                .dirty_effects
                .iter()
                .map(|entry| entry.id)
                .eq(expected)
        );
    });
}

#[test]
fn computed_derives_and_recomputes() {
    reset();
    let n = Signal::new(2i32);
    let nc = n.clone();
    let doubled = Computed::new(move || nc.get() * 2);
    assert_eq!(doubled.get(), 4);
    n.set(5);
    assert_eq!(doubled.get(), 10);
}

#[test]
fn computed_is_lazy_until_get() {
    reset();
    let n = Signal::new(1i32);
    let calls = Rc::new(RefCell::new(0));
    let (nc, cc) = (n.clone(), Rc::clone(&calls));
    let c = Computed::new(move || {
        *cc.borrow_mut() += 1;
        nc.get()
    });
    assert_eq!(*calls.borrow(), 0, "no compute before first get");
    let _ = c.get();
    assert_eq!(*calls.borrow(), 1);
    let _ = c.get();
    assert_eq!(*calls.borrow(), 1, "clean re-get does not recompute");
    n.set(2);
    let _ = c.get();
    assert_eq!(*calls.borrow(), 2, "recompute only after a source change");
}

#[test]
fn computed_first_registered_model_branch_switch_does_not_allocate() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let first = Rc::new(ModelSource::new());
    let second = Rc::new(ModelSource::new());
    first.register_owner(world.id());
    second.register_owner(world.id());
    let use_first = Signal::new(true);
    let evaluations = Rc::new(Cell::new(0u32));
    let (branch, first_read, second_read, visits) = (
        use_first.clone(),
        Rc::clone(&first),
        Rc::clone(&second),
        Rc::clone(&evaluations),
    );
    let computed = with_world_scope(&mut world, || {
        Computed::new(move || {
            visits.set(visits.get() + 1);
            if branch.get() {
                first_read.track();
                1u8
            } else {
                second_read.track();
                2u8
            }
        })
    });
    assert_eq!(computed.get(), 1);
    assert_eq!(evaluations.get(), 1);

    let allocations = graph::tests::tracked_allocations(|| {
        use_first.set(false);
        assert_eq!(computed.get(), 2);
    });
    assert_eq!(allocations, 0);
    assert_eq!(evaluations.get(), 2);
    first.notify();
    assert_eq!(computed.get(), 2);
    assert_eq!(evaluations.get(), 2);
    second.notify();
    assert_eq!(computed.get(), 2);
    assert_eq!(evaluations.get(), 3);
}

#[test]
fn model_source_notification_dirties_computed_before_lazy_evaluation() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let source = Rc::new(ModelSource::new());
    source.register_owner(world.id());
    let evaluations = Rc::new(Cell::new(0u32));
    let (observed, visits) = (Rc::clone(&source), Rc::clone(&evaluations));
    let computed = with_world_scope(&mut world, || {
        Computed::new(move || {
            observed.track();
            visits.set(visits.get() + 1);
            1u8
        })
    });
    let read = computed.clone();
    let effect = with_world_scope(&mut world, || {
        Effect::new(move || {
            let _ = read.get();
        })
    });
    assert_eq!(evaluations.get(), 1);

    source.notify();
    assert!(computed.inner.borrow().dirty);
    assert_eq!(
        evaluations.get(),
        1,
        "notification must not evaluate user code"
    );
    assert!(with_reactive(|r| r
        .dirty_effects
        .iter()
        .any(|entry| entry.id == effect.id)));
    flush_signal_dirty(&mut world);
    assert_eq!(evaluations.get(), 2);
}

#[test]
fn dropping_model_source_invalidates_cached_computed_before_removing_edges() {
    reset();
    let mut world = World::new();
    let source = Rc::new(ModelSource::new());
    drop(OwnerGuard::enter(&mut world));
    source.register_owner(world.id());
    let weak = Rc::downgrade(&source);
    let computed = with_world_scope(&mut world, || {
        Computed::new(move || {
            weak.upgrade().expect("model source is gone").track();
            9u8
        })
    });
    assert_eq!(computed.get(), 9);

    drop(source);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| computed.get()));
    assert!(
        failure.is_err(),
        "removed source cannot leave a valid cache"
    );
}

#[test]
fn retired_model_source_reuses_storage_without_inheriting_subscribers() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let source = Rc::new(ModelSource::new());
    source.register_owner(world.id());
    let old_key = source.graph_source.get().unwrap();
    let observed = source.clone();
    let computed = with_world_scope(&mut world, || {
        Computed::new(move || {
            observed.track();
            9_u8
        })
    });
    let upstream = computed.clone();
    let downstream = with_world_scope(&mut world, || Computed::new(move || upstream.get()));
    assert_eq!(downstream.get(), 9);
    let old_view = source.subscribe_visual_widget(world.id(), entity(1));
    let runs = Rc::new(Cell::new(0));
    let (observed, count) = (source.clone(), runs.clone());
    let _effect = with_world_scope(&mut world, || {
        Effect::new(move || {
            observed.track();
            count.set(count.get() + 1);
        })
    });

    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            source.retire();
            source.retire();
            source.register_owner(world.id());
        }),
        0
    );
    let new_key = source.graph_source.get().unwrap();
    assert_ne!(old_key, new_key);
    assert_eq!(old_key.slot.slot, new_key.slot.slot);
    assert!(computed.inner.borrow().dirty);
    assert!(downstream.inner.borrow().dirty);
    assert!(with_reactive(|r| r.graph.publish(old_key)).is_err());
    source.notify();
    flush_signal_dirty(&mut world);
    assert_eq!(runs.get(), 1);
    assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));

    let new_view = source.subscribe_visual_widget(world.id(), entity(2));
    drop(old_view);
    source.notify();
    with_reactive(|r| {
        assert_eq!(r.dirty_visual_widgets.len(), 1);
        assert!(
            r.dirty_visual_widgets
                .contains(&VisualInvalidation::ready(Some(world.id()), entity(2),))
        );
    });
    drop(new_view);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..32 {
                source.retire();
                source.register_owner(world.id());
            }
        }),
        0
    );
}

#[test]
fn retired_unregistered_model_source_releases_fallback_subscriptions_once() {
    reset();
    let source = ModelSource::new();
    with_scope(Subscriber::VisualWidget(entity(1)), || source.track());
    assert_eq!(with_reactive(|r| r.model_visual_subscriptions), 1);
    let capacity = source.subscribers.borrow().capacity();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            source.retire();
            source.retire();
        }),
        0
    );
    assert_eq!(with_reactive(|r| r.model_visual_subscriptions), 0);
    assert_eq!(source.subscribers.borrow().capacity(), capacity);
    source.notify();
    assert!(with_reactive(|r| r.dirty_visual_widgets.is_empty()));
    with_scope(Subscriber::VisualWidget(entity(2)), || source.track());
    assert_eq!(with_reactive(|r| r.model_visual_subscriptions), 0);
    drop(source);
    assert_eq!(with_reactive(|r| r.model_visual_subscriptions), 0);
}

#[test]
fn retired_model_source_keeps_its_reserved_slot_until_drop() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let first = ModelSource::new();
    first.register_owner(world.id());
    let original = first.graph_source.get().unwrap();
    first.retire();
    let reserved = first.graph_source.get().unwrap();
    first.retire();
    assert_eq!(first.graph_source.get(), Some(reserved));
    let second = ModelSource::new();
    second.register_owner(world.id());
    assert_ne!(
        second.graph_source.get().unwrap().slot.slot,
        original.slot.slot
    );
    assert_eq!(
        crate::test_support::tracked_allocations(|| first.register_owner(world.id())),
        0
    );
    assert_eq!(first.graph_source.get(), Some(reserved));
    drop(first);
    let replacement = ModelSource::new();
    assert_eq!(
        crate::test_support::tracked_allocations(|| replacement.register_owner(world.id())),
        0
    );
    assert_eq!(
        replacement.graph_source.get().unwrap().slot.slot,
        original.slot.slot
    );
    assert!(with_reactive(|r| r.graph.publish(original)).is_err());
    assert!(with_reactive(|r| r.graph.publish(reserved)).is_err());
}

#[test]
fn model_source_rebinding_preflights_the_next_retirement_generation() {
    reset();
    let mut world = World::new();
    drop(OwnerGuard::enter(&mut world));
    let source = ModelSource::new();
    source.register_owner(world.id());
    let original = source.graph_source.get().unwrap();
    assert!(source.can_rebind(world.id()));
    let mut last = original;
    last.slot.generation = u32::MAX;
    source.graph_source.set(Some(last));
    assert!(!source.can_rebind(world.id()));
    source.lifecycle.set(ModelSourceLifecycle::Retired);
    assert!(source.can_rebind(world.id()));
    source.lifecycle.set(ModelSourceLifecycle::Exhausted);
    assert!(!source.can_rebind(world.id()));
    source.graph_source.set(Some(original));
    source.lifecycle.set(ModelSourceLifecycle::Active);
}

#[test]
fn computed_source_change_dirties_subscribing_widget() {
    reset();
    let n = Signal::new(0i32);
    let nc = n.clone();
    let c = Computed::new(move || nc.get() + 1);
    let w = entity(7);
    with_scope(Subscriber::Widget(w), || {
        let _ = c.get();
    });
    n.set(9);
    with_reactive(|r| {
        assert!(
            r.dirty_widgets.contains(&(None, w)),
            "source change cascades to widget via computed"
        );
        r.dirty_widgets.clear();
    });
}

#[test]
fn chained_computeds_propagate() {
    reset();
    let n = Signal::new(1i32);
    let nc = n.clone();
    let a = Computed::new(move || nc.get() + 1);
    let ac = a.clone();
    let b = Computed::new(move || ac.get() * 10);
    assert_eq!(b.get(), 20);
    n.set(4);
    assert_eq!(b.get(), 50, "change flows source -> a -> b");
}

fn effect_count() -> usize {
    with_reactive(|r| r.effects.len())
}

#[test]
fn reclamation_tracks_retirement_and_registration_without_rescanning_live_effects() {
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    with_world_scope(&mut world, || effect_with_widget(entity, || {}));
    flush_signal_dirty(&mut world);
    let live = with_reactive(|r| r.effects.values().next().unwrap().clone());
    {
        // A stable empty flush must not inspect the live effect at all.
        let _borrow = live.borrow_mut();
        assert_eq!(
            crate::test_support::tracked_allocations(|| flush_signal_dirty(&mut world)),
            0
        );
    }
    let before = world.retirement_revision();
    world.spawn_empty();
    world.insert(entity, 7_u8);
    assert_eq!(world.retirement_revision(), before);
    world.despawn(entity);
    assert_ne!(world.retirement_revision(), before);
    let retired = world.retirement_revision();
    assert!(!world.despawn(entity));
    assert_eq!(world.retirement_revision(), retired);
    flush_signal_dirty(&mut world);
    assert_eq!(effect_count(), 0);
    let replacement = world.spawn_empty();
    assert_eq!(replacement.id, entity.id);
    assert_ne!(replacement.generation, entity.generation);
    with_world_scope(&mut world, || {
        effect_with_widget(entity, || {});
        effect_with_widget(replacement, || {});
    });
    flush_signal_dirty(&mut world);
    assert_eq!(
        effect_count(),
        1,
        "new consumers of retired identities must be reclaimed"
    );
}

#[test]
fn reclamation_does_not_acknowledge_consumers_registered_by_drop() {
    struct RegisterOnDrop(Entity);
    impl Drop for RegisterOnDrop {
        fn drop(&mut self) {
            effect_with_widget(self.0, || {});
        }
    }
    reset();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let capture = RegisterOnDrop(entity);
    with_world_scope(&mut world, || {
        effect_with_widget(entity, move || {
            let _ = &capture;
        })
    });
    world.despawn(entity);
    flush_signal_dirty(&mut world);
    assert_eq!(
        effect_count(),
        1,
        "Drop registered another dead-owner effect after the scan"
    );
    flush_signal_dirty(&mut world);
    assert_eq!(effect_count(), 0);
}

#[test]
fn despawned_widget_effect_is_reclaimed_on_flush() {
    reset();
    let mut world = World::new();
    let e = world.spawn_empty();
    let s = Signal::new(0i32);
    let sc = s.clone();
    with_world_scope(&mut world, || {
        effect_with_widget(e, move || {
            let _ = sc.get();
        })
    });
    assert_eq!(effect_count(), 1);

    world.despawn(e);
    // quiet effect: its signal never fires again, only the sweep reclaims it
    flush_signal_dirty(&mut world);
    assert_eq!(effect_count(), 0, "sweep drops the dead-owner effect");
}

#[test]
fn live_widget_effect_survives_flush() {
    reset();
    let mut world = World::new();
    let e = world.spawn_empty();
    let s = Signal::new(0i32);
    let sc = s.clone();
    with_world_scope(&mut world, || {
        effect_with_widget(e, move || {
            let _ = sc.get();
        })
    });
    flush_signal_dirty(&mut world);
    assert_eq!(effect_count(), 1, "live owner keeps its effect");
}

#[test]
fn quiet_retirement_delivers_work_enqueued_by_dropped_captures() {
    struct PublishOnDrop(Signal<u8>, WorldId);
    impl Drop for PublishOnDrop {
        fn drop(&mut self) {
            assert_eq!(with_world(|world| world.id()), Some(self.1));
            self.0.set(7);
        }
    }
    reset();
    let mut world = World::new();
    let dead = world.spawn_empty();
    let source = Signal::new(0u8);
    let observed = Rc::new(Cell::new(0));
    let signal = source.clone();
    let result = observed.clone();
    let _survivor = with_world_scope(&mut world, || Effect::new(move || result.set(signal.get())));
    let release = PublishOnDrop(source.clone(), world.id());
    with_world_scope(&mut world, || {
        effect_with_widget(dead, move || {
            let _ = &release;
        });
    });
    assert!(with_reactive(|r| r.dirty_effects.is_empty()));
    world.despawn(dead);
    let mut outer = World::new();
    let outer_id = outer.id();
    with_world_scope(&mut outer, || {
        flush_signal_dirty(&mut world);
        assert_eq!(with_world(|world| world.id()), Some(outer_id));
    });
    assert_eq!(observed.get(), 7);
    assert_eq!(effect_count(), 1);
    assert_eq!(
        crate::test_support::tracked_allocations(|| flush_signal_dirty(&mut world)),
        0
    );
}

#[test]
fn empty_flush_prepares_the_owner_without_replacing_the_outer_window() {
    reset();
    let mut world = World::new();
    let mut outer = World::new();
    let outer_id = outer.id();
    with_world_scope(&mut outer, || {
        flush_signal_dirty(&mut world);
        assert_eq!(with_world(|world| world.id()), Some(outer_id));
        assert_eq!(
            crate::test_support::tracked_allocations(|| flush_signal_dirty(&mut world)),
            0
        );
    });
    assert!(has_graph_owner(world.id()));
    assert!(with_world(|_| ()).is_none());
    let source = Signal::new(0u8);
    let signal = source.clone();
    let observed = Rc::new(Cell::new(0));
    let result = observed.clone();
    let _effect = with_world_scope(&mut world, || Effect::new(move || result.set(signal.get())));
    source.set(9);
    flush_signal_dirty(&mut world);
    assert_eq!(observed.get(), 9);
    assert!(has_graph_owner(world.id()));
    assert!(with_world(|_| ()).is_none());
}

#[test]
fn dispose_unregisters_standalone_effect() {
    reset();
    let eff = Effect::new(|| {});
    assert_eq!(effect_count(), 1);
    eff.dispose();
    assert_eq!(effect_count(), 0);
}

#[test]
fn self_feeding_effect_terminates_within_cap() {
    reset();
    let mut world = World::new();
    let s = Signal::new(0i32);
    let sc = s.clone();
    core::mem::forget(Effect::new(move || {
        let v = sc.get();
        if v < 5 {
            sc.set(v + 1);
        }
    }));
    s.set(1);
    flush_signal_dirty(&mut world); // must return, not hang
}

#[test]
fn nested_world_scope_restores_outer_pointer() {
    reset();
    let mut outer = World::new();
    with_world_scope(&mut outer, || {
        let mut inner = World::new();
        with_world_scope(&mut inner, || {
            assert!(with_world(|_| ()).is_some(), "inner scope sees a world");
        });
        assert!(
            with_world(|_| ()).is_some(),
            "outer scope still reachable after inner drops",
        );
    });
    assert!(with_world(|_| ()).is_none(), "no world outside any scope");
}

#[test]
fn world_suppression_records_are_nested_and_owner_specific() {
    reset();
    let mut first = World::new();
    let first_id = first.id();
    let mut second = World::new();
    let second_id = second.id();
    with_world_scope(&mut first, || {
        let first_suppression = with_world(|world| WorldAccessGuard::suppress(world)).unwrap();
        let nested = WorldAccessGuard::enter(WorldAccess::Suppressed(first_id));
        let second_suppression = WorldAccessGuard::suppress(&second);
        assert!(with_world(|_| ()).is_none());
        drop(first_suppression);
        assert!(with_world(|_| ()).is_none());
        drop(nested);
        assert_eq!(with_world(|world| world.id()), Some(first_id));
        with_world_scope(&mut second, || {
            assert!(with_world(|_| ()).is_none());
        });
        drop(second_suppression);
        with_world_scope(&mut second, || {
            assert_eq!(with_world(|world| world.id()), Some(second_id));
        });
        assert_eq!(with_world(|world| world.id()), Some(first_id));
    });
}

#[test]
fn world_suppression_releases_its_record_on_unwind() {
    reset();
    let mut world = World::new();
    let owner = world.id();
    with_world_scope(&mut world, || {
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_world(|world| {
                let _suppression = WorldAccessGuard::suppress(world);
                assert!(with_world(|_| ()).is_none());
                panic!("suppression unwind");
            });
        }));
        assert!(failed.is_err());
        assert_eq!(with_world(|world| world.id()), Some(owner));
    });
}

#[test]
fn world_access_limit_is_shared_by_borrows_and_suppression() {
    reset();
    let mut world = World::new();
    let owner = world.id();
    let guards: [WorldAccessGuard; 8] =
        ::core::array::from_fn(|_| WorldAccessGuard::suppress(&world));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        WorldAccessGuard::suppress(&world)
    }));
    assert!(failed.is_err());
    assert!(with_reactive(|runtime| runtime
        .borrowed_worlds
        .iter()
        .all(Option::is_some)));
    let mut other = World::new();
    with_world_scope(&mut other, || {
        let failed = std::panic::catch_unwind(|| with_world(|_| ()));
        assert!(failed.is_err());
    });
    drop(guards);
    assert!(with_reactive(|runtime| runtime
        .borrowed_worlds
        .iter()
        .all(Option::is_none)));
    with_world_scope(&mut world, || {
        assert_eq!(with_world(|world| world.id()), Some(owner));
    });
}

#[test]
fn foreign_compose_owner_cannot_inherit_an_outer_world_window() {
    reset();
    let mut outer = World::new();
    let outer_id = outer.id();
    let mut inner = crate::app::App::headless(1, 1);
    let inner_id = inner.world.id();
    let root = inner.spawn_root().id();
    with_world_scope(&mut outer, || {
        assert_eq!(with_world(|world| world.id()), Some(outer_id));
        inner.compose(root, |cx| {
            assert_eq!(current_world_id(), Some(inner_id));
            assert!(model_writes_allowed());
            assert_eq!(with_world(|world| world.id()), None);
            with_world_scope(cx.world_mut(), || {
                assert_eq!(with_world(|world| world.id()), Some(inner_id));
            });
            assert_eq!(with_world(|world| world.id()), None);
        });
        assert_eq!(with_world(|world| world.id()), Some(outer_id));
        with_world(|_| {
            inner.compose(root, |_| {
                assert_eq!(with_world(|world| world.id()), None);
            });
            let reentrant = std::panic::catch_unwind(|| with_world(|_| ()));
            assert!(reentrant.is_err());
        });
    });
    assert_eq!(with_world(|world| world.id()), None);
}

#[test]
fn escaping_owner_scope_does_not_revive_an_ended_world_window() {
    reset();
    let mut outer = World::new();
    let mut inner = World::new();
    let root = inner.spawn_empty();
    let inner_ref = &mut inner;
    let scope = with_world_scope(&mut outer, move || crate::ui::UiScope::new(inner_ref, root));
    let moved_outer = Box::new(outer);
    assert!(with_reactive(|runtime| runtime.world_window.is_none()));
    drop(scope);
    assert!(with_reactive(|runtime| runtime.world_window.is_none()));
    drop(moved_outer);
}

#[test]
fn owner_window_validation_does_not_allocate_after_owner_preparation() {
    reset();
    let mut outer = World::new();
    let mut inner = World::new();
    drop(OwnerGuard::enter(&mut inner));
    let outer_id = outer.id();
    with_world_scope(&mut outer, || {
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..64 {
                {
                    let _owner = OwnerGuard::enter(&mut inner);
                    assert_eq!(with_world(|world| world.id()), None);
                }
                assert_eq!(with_world(|world| world.id()), Some(outer_id));
            }
        });
        assert_eq!(allocations, 0);
    });
}

#[test]
fn computed_world_access_matches_its_registration_owner() {
    reset();
    let ownerless = Computed::new(|| with_world(|world| world.id()));
    let mut world = World::new();
    let id = world.id();
    with_world_scope(&mut world, || {
        assert_eq!(ownerless.get(), None);
        let owned = Computed::new(|| with_world(|world| world.id()));
        assert_eq!(owned.get(), Some(id));
        assert_eq!(with_world(|world| world.id()), Some(id));
    });
}

#[test]
fn world_scope_restores_after_unwind() {
    reset();
    let mut outer = World::new();
    let mut inner = World::new();
    let outer_id = outer.id();
    with_world_scope(&mut outer, || {
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_world_scope(&mut inner, || panic!("world unwind"));
        }));
        assert!(failed.is_err());
        assert_eq!(current_world_id(), Some(outer_id));
    });
    assert_eq!(current_world_id(), None);
}

#[test]
fn explicit_world_reborrow_restores_access_and_preserves_suppression() {
    reset();
    let mut world = World::new();
    let owner = world.id();
    with_world_scope(&mut world, || {
        with_world(|world| {
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    for _ in 0..16 {
                        with_world_scope(world, || {
                            with_world(|world| {
                                with_world_scope(world, || {
                                    assert_eq!(with_world(|world| world.id()), Some(owner));
                                });
                            });
                        });
                    }
                }),
                0
            );
            let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_world_scope(world, || {
                    with_world(|_| panic!("reborrow unwind"));
                });
            }));
            assert!(failed.is_err());
            assert!(std::panic::catch_unwind(|| with_world(|_| ())).is_err());
            let suppression = WorldAccessGuard::suppress(world);
            with_world_scope(world, || {
                assert!(with_world(|_| ()).is_none());
            });
            drop(suppression);
            with_world_scope(world, || {
                assert_eq!(with_world(|world| world.id()), Some(owner));
                with_world(|_| {
                    assert!(std::panic::catch_unwind(|| with_world(|_| ())).is_err());
                });
            });
        });
        assert_eq!(with_world(|world| world.id()), Some(owner));
    });
    assert!(with_reactive(|runtime| runtime
        .borrowed_worlds
        .iter()
        .all(Option::is_none)));
}

#[test]
fn world_mutable_borrow_is_released_after_unwind() {
    reset();
    let mut world = World::new();
    let mut other = World::new();
    with_world_scope(&mut world, || {
        let failed = std::panic::catch_unwind(|| {
            with_world(|_| panic!("borrow unwind"));
        });
        assert!(failed.is_err());
        assert!(with_world(|_| ()).is_some());
        with_world(|_| {
            let reentrant = std::panic::catch_unwind(|| {
                with_world(|_| ());
            });
            assert!(reentrant.is_err());
            with_world_scope(&mut other, || {
                assert!(with_world(|_| ()).is_some());
            });
        });
    });
}

#[test]
fn shared_signal_keeps_same_numbered_widgets_in_their_worlds() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    let a_widget = a.spawn_empty();
    let b_widget = b.spawn_empty();
    assert_eq!(a_widget, b_widget);
    let signal = Signal::new(0);
    let _a_subscription = signal.subscribe_widget(a.id(), a_widget);
    let _b_subscription = signal.subscribe_widget(b.id(), b_widget);

    signal.set(1);
    flush_signal_dirty(&mut a);
    assert!(a.get::<Dirty>(a_widget).is_some());
    assert!(b.get::<Dirty>(b_widget).is_none());
    flush_signal_dirty(&mut b);
    assert!(b.get::<Dirty>(b_widget).is_some());
}

#[test]
fn owned_widget_scope_switches_signal_edges_without_allocating() {
    reset();
    let mut world = World::new();
    let widget = world.spawn_empty();
    let choose_right = Signal::new(false);
    let left = Signal::new(1u8);
    let right = Signal::new(10u8);
    with_world_scope(&mut world, || {
        with_scope(Subscriber::Widget(widget), || {
            if choose_right.get() {
                let _ = right.get();
            } else {
                let _ = left.get();
            }
        });
    });
    assert!(left.inner.borrow().subscribers.is_empty());

    choose_right.set(true);
    flush_signal_dirty(&mut world);
    world.remove::<Dirty>(widget);
    let allocations = graph::tests::tracked_allocations(|| {
        with_world_scope(&mut world, || {
            with_scope(Subscriber::Widget(widget), || {
                if choose_right.get() {
                    let _ = right.get();
                } else {
                    let _ = left.get();
                }
            });
        });
    });
    assert_eq!(allocations, 0, "reevaluation must reuse graph storage");

    left.set(2);
    flush_signal_dirty(&mut world);
    assert!(!world.has::<Dirty>(widget), "old edge must be removed");
    assert_eq!(graph::tests::tracked_allocations(|| right.set(11)), 0);
    flush_signal_dirty(&mut world);
    assert!(world.has::<Dirty>(widget));

    cleanup_effects_for(&world, widget);
    world.remove::<Dirty>(widget);
    right.set(12);
    flush_signal_dirty(&mut world);
    assert!(
        !world.has::<Dirty>(widget),
        "teardown removes graph consumer"
    );
}

#[test]
fn owned_visual_scope_isolates_model_sources_across_worlds() {
    reset();
    let mut first_world = World::new();
    let mut second_world = World::new();
    let first_widget = first_world.spawn_empty();
    let second_widget = second_world.spawn_empty();
    let first_owner = first_world.id();
    let second_owner = second_world.id();
    assert_eq!(first_widget, second_widget);
    let first = ModelSource::new();
    let second = ModelSource::new();
    with_world_scope(&mut first_world, || {
        first.register_owner(first_owner);
        with_scope(Subscriber::VisualWidget(first_widget), || first.track());
    });
    with_world_scope(&mut second_world, || {
        second.register_owner(second_owner);
        with_scope(Subscriber::VisualWidget(second_widget), || second.track());
    });
    assert!(first.subscribers.borrow().is_empty());
    assert!(second.subscribers.borrow().is_empty());

    assert_eq!(graph::tests::tracked_allocations(|| first.notify()), 0);
    flush_signal_dirty(&mut second_world);
    assert!(!second_world.has::<VisualDirty>(second_widget));
    flush_signal_dirty(&mut first_world);
    assert!(first_world.has::<VisualDirty>(first_widget));

    first_world.remove::<VisualDirty>(first_widget);
    let backup = ModelSource::new();
    backup.register_owner(first_owner);
    let allocations = graph::tests::tracked_allocations(|| {
        with_world_scope(&mut first_world, || {
            with_scope(Subscriber::VisualWidget(first_widget), || backup.track());
        });
    });
    assert_eq!(
        allocations, 0,
        "visual dependency switch must reuse storage"
    );
    first.notify();
    flush_signal_dirty(&mut first_world);
    assert!(!first_world.has::<VisualDirty>(first_widget));
    backup.notify();
    flush_signal_dirty(&mut first_world);
    assert!(first_world.has::<VisualDirty>(first_widget));

    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_world_scope(&mut second_world, || {
            with_scope(Subscriber::VisualWidget(second_widget), || first.track());
        });
    }));
    assert!(failure.is_err(), "cross-World model edge must be rejected");
}

#[test]
fn flushing_one_world_preserves_other_worlds_effects() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    let signal = Signal::new(0);
    let a_runs = Rc::new(RefCell::new(0));
    let b_runs = Rc::new(RefCell::new(0));
    let a_signal = signal.clone();
    let b_signal = signal.clone();
    let a_count = Rc::clone(&a_runs);
    let b_count = Rc::clone(&b_runs);
    let _a_effect = with_world_scope(&mut a, || {
        Effect::new(move || {
            let _ = a_signal.get();
            *a_count.borrow_mut() += 1;
        })
    });
    let _b_effect = with_world_scope(&mut b, || {
        Effect::new(move || {
            let _ = b_signal.get();
            *b_count.borrow_mut() += 1;
        })
    });
    assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (1, 1));

    signal.set(1);
    flush_signal_dirty(&mut a);
    assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (2, 1));
    flush_signal_dirty(&mut b);
    assert_eq!((*a_runs.borrow(), *b_runs.borrow()), (2, 2));
}

#[test]
fn shared_computed_notifies_widgets_in_each_world() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    let a_widget = a.spawn_empty();
    let b_widget = b.spawn_empty();
    let signal = Signal::new(1);
    let source = signal.clone();
    let computed = Computed::new(move || source.get() * 2);
    with_world_scope(&mut a, || {
        with_scope(Subscriber::Widget(a_widget), || {
            assert_eq!(computed.get(), 2)
        });
    });
    with_world_scope(&mut b, || {
        with_scope(Subscriber::Widget(b_widget), || {
            assert_eq!(computed.get(), 2)
        });
    });

    signal.set(2);
    flush_signal_dirty(&mut b);
    assert!(b.get::<Dirty>(b_widget).is_some());
    assert!(a.get::<Dirty>(a_widget).is_none());
    flush_signal_dirty(&mut a);
    assert!(a.get::<Dirty>(a_widget).is_some());
}

#[test]
fn shared_visual_signal_preserves_other_worlds_pending_visuals() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    let a_widget = a.spawn_empty();
    let b_widget = b.spawn_empty();
    let signal = Signal::new(0);
    let _a_subscription = signal.subscribe_visual_widget(a.id(), a_widget);
    let _b_subscription = signal.subscribe_visual_widget(b.id(), b_widget);

    signal.set(1);
    flush_signal_dirty(&mut a);
    assert!(a.get::<VisualDirty>(a_widget).is_some());
    assert!(b.get::<VisualDirty>(b_widget).is_none());
    flush_signal_dirty(&mut b);
    assert!(b.get::<VisualDirty>(b_widget).is_some());
}

#[test]
fn ownerless_effect_runs_on_the_first_world_flush() {
    reset();
    let mut a = World::new();
    let mut b = World::new();
    let signal = Signal::new(0);
    let source = signal.clone();
    let runs = Rc::new(RefCell::new(0));
    let count = Rc::clone(&runs);
    let _effect = Effect::new(move || {
        let _ = source.get();
        *count.borrow_mut() += 1;
    });
    signal.set(1);
    flush_signal_dirty(&mut b);
    assert_eq!(*runs.borrow(), 2);
    flush_signal_dirty(&mut a);
    assert_eq!(*runs.borrow(), 2);
    signal.set(2);
    flush_signal_dirty(&mut a);
    assert_eq!(*runs.borrow(), 3);
    let source = signal.inner.borrow().graph_source;
    let mut subscribers = 0;
    with_reactive(|r| {
        r.graph
            .for_each_subscriber(source, |_, subscriber| {
                assert!(matches!(subscriber, Subscriber::Effect(_)));
                subscribers += 1;
            })
            .unwrap();
    });
    assert_eq!(subscribers, 1);
}

#[test]
fn dropped_world_releases_owner_graph_and_effect_edges() {
    reset();
    let mut world = World::new();
    let world_id = world.id();
    let signal = Signal::new(0u8);
    let source = signal.clone();
    let runs = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&runs);
    let effect = with_world_scope(&mut world, || {
        Effect::new(move || {
            let _ = source.get();
            counter.set(counter.get() + 1);
        })
    });
    assert_eq!(runs.get(), 1);
    assert!(with_reactive(|r| r.graph.owner_key(world_id).is_ok()));

    drop(world);
    assert!(with_reactive(|r| r.graph.owner_key(world_id).is_err()));
    assert_eq!(effect_count(), 0);
    let mut next_world = World::new();
    with_world_scope(&mut next_world, || ());
    signal.set(1);
    assert_eq!(runs.get(), 1);
    drop(effect);
}
use crate::ui::bindings::widget_binding;
