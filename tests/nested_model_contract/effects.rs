use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Delivery {
    route: u8,
    audit: bool,
    value: u16,
    total: u16,
}

#[derive(Default)]
struct Trace {
    entries: [Option<Delivery>; 16],
    len: usize,
}

impl Trace {
    fn record(&mut self, entry: Delivery) {
        self.entries[self.len] = Some(entry);
        self.len += 1;
    }
}

macro_rules! record_route {
    ($app:expr, $handle:expr, $root:expr, $trace:expr, $route:expr, $event:ident, $audit:expr) => {{
        let root = $root.clone();
        let trace = Rc::clone(&$trace);
        $app.on_effect(&$handle, move |event: $event| {
            trace.borrow_mut().record(Delivery {
                route: $route,
                audit: $audit,
                value: event.0,
                total: root.total(),
            });
        })
        .unwrap();
    }};
}

#[test]
fn effects_read_final_tree_in_declared_depth_first_order() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let trace = Rc::new(RefCell::new(Trace::default()));
    record_route!(app, workspace, workspace, trace, 0, Note, false);
    record_route!(app, pair, workspace, trace, 1, Note, false);
    for (route, child) in [(2, pair.left()), (3, pair.right()), (4, workspace.spare())] {
        record_route!(app, child, workspace, trace, route, Note, false);
        record_route!(app, child, workspace, trace, route, Audit, true);
    }

    assert_eq!(tracked_allocations(|| workspace.emit_reverse()), 0);
    let trace = trace.borrow();
    let expected = [
        (0, false, 200),
        (1, false, 20),
        (2, false, 2),
        (2, false, 3),
        (2, true, 2),
        (3, false, 12),
        (3, false, 13),
        (3, true, 12),
        (4, false, 103),
        (4, false, 104),
        (4, true, 103),
    ];
    assert_eq!(trace.len, expected.len());
    for (actual, (route, audit, value)) in trace.entries.iter().zip(expected) {
        assert_eq!(
            *actual,
            Some(Delivery {
                route,
                audit,
                value,
                total: 117
            })
        );
    }
}

#[test]
fn external_error_keeps_data_without_notifying_or_replaying_effects() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    let right = pair.right();
    let read = workspace.clone();
    let (_observer, runs) = count_reads(move || {
        let _ = read.total();
    });
    let trace = Rc::new(RefCell::new(Trace::default()));
    record_route!(app, left, workspace, trace, 0, Note, false);
    record_route!(app, right, workspace, trace, 1, Note, false);

    assert_eq!(pair.reject_after_success(), Err(Rejected));
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (left.value(), right.value(), workspace.total()),
        (2, 12, 114)
    );
    assert_eq!(
        (
            left.visual_revision(),
            right.visual_revision(),
            pair.summary_revision()
        ),
        (0, 0, 0)
    );
    assert_eq!(runs.get(), 1);
    assert_eq!(trace.borrow().len, 0);
    pair.idle();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (left.visual_revision(), right.visual_revision(), runs.get()),
        (0, 0, 1)
    );
    assert_eq!(trace.borrow().len, 0);

    assert_eq!(left.reject(3), Err(Rejected));
    assert_eq!(left.value(), 3);
    assert_eq!(left.visual_revision(), 0);
    assert_eq!(trace.borrow().len, 0);
}

#[test]
fn handled_inner_errors_use_outer_result_without_independent_savepoints() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    let right = pair.right();
    let trace = Rc::new(RefCell::new(Trace::default()));
    record_route!(app, left, workspace, trace, 0, Note, false);
    record_route!(app, right, workspace, trace, 1, Note, false);
    assert_eq!(pair.handle_child_error(), Ok(ParentChange::None));
    assert_eq!(
        (
            left.visual_revision(),
            right.visual_revision(),
            pair.summary_revision()
        ),
        (0, 1, 0)
    );
    assert_eq!(trace.borrow().len, 4);
    assert_eq!(workspace.total(), 114);

    trace.replace(Trace::default());
    assert_eq!(workspace.handle_pair_error(), Ok(()));
    assert_eq!(
        (
            left.visual_revision(),
            right.visual_revision(),
            pair.summary_revision()
        ),
        (0, 2, 0)
    );
    assert_eq!(trace.borrow().len, 4);
}

#[test]
fn reset_discards_replaced_effects_and_swap_routes_moved_data_at_final_field() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    let right = pair.right();
    let trace = Rc::new(RefCell::new(Trace::default()));
    record_route!(app, left, workspace, trace, 0, Note, false);
    record_route!(app, right, workspace, trace, 1, Note, false);
    pair.replace_pending();
    assert_eq!(left.value(), 1);
    assert_eq!(trace.borrow().len, 0);
    assert_eq!((left.visual_revision(), left.save_revision()), (1, 1));

    pair.swap_pending();
    assert_eq!((left.value(), right.value()), (30, 3));
    assert_eq!(
        (
            left.visual_revision(),
            left.save_revision(),
            right.visual_revision(),
            right.save_revision()
        ),
        (2, 2, 1, 1)
    );
    let trace = trace.borrow();
    assert_eq!(trace.len, 4);
    assert_eq!(
        trace.entries[0],
        Some(Delivery {
            route: 0,
            audit: false,
            value: 30,
            total: 133
        })
    );
    assert_eq!(
        trace.entries[2],
        Some(Delivery {
            route: 1,
            audit: false,
            value: 3,
            total: 133
        })
    );
}

#[test]
fn effect_consumers_can_read_the_tree_but_cannot_reenter_writes() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let left = workspace.pair().left();
    let root = workspace.clone();
    app.on_effect(&left, move |_note: Note| {
        assert_eq!(root.total(), 112);
        root.spare().add(1);
    })
    .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| left.emit(2))).is_err());
    assert_eq!((left.value(), workspace.spare().value()), (2, 100));
    left.add(1);
    assert_eq!(left.value(), 3);
}
