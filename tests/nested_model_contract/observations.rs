use super::*;

#[test]
fn child_commands_publish_parent_derived_and_only_changed_siblings() {
    let mut app = App::headless(16, 16);
    let workspace = app.add_model(Workspace::new());
    let pair = workspace.pair();
    let left = pair.left();
    let right = pair.right();
    let root_read = workspace.clone();
    let pair_read = pair.clone();
    let left_read = left.clone();
    let right_read = right.clone();
    let (_root_effect, root_runs) = count_reads(move || {
        let _ = root_read.total();
    });
    let (_pair_effect, pair_runs) = count_reads(move || {
        let _ = pair_read.total();
    });
    let (_left_effect, left_runs) = count_reads(move || {
        let _ = left_read.value();
    });
    let (_right_effect, right_runs) = count_reads(move || {
        let _ = right_read.value();
    });

    left.add(1);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (left.value(), right.value(), pair.total(), workspace.total()),
        (2, 10, 12, 112)
    );
    assert_eq!(
        (
            root_runs.get(),
            pair_runs.get(),
            left_runs.get(),
            right_runs.get()
        ),
        (2, 2, 2, 1)
    );

    pair.add_both();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (
            root_runs.get(),
            pair_runs.get(),
            left_runs.get(),
            right_runs.get()
        ),
        (3, 3, 3, 2)
    );
    assert_eq!(
        (
            left.visual_revision(),
            right.visual_revision(),
            pair.summary_revision()
        ),
        (2, 1, 1)
    );

    pair.paint_left(99);
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        (
            left.visual_revision(),
            right.visual_revision(),
            pair.summary_revision()
        ),
        (3, 1, 1)
    );
    assert_eq!(
        (
            root_runs.get(),
            pair_runs.get(),
            left_runs.get(),
            right_runs.get()
        ),
        (3, 3, 3, 2)
    );
    assert_eq!(left.save_revision(), 0);
}

#[component]
struct RootTile {
    workspace: model!(Workspace),
}

#[component]
struct ChildTile {
    counter: model!(Counter),
}

struct PaintCounts(Rc<Cell<[usize; 2]>>);

#[view(component = RootTile)]
fn root_view(
    workspace: model!(component.workspace, watch(total)),
    counts: res!(PaintCounts),
    paint: paint!(),
) {
    let mut count = counts.0.get();
    count[0] += 1;
    counts.0.set(count);
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color: Color::rgb(workspace.total() as u8, 0, 0),
        radius: Fixed::ZERO,
        opa: 255,
    });
    paint.set_background_handled(true);
}

#[view(component = ChildTile)]
fn child_view(
    counter: model!(component.counter, watch(value, visual_revision)),
    counts: res!(PaintCounts),
    paint: paint!(),
) {
    let mut count = counts.0.get();
    count[1] += 1;
    counts.0.set(count);
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color: Color::rgb(counter.value as u8, counter.pixels() as u8, 0),
        radius: Fixed::ZERO,
        opa: 255,
    });
    paint.set_background_handled(true);
}

#[compose]
fn tiles(workspace: model!(Workspace), counter: model!(Counter)) {
    ui! {
        Column () {
            View (width: 8, height: 8) [
                RootTile { workspace },
            ]
            View (width: 8, height: 8) [
                ChildTile { counter },
            ]
        }
    };
}

#[test]
fn parent_and_child_typed_view_watches_follow_real_dirty_pixels() {
    let pixels = Rc::new(Cell::new([[0_u8; 4]; 2]));
    let output = Rc::clone(&pixels);
    let backend = FramebufSurface::new(8, 16, move |buffer, _| {
        output.set(core::array::from_fn(|slot| {
            let offset = ((slot * 8 + 4) * 8 + 4) * 4;
            buffer[offset..offset + 4].try_into().unwrap()
        }));
    });
    let mut app = App::new(backend);
    app.with_default_widgets();
    let counts = Rc::new(Cell::new([0; 2]));
    app.add_resource(PaintCounts(Rc::clone(&counts)));
    app.with_widgets([root_view::view(), child_view::view()]);
    let workspace = app.add_model(Workspace::new());
    let child = workspace.pair().left();
    let root = app.spawn_root().id();
    app.compose(root, tiles(workspace.clone(), child.clone()));
    assert!(!app.tick());
    assert_eq!(pixels.get(), [[111, 0, 0, 255], [1, 1, 0, 255]]);
    let baseline = counts.get();

    child.add(1);
    assert!(!app.tick());
    assert_eq!(pixels.get(), [[112, 0, 0, 255], [2, 1, 0, 255]]);
    assert_eq!(counts.get(), baseline.map(|value| value + 1));
    let baseline = counts.get();
    child.idle();
    assert!(!app.tick());
    assert_eq!(counts.get(), baseline);

    child.paint(42);
    assert!(!app.tick());
    assert_eq!(pixels.get()[1], [2, 42, 0, 255]);
    assert!(counts.get()[1] > baseline[1]);

    workspace.reset_pair();
    assert!(!app.tick());
    assert_eq!(pixels.get(), [[111, 0, 0, 255], [1, 1, 0, 255]]);
    child.add(1);
    assert!(!app.tick());
    assert_eq!(pixels.get()[0], [112, 0, 0, 255]);
}
