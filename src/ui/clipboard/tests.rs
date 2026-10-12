use super::*;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::widgets::{TextEditing, TextInput, TextSelection};
use alloc::{string::String, vec::Vec};

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

#[derive(Default)]
struct Results(Vec<ClipboardFinished>);

#[crate::event]
fn finished(event: event!(ClipboardFinished), results: res!(mut Results)) {
    results.0.push(*event);
}

fn fixture() -> (TestApp, Entity) {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    app.add_resource(Results::default());
    let root = app.spawn_root().id();
    let editor = TextEditing::default().with_text("A中🙂Z").unwrap();
    let node = app.compose(root, |cx| {
        ui! {
            TextInput () [editor] on ClipboardFinished(finished::callback())
        }
    });
    (app, node)
}

fn select(app: &mut TestApp, node: Entity, anchor: usize, cursor: usize) {
    app.world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection { anchor, cursor })
        .unwrap();
}

fn flush(app: &mut TestApp) {
    crate::ui::delivery::flush(&mut app.world).unwrap();
}

#[test]
fn reversed_utf8_selection_is_owned_and_survives_source_changes() {
    let (mut app, node) = fixture();
    select(&mut app, node, 8, 1);
    copy(&mut app.world, node);
    let write = take_write(&mut app.world).unwrap();
    assert_eq!(write.data().plain_text(), Ok("中🙂"));
    app.world.insert(node, TextEditing::default());
    assert_eq!(write.data().plain_text(), Ok("中🙂"));
    complete(&mut app.world, &write.complete(Ok(())));
    flush(&mut app);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Ok(ClipboardOutcome::Copied)
    );
}

#[test]
fn empty_selection_preserves_clipboard_and_default_surface_reports_unsupported() {
    let (mut app, node) = fixture();
    copy(&mut app.world, node);
    assert!(take_write(&mut app.world).is_none());
    flush(&mut app);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Ok(ClipboardOutcome::EmptySelection)
    );
    select(&mut app, node, 0, 1);
    copy(&mut app.world, node);
    app.poll_event();
    flush(&mut app);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[1].result,
        Err(ClipboardError::Unsupported)
    );
    assert!(take_write(&mut app.world).is_none());
}

#[test]
fn acknowledgments_are_ordered_single_use_and_owned_by_the_issuing_app() {
    let (mut app, node) = fixture();
    select(&mut app, node, 0, 1);
    copy(&mut app.world, node);
    let first = take_write(&mut app.world).unwrap();
    select(&mut app, node, 1, 4);
    copy(&mut app.world, node);
    assert!(take_write(&mut app.world).is_none());
    let failed = first.complete(Err(ClipboardError::PermissionDenied));
    let (mut other, other_node) = fixture();
    select(&mut other, other_node, 0, 1);
    copy(&mut other.world, other_node);
    let other_write = take_write(&mut other.world).unwrap();
    complete(&mut other.world, &failed);
    assert!(other.world.resource::<Service>().unwrap().running.is_some());
    complete(&mut app.world, &failed);
    complete(&mut app.world, &failed);
    let second = take_write(&mut app.world).unwrap();
    assert_eq!(second.data().plain_text(), Ok("中"));
    complete(&mut app.world, &failed);
    assert!(app.world.resource::<Service>().unwrap().running.is_some());
    complete(&mut app.world, &second.complete(Ok(())));
    flush(&mut app);
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].result, Err(ClipboardError::PermissionDenied));
    assert_eq!(results[1].result, Ok(ClipboardOutcome::Copied));
    complete(&mut other.world, &other_write.complete(Ok(())));
}

#[test]
fn commands_snapshot_the_selection_at_their_own_execution_position() {
    let (mut app, node) = fixture();
    let root = app.root.unwrap();
    let button = app.compose(root, |cx| {
        ui! {
            View () on Tap {
                commands!().clipboard().copy(node);
                commands!().selection().select_all(node);
                commands!().clipboard().copy(node);
            }
        }
    });
    crate::ui::delivery::dispatch_gesture(
        &mut app.world,
        &GestureEvent::Tap {
            target: button,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
        0,
    )
    .unwrap();
    let write = take_write(&mut app.world).unwrap();
    assert_eq!(write.data().plain_text(), Ok("A中🙂Z"));
    complete(&mut app.world, &write.complete(Ok(())));
    flush(&mut app);
    let results = &app.world.resource::<Results>().unwrap().0;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].result, Ok(ClipboardOutcome::EmptySelection));
    assert_eq!(results[1].result, Ok(ClipboardOutcome::Copied));
}

#[crate::event]
fn mutating_copy(
    node: entity!(),
    _event: event!(CopySelection),
    editors: query!(mut TextEditing),
) -> Result<ClipboardData, ClipboardError> {
    editors
        .get(node)
        .unwrap()
        .set_selection(TextSelection::default())
        .unwrap();
    ClipboardData::text("stale")
}

#[test]
fn mutations_during_snapshot_capture_never_reach_the_platform() {
    let (mut app, node) = fixture();
    select(&mut app, node, 0, 1);
    request::install(
        &mut app.world,
        node,
        mutating_copy::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    copy(&mut app.world, node);
    assert!(take_write(&mut app.world).is_none());
    flush(&mut app);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Err(ClipboardError::Unavailable)
    );
}

#[test]
fn in_flight_capacity_includes_the_platform_owned_write() {
    let (mut app, node) = fixture();
    select(&mut app, node, 0, 1);
    copy(&mut app.world, node);
    let first = take_write(&mut app.world).unwrap();
    let limit = app.world.resource::<Service>().unwrap().limit;
    for _ in 0..limit {
        copy(&mut app.world, node);
    }
    assert_eq!(
        app.world.resource::<Service>().unwrap().pending.len(),
        limit - 1
    );
    flush(&mut app);
    assert_eq!(
        app.world.resource::<Results>().unwrap().0[0].result,
        Err(ClipboardError::Capacity)
    );
    complete(&mut app.world, &first.complete(Ok(())));
}

#[test]
fn snapshots_validate_formats_and_never_silently_discard_representations() {
    static TEXT: [ClipboardFormat; 1] = [ClipboardFormat::borrowed("text/plain", b"hello")];
    static ITEMS: [ClipboardItem; 1] = [ClipboardItem::borrowed(&TEXT)];
    let static_data = ClipboardData::borrowed(&ITEMS);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            static_data.validate().unwrap();
            assert_eq!(static_data.plain_text(), Ok("hello"));
        }),
        0
    );
    let mut text = String::from("before");
    let snapshot = ClipboardData::text(&text).unwrap();
    text.clear();
    assert_eq!(snapshot.plain_text(), Ok("before"));
    let mut item = ClipboardItem::with_capacity(2).unwrap();
    item.push(ClipboardFormat::borrowed("text/plain", b"text"))
        .unwrap();
    assert_eq!(
        item.push(ClipboardFormat::borrowed("TEXT/PLAIN", b"other")),
        Err(ClipboardError::InvalidData)
    );
    for invalid in [
        "/plain",
        "text/",
        "text/plain/extra",
        "text/pla in",
        "text/(plain)",
    ] {
        assert_eq!(
            item.push(ClipboardFormat::borrowed(invalid, b"")),
            Err(ClipboardError::InvalidData)
        );
    }
    item.push(ClipboardFormat::borrowed(
        "application/x-pixel",
        b"\x00\xff",
    ))
    .unwrap();
    assert_eq!(
        item.push(ClipboardFormat::borrowed("text/html", b"a")),
        Err(ClipboardError::Capacity)
    );
    let mut data = ClipboardData::with_capacity(1, 6).unwrap();
    data.push(item).unwrap();
    assert_eq!(data.plain_text(), Err(ClipboardError::Unsupported));
    let mut too_small = ClipboardData::with_capacity(1, 4).unwrap();
    assert_eq!(
        too_small.push(ClipboardItem::borrowed(&TEXT)),
        Err(ClipboardError::Capacity)
    );
    assert!(too_small.items().is_empty());
    static INVALID_UTF8: [ClipboardFormat; 1] = [ClipboardFormat::borrowed("text/plain", b"\xff")];
    static INVALID_ITEMS: [ClipboardItem; 1] = [ClipboardItem::borrowed(&INVALID_UTF8)];
    assert_eq!(
        ClipboardData::borrowed(&INVALID_ITEMS).plain_text(),
        Err(ClipboardError::InvalidData)
    );
}

#[test]
fn cloned_builders_keep_their_reserved_storage() {
    let mut item = ClipboardItem::with_capacity(3).unwrap();
    item.push(ClipboardFormat::borrowed("text/plain", b"one"))
        .unwrap();
    let mut item = item.clone();
    let data = ClipboardData::with_capacity(3, 128).unwrap();
    let mut data = data.clone();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            item.push(ClipboardFormat::borrowed("text/html", b"two"))
                .unwrap();
            data.push(item).unwrap();
        }),
        0
    );
    assert_eq!(data.items().len(), 1);
}

#[test]
fn platform_writes_do_not_require_notification_storage_or_a_flush_budget() {
    let mut app = App::headless(32, 32);
    app.with_event_capacity(
        crate::app::EventCapacity::new().commands(CommandCapacity::new().clipboard(1)),
    );
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| {
        ui! {
            View ()
            on InspectSelection(|_: &InspectSelection| Ok(SelectionState::new(true, 0u64)))
            on CopySelection(|_: &CopySelection| ClipboardData::text("payload"))
        }
    });
    copy(&mut app.world, node);
    assert_eq!(app.world.resource::<Service>().unwrap().pending.len(), 1);
    app.poll_event();
    let service = app.world.resource::<Service>().unwrap();
    assert!(service.pending.is_empty());
    assert!(service.running.is_none());
}
