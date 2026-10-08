//! Build a tabbed control scene through App composition and exercise its input routing.
//! Synthetic pointer events verify Slider and Switch state after selecting their pages.

use mirui::ecs::{Entity, World};
use mirui::input::event::dispatch_input;
use mirui::input::event::gesture::{GestureEvent, GestureSystem};
use mirui::input::event::input::InputEvent;
use mirui::types::{Dimension, Fixed, Viewport};
use mirui::ui;
use mirui::ui::layout::{FlexDirection, LayoutStyle, Position};
use mirui::ui::render_system;
use mirui::ui::widgets::slider::Slider;
use mirui::ui::widgets::switch::Switch;
use mirui::ui::widgets::tab_pages::{TabContent, tab_pages_system};
use mirui::ui::widgets::tabbar::TabBar;

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
    let mut app = mirui::app::App::headless(W, H);
    app.with_default_widgets();
    let root = app
        .spawn_root()
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();
    app.compose(root, |cx| {
        ui! {
            TabBar (id: "tabs", count: 3, width: W, height: 14)
        };
        ui! {
            View (width: W, height: H - 14) [
                TabContent {
                    tab_bar: id("tabs"),
                    index: 0,
                },
            ]
        };
        ui! {
            View (width: W, height: H - 14) [
                TabContent {
                    tab_bar: id("tabs"),
                    index: 1,
                },
            ] {
                Slider (
                    id: "slider",
                    min: 0,
                    max: 100,
                    position: Position::Absolute,
                    left: SLIDER_X,
                    top: SLIDER_Y,
                    width: SLIDER_W,
                    height: SLIDER_H
                )
            }
        };
        ui! {
            View (width: W, height: H - 14) [
                TabContent {
                    tab_bar: id("tabs"),
                    index: 2,
                },
            ] {
                Switch (
                    id: "switch",
                    position: Position::Absolute,
                    left: SWITCH_X,
                    top: SWITCH_Y,
                    width: SWITCH_W,
                    height: SWITCH_H
                )
            }
        };
    });
    let mut world = app.world;
    let slider = world.find_by_id("slider").unwrap();
    let switch = world.find_by_id("switch").unwrap();
    let tab_bar = world.find_by_id("tabs").unwrap();
    tab_pages_system(&mut world).unwrap();
    let viewport = Viewport::new(W, H, Fixed::ONE);
    render_system::update_layout(&mut world, root, &viewport);
    (world, root, slider, switch, tab_bar)
}

/// Drive a single Tap (Down + Up + drain + bubble) at (x, y).
fn tap_at(world: &mut World, root: Entity, x: i32, y: i32, mut now_ms: u32) -> u32 {
    let xf = Fixed::from_int(x);
    let yf = Fixed::from_int(y);
    dispatch_input(
        world,
        Some(root),
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
        Some(root),
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
        {
            mirui::__macro_support::dispatch_entity_gesture(world, g, now_ms).unwrap();
            mirui::__macro_support::flush_entity_events(world).unwrap();
        };
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
    tab_pages_system(world).unwrap();
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
