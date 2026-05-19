//! End-to-end smoke test of the public API surface a third-party
//! crate uses to wire up Tap → state. No SDL window, no ESP flash.
//! Builds a 3-tab demo (TabBar + Slider + Switch), drives synthetic
//! InputEvents through the same `dispatch_input` + `bubble_dispatch`
//! pipeline `App::run` uses, and asserts state changes the user
//! would observe.
//!
//! If any of Slider, Switch, TabBar, dispatch_input, bubble_dispatch,
//! attach_widget_input_handlers, ViewRegistry::with_builtins, or
//! render_system::update_layout stops being reachable from `tests/`,
//! this build breaks — that's the point.

use mirui::components::slider::Slider;
use mirui::components::switch::Switch;
use mirui::components::tab_pages::{TabContent, tab_pages_system};
use mirui::components::tabbar::TabBar;
use mirui::ecs::{Entity, World};
use mirui::event::gesture::{GestureEvent, GestureSystem};
use mirui::event::input::InputEvent;
use mirui::event::widget_input::attach_widget_input_handlers;
use mirui::event::{bubble_dispatch, dispatch_input};
use mirui::layout::{FlexDirection, LayoutStyle, Position};
use mirui::types::{Dimension, Fixed, Viewport};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::render_system;
use mirui::widget::view::ViewRegistry;
use mirui::widget::{Children, Parent, Theme};

const W: u16 = 128;
const H: u16 = 128;
// Switch rect within sw_page (sw_page itself starts at y=14).
const SWITCH_W: i32 = 50;
const SWITCH_H: i32 = 26;
const SWITCH_X: i32 = (W as i32 - SWITCH_W) / 2;
const SWITCH_Y: i32 = 44; // roughly the centre of the 114-tall page

// Slider rect within slide_page.
const SLIDER_W: i32 = 108;
const SLIDER_H: i32 = 20;
const SLIDER_X: i32 = (W as i32 - SLIDER_W) / 2;
const SLIDER_Y: i32 = 47;

/// root → TabBar + 3 TabContent pages, Slider on page 1, Switch on
/// page 2. Returns (world, root, slider, switch, tab_bar).
fn build() -> (World, Entity, Entity, Entity, Entity) {
    let mut world = World::new();
    world.insert_resource(ViewRegistry::with_builtins());
    world.insert_resource(Theme::default());
    world.insert_resource(GestureSystem::default());

    let root = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();

    let tab_bar = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            width: Dimension::px(W as i32),
            height: Dimension::px(14),
            ..Default::default()
        })
        .id();
    world.insert(tab_bar, TabBar::new(3));
    attach(&mut world, root, tab_bar);

    let mk_page = |w: &mut World, idx: u8| {
        let p = WidgetBuilder::new(w)
            .layout(LayoutStyle {
                width: Dimension::px(W as i32),
                height: Dimension::px((H - 14) as i32),
                ..Default::default()
            })
            .id();
        w.insert(
            p,
            TabContent {
                tab_bar,
                index: idx,
            },
        );
        p
    };
    let list_page = mk_page(&mut world, 0);
    let slide_page = mk_page(&mut world, 1);
    let sw_page = mk_page(&mut world, 2);
    attach(&mut world, root, list_page);
    attach(&mut world, root, slide_page);
    attach(&mut world, root, sw_page);

    // Absolutely positioned so the test can predict each handler's
    // ComputedRect without depending on flex justify/align defaults.
    let slider = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(SLIDER_X),
            top: Dimension::px(SLIDER_Y),
            width: Dimension::px(SLIDER_W),
            height: Dimension::px(SLIDER_H),
            ..Default::default()
        })
        .id();
    world.insert(slider, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
    attach(&mut world, slide_page, slider);

    let switch = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(SWITCH_X),
            top: Dimension::px(SWITCH_Y),
            width: Dimension::px(SWITCH_W),
            height: Dimension::px(SWITCH_H),
            ..Default::default()
        })
        .id();
    world.insert(switch, Switch::default());
    attach(&mut world, sw_page, switch);

    // Order matters: pages must have correct Hidden state before
    // update_layout runs, so handlers see ComputedRect for the
    // visible tab only. attach_widget_input_handlers also needs
    // the registry to be installed (done above).
    tab_pages_system(&mut world);
    attach_widget_input_handlers(&mut world, root);
    let viewport = Viewport::new(W, H, Fixed::ONE);
    render_system::update_layout(&mut world, root, &viewport);
    (world, root, slider, switch, tab_bar)
}

fn attach(world: &mut World, parent: Entity, child: Entity) {
    world.insert(child, Parent(parent));
    if let Some(c) = world.get_mut::<Children>(parent) {
        c.0.push(child);
    }
}

/// Drive a single Tap (Down + Up + drain + bubble) at (x, y).
fn tap_at(world: &mut World, root: Entity, x: i32, y: i32, mut now_ms: u32) -> u32 {
    let xf = Fixed::from_int(x);
    let yf = Fixed::from_int(y);
    dispatch_input(
        world,
        root,
        &InputEvent::PointerDown {
            id: 0,
            x: xf,
            y: yf,
        },
        now_ms,
        W,
        H,
    );
    now_ms += 50;
    dispatch_input(
        world,
        root,
        &InputEvent::PointerUp {
            id: 0,
            x: xf,
            y: yf,
        },
        now_ms,
        W,
        H,
    );
    let pending: Vec<GestureEvent> = world
        .resource_mut::<GestureSystem>()
        .map(|gs| gs.events.drain().collect())
        .unwrap_or_default();
    for g in &pending {
        bubble_dispatch(world, g);
    }
    now_ms + 100
}

/// Tap a TabBar tab to switch the visible page. Recomputes layout
/// because `tab_pages_system` toggles `Hidden` markers; ComputedRect
/// for the newly-visible page only gets stamped once layout runs
/// against the new visibility set.
fn select_tab(world: &mut World, root: Entity, idx: u8, now_ms: u32) -> u32 {
    let third = (W as i32) / 3;
    let cx = idx as i32 * third + third / 2;
    let next = tap_at(world, root, cx, 7, now_ms);
    tab_pages_system(world);
    let viewport = Viewport::new(W, H, Fixed::ONE);
    render_system::update_layout(world, root, &viewport);
    next
}

#[test]
fn switch_tab_toggles_switch_via_tap() {
    let (mut world, root, _slider, switch, _bar) = build();
    let now = select_tab(&mut world, root, 2, 100);
    assert!(!world.get::<Switch>(switch).unwrap().on);

    let cx = SWITCH_X + SWITCH_W / 2;
    let cy = 14 + SWITCH_Y + SWITCH_H / 2;
    let now = tap_at(&mut world, root, cx, cy, now);
    assert!(
        world.get::<Switch>(switch).unwrap().on,
        "Switch.on must flip true after Tap at its centre",
    );

    let _ = tap_at(&mut world, root, cx, cy, now);
    assert!(
        !world.get::<Switch>(switch).unwrap().on,
        "second Tap must flip Switch.on back to false",
    );
}

#[test]
fn switch_tab_drives_slider_value_via_tap() {
    let (mut world, root, slider, _switch, _bar) = build();
    let now = select_tab(&mut world, root, 1, 100);

    let sx = SLIDER_X;
    let sy = 14 + SLIDER_Y + SLIDER_H / 2;

    // Tap left edge → 0%.
    let now = tap_at(&mut world, root, sx, sy, now);
    let r = world.get::<Slider>(slider).unwrap().ratio();
    assert!(
        r < Fixed::from_int(1) / Fixed::from_int(20),
        "left edge → ~0, got {r:?}",
    );

    // Tap right edge → 100% (rect right edge is exclusive, so use w-1).
    let now = tap_at(&mut world, root, sx + SLIDER_W - 1, sy, now);
    let r = world.get::<Slider>(slider).unwrap().ratio();
    assert!(
        r > Fixed::from_int(19) / Fixed::from_int(20),
        "right edge → ~1, got {r:?}",
    );

    // Tap centre → ~50%.
    let _ = tap_at(&mut world, root, sx + SLIDER_W / 2, sy, now);
    let r = world.get::<Slider>(slider).unwrap().ratio();
    let promille = (r * Fixed::from_int(1000)).to_int();
    assert!(
        (450..=550).contains(&promille),
        "centre → ~500‰, got {promille}",
    );
}
