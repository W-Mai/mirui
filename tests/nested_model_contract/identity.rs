use super::*;

#[test]
fn same_value_reset_invalidates_descendant_watches_without_changing_identity() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    let right = pair.right();
    let observed = left.clone();
    let (_effect, runs) = count_reads(move || {
        let _ = observed.value();
    });
    workspace.reset_pair();
    flush_signal_dirty(&mut app.world);
    assert_eq!(runs.get(), 1);
    assert_eq!(
        (
            pair.summary_revision(),
            left.visual_revision(),
            left.save_revision(),
            right.visual_revision(),
            right.save_revision()
        ),
        (1, 1, 1, 1, 1)
    );
    left.add(2);
    assert_eq!(workspace.total(), 113);
    pair.swap_round_trip();
    assert_eq!((left.visual_revision(), right.visual_revision()), (2, 1));
    assert_eq!(pair.summary_revision(), 1);
}

#[test]
fn rejected_replacement_clears_its_marker_without_delayed_watch_publication() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    assert_eq!(pair.rejected_reset(), Err(Rejected));
    pair.idle();
    assert_eq!(
        (
            left.visual_revision(),
            left.save_revision(),
            pair.summary_revision()
        ),
        (0, 0, 0)
    );
    left.add(1);
    assert_eq!(left.value(), 2);
    assert_eq!((left.visual_revision(), left.save_revision()), (1, 0));
}

#[system]
fn advance(counter: model!(Counter)) {
    counter.add(1);
}

#[test]
fn child_owner_and_registration_lifetime_follow_the_parent() {
    let mut owner = App::headless(16, 16);
    let workspace = owner.add_model(Workspace::new());
    let child = workspace.pair().left();
    let mut foreign = App::headless(16, 16);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            foreign.add_system(advance::system(child.clone()));
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(
            || foreign.on_effect(&child, |_note: Note| {})
        ))
        .is_err()
    );
    let captured = workspace.clone();
    owner
        .on_effect(&child, move |_note: Note| {
            let _ = captured.total();
        })
        .unwrap();
    owner.add_system(advance::system(child.clone()));
    assert!(!owner.tick());
    assert_eq!(child.value(), 2);
    drop(owner);
    assert!(catch_unwind(AssertUnwindSafe(|| child.value())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| workspace.total())).is_err());
}

#[test]
fn owned_construction_and_warm_tree_operations_do_not_allocate() {
    assert_eq!(
        tracked_allocations(|| {
            drop(Workspace::new());
        }),
        0
    );
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let child = workspace.pair().left();
    let observed = workspace.clone();
    let (_effect, _) = count_reads(move || {
        let _ = observed.total();
    });
    let count = Rc::new(Cell::new(0_u16));
    let delivered = Rc::clone(&count);
    app.on_effect(&child, move |_note: Note| {
        delivered.set(delivered.get() + 1)
    })
    .unwrap();
    workspace.emit_reverse();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..32 {
                let pair = workspace.pair();
                let child = pair.left().clone();
                child.add(1);
                pair.add_both();
                pair.reset_left(1);
                pair.swap_pending();
                workspace.reset_pair();
                workspace.emit_reverse();
                let _ = child.value();
                flush_signal_dirty(&mut app.world);
            }
        }),
        0
    );
    assert_eq!(workspace.total(), 117);
}
