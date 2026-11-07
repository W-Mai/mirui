use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;

#[compose]
fn position(position: prop!(ScrollOffset)) -> Entity {
    ui! {
        Column (scroll_offset: ${ position }, clip_children: true) [
            ScrollOffset::default(),
            ScrollConfig::default(),
        ] {
            View (height: 200, shrink: 0.0)
        }
    }
}

#[compose]
fn forwarded(offset: prop!(ScrollOffset)) -> Entity {
    ui! {
        position (offset)
    }
}

#[test]
fn forwarded_scroll_positions_update_only_visual_state_and_keep_fractional_axes() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let source = Signal::new(ScrollOffset {
        x: Fixed::from_ratio(1, 2),
        y: 12.into(),
    });
    let read = source.clone();
    let node = app.compose(root, forwarded(Prop::new(move || read.get())));
    assert_eq!(app.world.get::<ScrollOffset>(node), Some(&source.get()));
    app.render().unwrap();
    let next = ScrollOffset {
        x: Fixed::from_ratio(-3, 2),
        y: Fixed::from_ratio(37, 2),
    };
    source.set(next);
    flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.get::<ScrollOffset>(node), Some(&next));
    assert!(!app.world.has::<mirui::ui::dirty::Dirty>(node));
    source.set(next);
    flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.get::<ScrollOffset>(node), Some(&next));
}

#[compose]
fn initial() -> Entity {
    ui! {
        Row (
            scroll_offset: ScrollOffset {
                x: 9.into(),
                y: 3.into(),
            }
        ) [
            ScrollOffset::default(),
            ScrollConfig::default(),
        ]
    }
}

#[test]
fn static_scroll_positions_apply_after_component_attachment() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let node = app.compose(root, initial());
    assert_eq!(
        app.world.get::<ScrollOffset>(node),
        Some(&ScrollOffset {
            x: 9.into(),
            y: 3.into()
        })
    );
}

#[compose]
fn without_scroll() -> Entity {
    ui! {
        View (scroll_offset: ScrollOffset::default())
    }
}

#[test]
fn a_position_does_not_install_scroll_capabilities() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let node = app.compose(root, without_scroll());
    assert!(!app.world.has::<ScrollOffset>(node));
    assert!(!app.world.has::<ScrollConfig>(node));
}

#[compose]
fn optional_reader(shown: Signal<bool>, offset: prop!(ScrollOffset)) -> Entity {
    ui! {
        Column () {
            if ${ shown.get() } {
                position (offset)
            }
        }
    }
}

#[test]
fn hidden_scroll_bindings_resume_on_the_same_node_with_the_latest_position() {
    use std::{cell::Cell, rc::Rc};
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let shown = Signal::new(true);
    let offset = Signal::new(ScrollOffset {
        x: 0.into(),
        y: 5.into(),
    });
    let source = offset.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    app.compose(
        root,
        optional_reader(
            shown.clone(),
            Prop::new(move || {
                count.set(count.get() + 1);
                source.get()
            }),
        ),
    );
    let node = app.world.query::<ScrollOffset>().iter().next().unwrap().0;
    shown.set(false);
    flush_signal_dirty(&mut app.world);
    assert!(app.world.is_alive(node));
    let before = calls.get();
    offset.set(ScrollOffset {
        x: 0.into(),
        y: 19.into(),
    });
    flush_signal_dirty(&mut app.world);
    assert_eq!(calls.get(), before);
    shown.set(true);
    flush_signal_dirty(&mut app.world);
    let (replacement, actual) = app
        .world
        .query::<ScrollOffset>()
        .iter()
        .next()
        .map(|(entity, offset)| (entity, *offset))
        .unwrap();
    assert_eq!(replacement, node);
    assert_eq!(actual, offset.get());
}

#[test]
fn retired_scroll_bindings_do_not_write_to_replacements() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let offset = Signal::new(ScrollOffset {
        x: 0.into(),
        y: 5.into(),
    });
    let read = offset.clone();
    let removed = app.compose(root, position(Prop::new(move || read.get())));
    app.world.despawn(removed);
    flush_signal_dirty(&mut app.world);
    let replacement = app.compose(root, initial());
    offset.set(ScrollOffset {
        x: 30.into(),
        y: 40.into(),
    });
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<ScrollOffset>(replacement),
        Some(&ScrollOffset {
            x: 9.into(),
            y: 3.into()
        })
    );
}
