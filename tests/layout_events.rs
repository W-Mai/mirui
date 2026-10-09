use mirui::prelude::*;
use mirui::types::Viewport;
use mirui::ui::widgets::Button;
use mirui::ui::{ComputedRect, Hidden, HitTarget, InteractionFeedback, Style};

#[model]
struct Sizes {
    width: Fixed,
    calls: [u16; 5],
    initial: [u16; 5],
}

#[model]
impl Sizes {
    #[observe]
    fn width(&self) -> Fixed {
        self.width
    }

    fn record(&mut self, slot: usize, change: &SizeChanged) {
        self.calls[slot] += 1;
        self.initial[slot] += u16::from(change.old.is_none());
        self.width = change.new.width;
    }

    fn calls(&self) -> [u16; 5] {
        self.calls
    }
    fn initial(&self) -> [u16; 5] {
        self.initial
    }
}

#[event]
fn record_size(sizes: model!(Sizes), slot: usize, change: event!(SizeChanged)) {
    sizes.record(slot, change);
}

fn check_size(change: &SizeChanged) {
    assert_eq!(change.new.width, Fixed::from_int(43));
}

#[compose]
fn callbacks(sizes: model!(Sizes)) -> Entity {
    ui! {
        Column (width: 200, height: 240) {
            View (id: "block", width: 40, height: 20) on SizeChanged { sizes.record(0, event); }
            Row (width: 41, height: 20) on SizeChanged(|change: event!(SizeChanged)| {
                sizes.record(1, change);
            })
            Column (width: 42, height: 20) on SizeChanged(record_size::callback(sizes.clone(), 2_usize))
            View (width: 43, height: 20) on SizeChanged(check_size)
            View (id: "dependent", width: ${ Dimension::Px(sizes.width()) }, height: 20)
        }
    }
}

#[test]
fn all_callback_forms_settle_model_properties_without_business_capacity_or_hit_targets() {
    let mut app = App::headless(240, 300);
    app.with_event_capacity(EventCapacity::new());
    let sizes = app.add_model(Sizes {
        width: Fixed::ZERO,
        calls: [0; 5],
        initial: [0; 5],
    });
    let root = app.spawn_root().id();
    let panel = app.compose(root, callbacks(sizes.clone()));
    app.compose(root, |tree| {
        tree.spawn((
            Button::build().on_event(
                SizeChanged::EVENT,
                record_size::callback(sizes.clone(), 3_usize),
            ),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(44),
                    height: Dimension::px(20),
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
    });
    let viewport = Viewport::new(240, 300, Fixed::ONE);
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(sizes.calls(), [1, 1, 1, 1, 0]);
    assert_eq!(sizes.initial(), [1, 1, 1, 1, 0]);
    let dependent = app.world.find_by_id("dependent").unwrap();
    assert_eq!(
        app.world.get::<ComputedRect>(dependent).unwrap().0.w,
        Fixed::from_int(44)
    );
    for (node, _) in app.world.query::<mirui::ui::Widget>().iter() {
        assert!(!app.world.has::<HitTarget>(node));
        assert!(!app.world.has::<InteractionFeedback>(node));
    }
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(sizes.calls(), [1, 1, 1, 1, 0]);
    assert!(!app.world.has::<HitTarget>(panel));

    let block = app.world.find_by_id("block").unwrap();
    app.compose(root, |tree| {
        tree.on_event(
            block,
            SizeChanged::EVENT,
            record_size::callback(sizes.clone(), 4_usize),
        );
    });
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(sizes.calls(), [1, 1, 1, 1, 1]);
    assert_eq!(sizes.initial(), [1, 1, 1, 1, 1]);
}

#[derive(Default)]
struct Trace {
    count: usize,
    initial: usize,
    last: Option<LayoutSize>,
}

#[event]
fn trace_size(change: event!(SizeChanged), trace: res!(mut Trace)) {
    trace.count += 1;
    trace.initial += usize::from(change.old.is_none());
    trace.last = Some(change.new);
}

#[test]
fn size_storage_scales_with_observers_and_ignores_hidden_and_retired_nodes() {
    let mut app = App::headless(200, 200);
    app.with_event_capacity(EventCapacity::new())
        .add_resource(Trace::default());
    let root = app.spawn_root().id();
    let nodes = app.compose(root, |cx| {
        let mut nodes = Vec::new();
        for _ in 0..24 {
            nodes.push(ui! {
                View (width: 10, height: 2) on SizeChanged(trace_size::callback())
            });
        }
        nodes
    });
    let hidden = app.compose(root, |cx| {
        ui! {
            View (width: 19, height: 2) [
                Hidden,
            ] on SizeChanged(trace_size::callback())
        }
    });
    let viewport = Viewport::new(200, 200, Fixed::ONE);
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().count, 24);
    app.world.remove::<Hidden>(hidden);
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().initial, 25);
    app.world.despawn(nodes[0]);
    app.world.get_mut::<Style>(nodes[1]).unwrap().layout.width = Dimension::px(12);
    mirui::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
    let trace = app.world.resource::<Trace>().unwrap();
    assert_eq!(trace.count, 26);
    assert_eq!(trace.initial, 25);
}

#[event]
fn grow(
    change: event!(SizeChanged),
    node: entity!(),
    trace: res!(mut Trace),
    commands: commands!(),
) {
    trace.count += 1;
    commands.set::<mirui::ui::property::prop::Width>(
        node,
        Dimension::Px(change.new.width + Fixed::ONE),
    );
}

#[test]
fn size_feedback_is_bounded_and_remains_pending_after_dirty_collection() {
    let mut app = App::headless(200, 200);
    app.add_resource(Trace::default());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| {
        ui! {
            View (width: 10, height: 20) on SizeChanged(grow::callback())
        }
    });
    app.render_dirty().unwrap();
    let count = app.world.resource::<Trace>().unwrap().count;
    assert_eq!(count, 2);
    assert_eq!(
        app.world.get::<ComputedRect>(node).unwrap().0.w,
        Fixed::from_int(12)
    );
    app.render_dirty().unwrap();
    assert_eq!(app.world.resource::<Trace>().unwrap().count, count * 2);
    assert_eq!(
        app.world.get::<ComputedRect>(node).unwrap().0.w,
        Fixed::from_int(14)
    );
}

#[derive(Default)]
struct PeerTrace {
    calls: usize,
    widths: [Fixed; 2],
}

#[event]
fn resize_peer(
    change: event!(SizeChanged),
    peer: Entity,
    geometry: query!(ComputedRect),
    commands: commands!(),
) {
    assert!(change.old.is_none());
    assert_eq!(geometry.get(peer).unwrap().0.w, Fixed::from_int(50));
    commands.set::<mirui::ui::property::prop::Width>(peer, Dimension::px(60));
    assert_eq!(geometry.get(peer).unwrap().0.w, Fixed::from_int(50));
}

#[event]
fn record_peer(
    change: event!(SizeChanged),
    node: entity!(),
    geometry: query!(ComputedRect),
    trace: res!(mut PeerTrace),
) {
    assert_eq!(geometry.get(node).unwrap().0.w, change.new.width);
    trace.widths[trace.calls] = change.new.width;
    trace.calls += 1;
}

#[test]
fn size_callbacks_read_complete_pass_geometry_and_defer_commands() {
    let mut app = App::headless(200, 200);
    app.add_resource(PeerTrace::default());
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        let first = ui! {
            View (width: 40, height: 20)
        };
        let peer = ui! {
            View (width: 50, height: 20)
        };
        cx.on_event(first, SizeChanged::EVENT, resize_peer::callback(peer));
        cx.on_event(peer, SizeChanged::EVENT, record_peer::callback());
    });
    mirui::ui::render_system::try_update_layout(
        &mut app.world,
        root,
        &Viewport::new(200, 200, Fixed::ONE),
    )
    .unwrap();
    let trace = app.world.resource::<PeerTrace>().unwrap();
    assert_eq!(trace.calls, 2);
    assert_eq!(trace.widths, [Fixed::from_int(50), Fixed::from_int(60)]);
}
