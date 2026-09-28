use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::ecs::{Entity, World};
use mirui::input::event::gesture::{GestureEvent, GestureSystem};
use mirui::input::event::hit_test::hit_test;
use mirui::input::event::input::InputEvent;
use mirui::input::event::{bubble_dispatch_at, dispatch_input};
use mirui::model;
use mirui::render::{DrawCommand, DrawRequest, RenderError, RenderRoute, Renderer};
use mirui::surface::FramebufferAccess;
use mirui::text::{TextLayoutCapacity, WorkspaceCapacity};
use mirui::types::{Color, Dimension, Fixed, Viewport};
use mirui::ui;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;
use mirui::ui::{ComputedRect, IdMap, branch};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

const FIRST_COLOR: Color = Color::rgb(241, 31, 42);
const SECOND_COLOR: Color = Color::rgb(27, 88, 239);

#[model]
struct Mode {
    #[observe]
    selected: u8,
}

#[model]
impl Mode {
    fn select(&mut self, selected: u8) {
        self.selected = selected;
    }
}

#[model]
struct BranchState {
    #[observe]
    selected: bool,
    #[observe]
    label: u8,
    first_taps: u32,
    second_taps: u32,
}

#[model]
impl BranchState {
    fn select(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn set_label(&mut self, label: u8) {
        self.label = label;
    }

    fn tap_first(&mut self) {
        self.first_taps += 1;
    }

    fn tap_second(&mut self) {
        self.second_taps += 1;
    }

    fn tap_counts(&self) -> (u32, u32) {
        (self.first_taps, self.second_taps)
    }
}

fn dispatch_tap(world: &mut World, root: Entity, x: Fixed, y: Fixed, now_ms: u32) -> Entity {
    let target = hit_test(world, root, x, y, 128, 96).unwrap();
    dispatch_input(
        world,
        root,
        &InputEvent::PointerDown { id: 0, x, y },
        now_ms,
        128,
        96,
    );
    dispatch_input(
        world,
        root,
        &InputEvent::PointerUp { id: 0, x, y },
        now_ms + 50,
        128,
        96,
    );
    let event = world
        .resource_mut::<GestureSystem>()
        .unwrap()
        .events
        .buffer
        .pop()
        .unwrap();
    assert!(matches!(event, GestureEvent::Tap { target: tapped, .. } if tapped == target));
    bubble_dispatch_at(world, &event, now_ms + 50);
    target
}

#[derive(Default)]
struct FillColors(Vec<Color>);

impl Renderer for FillColors {
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        if let DrawCommand::Fill { color, .. } = request.command {
            self.0.push(*color);
        }
        Ok(())
    }

    fn flush(&mut self) {}
}

fn rendered_fills_and_hit(world: &mut World, root: Entity) -> (Vec<Color>, Option<Entity>) {
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::update_layout(world, root, &viewport);
    let hit = hit_test(world, root, 8.into(), 8.into(), 64, 64);
    let mut fills = FillColors::default();
    render_system::render(world, root, &viewport, &mut fills).unwrap();
    (fills.0, hit)
}

#[test]
fn observed_model_switches_cached_match_paint_and_hit_target_in_both_directions() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    let mode = app.add_model(Mode { selected: 0 });
    let selection = mode.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (width: Dimension::px(64), height: Dimension::px(64)) {
            match ${ selection.selected() } {
                0 => {
                    View (
                        id: "first",
                        width: 32,
                        height: 32,
                        bg_color: FIRST_COLOR
                    ) on Tap {}
                }
                _ => {
                    View (
                        id: "second",
                        width: 32,
                        height: 32,
                        bg_color: SECOND_COLOR
                    ) on Tap {}
                    View (
                        id: "second_extra",
                        width: 32,
                        height: 32,
                        bg_color: SECOND_COLOR
                    ) on Tap {}
                }
            }
        }
    };

    let first = app.world.find_by_id("first").unwrap();
    let second = app.world.find_by_id("second").unwrap();

    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(fills.contains(&FIRST_COLOR));
    assert!(!fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(first));

    let model_allocations = tracked_allocations(|| mode.select(1));
    let flush_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
    let layout_allocations = tracked_allocations(|| {
        render_system::update_layout(&mut app.world, root, &Viewport::new(64, 64, Fixed::ONE));
    });
    let hit_allocations = tracked_allocations(|| {
        assert_eq!(
            hit_test(&app.world, root, 8.into(), 8.into(), 64, 64),
            Some(second)
        );
    });
    assert_eq!(
        (
            model_allocations,
            flush_allocations,
            layout_allocations,
            hit_allocations
        ),
        (0, 0, 0, 0),
        "first retained branch switch allocated"
    );
    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(!fills.contains(&FIRST_COLOR));
    assert!(fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(second));
    assert!(branch::is_effectively_hidden(&app.world, first));

    let switch_back_allocations = tracked_allocations(|| {
        mode.select(0);
        flush_signal_dirty(&mut app.world);
        render_system::update_layout(&mut app.world, root, &Viewport::new(64, 64, Fixed::ONE));
        assert_eq!(
            hit_test(&app.world, root, 8.into(), 8.into(), 64, 64),
            Some(first)
        );
    });
    assert_eq!(
        switch_back_allocations, 0,
        "retained branch return allocated"
    );
    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(fills.contains(&FIRST_COLOR));
    assert!(!fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(first));
    assert!(branch::is_effectively_hidden(&app.world, second));
    assert_eq!(app.world.find_by_id("first"), Some(first));
    assert_eq!(app.world.find_by_id("second"), Some(second));
    assert!(app.world.is_alive(first));
    assert!(app.world.is_alive(second));
}

#[test]
fn first_nested_bounded_text_branch_switch_reuses_full_frame_storage() {
    let mut app = App::headless(128, 96);
    app.with_default_widgets();
    app.with_text_layout_capacity(TextLayoutCapacity {
        layout_slots: 6,
        measurements: 6,
        lines: 24,
        runs: 40,
        glyphs: 64,
        carets: 96,
        workspace: WorkspaceCapacity {
            runs: 16,
            glyphs: 32,
            scratch_glyphs: 32,
            lines: 16,
        },
    })
    .unwrap();
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    let mode = app.add_model(Mode { selected: 0 });
    let selection = mode.share();
    let deep_label = mode.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (width: 128, height: 96) {
            match ${ selection.selected() } {
                0 => {
                    Text (
                        "SHALLOW", id: "shallow_text", text_capacity: 8,
                        width: 128, height: 16, font_size: 8,
                        bg_color: FIRST_COLOR
                    ) on Tap {}
                }
                _ => {
                    Column (width: 128, height: 32) {
                        Column (width: 128, height: 32) {
                            Text (
                                text: ${ format_args!("DEEP {:03}", deep_label.selected()) },
                                id: "deep_text", text_capacity: 8,
                                width: 128, height: 16, font_size: 8,
                                bg_color: SECOND_COLOR
                            ) on Tap {}
                            Text (
                                "SECOND", text_capacity: 8,
                                width: 128, height: 16, font_size: 8
                            )
                        }
                    }
                }
            }
            Text ("TAIL", id: "tail_text", width: 128, height: 16, font_size: 8)
        }
    };

    let shallow = app.world.find_by_id("shallow_text").unwrap();
    let deep = app.world.find_by_id("deep_text").unwrap();
    let tail = app.world.find_by_id("tail_text").unwrap();
    app.render().unwrap();
    let shallow_frame = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_eq!(
        app.world.get::<ComputedRect>(tail).unwrap().0.y,
        Fixed::from_int(16)
    );
    assert_eq!(
        hit_test(&app.world, root, 8.into(), 8.into(), 128, 96),
        Some(shallow)
    );

    let action_allocations = tracked_allocations(|| mode.select(1));
    let notify_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
    let hit_allocations = tracked_allocations(|| {
        assert_eq!(
            hit_test(&app.world, root, 8.into(), 8.into(), 128, 96),
            Some(deep)
        );
    });
    assert_eq!(
        (
            action_allocations,
            notify_allocations,
            render_allocations,
            hit_allocations
        ),
        (0, 0, 0, 0),
        "first nested bounded text branch switch allocated"
    );
    let deep_frame = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_ne!(deep_frame, shallow_frame);
    let text = app.world.get::<Text>(deep).unwrap();
    assert_eq!(text.text_capacity(), Some(8));
    assert_eq!(text.resolve(&app.world), "DEEP 001");
    assert_eq!(
        app.world.get::<ComputedRect>(tail).unwrap().0.y,
        Fixed::from_int(32)
    );
    assert!(app.world.is_alive(shallow));
    assert!(app.world.is_alive(deep));

    let action_allocations = tracked_allocations(|| mode.select(0));
    let notify_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
    let hit_allocations = tracked_allocations(|| {
        assert_eq!(
            hit_test(&app.world, root, 8.into(), 8.into(), 128, 96),
            Some(shallow)
        );
    });
    assert_eq!(
        (
            action_allocations,
            notify_allocations,
            render_allocations,
            hit_allocations
        ),
        (0, 0, 0, 0),
        "nested bounded text branch return allocated"
    );
    assert_eq!(app.backend.framebuffer().buf.as_slice(), shallow_frame);
    assert_eq!(
        app.world.get::<ComputedRect>(tail).unwrap().0.y,
        Fixed::from_int(16)
    );
    assert_eq!(app.world.find_by_id("shallow_text"), Some(shallow));
    assert_eq!(app.world.find_by_id("deep_text"), Some(deep));
}

#[test]
fn cached_branch_routes_taps_and_reveals_hidden_state_without_rebuilding() {
    let mut app = App::headless(128, 96);
    app.with_default_widgets();
    app.with_text_layout_capacity(TextLayoutCapacity {
        layout_slots: 4,
        measurements: 4,
        lines: 16,
        runs: 24,
        glyphs: 48,
        carets: 64,
        workspace: WorkspaceCapacity {
            runs: 12,
            glyphs: 24,
            scratch_glyphs: 24,
            lines: 12,
        },
    })
    .unwrap();
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    let state = app.add_model(BranchState {
        selected: false,
        label: 1,
        first_taps: 0,
        second_taps: 0,
    });
    let selection = state.share();
    let label = state.share();
    let first_action = state.share();
    let second_action = state.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (width: 128, height: 96) {
            match ${ selection.selected() } {
                false => {
                    Text (
                        "FIRST", id: "branch_first", text_capacity: 10,
                        width: 128, height: 16, font_size: 8,
                        bg_color: FIRST_COLOR
                    ) on Tap { first_action.tap_first(); }
                }
                true => {
                    Text (
                        text: ${ format_args!("SECOND {:03}", label.label()) },
                        id: "branch_second", text_capacity: 10,
                        width: 128, height: 16, font_size: 8,
                        bg_color: SECOND_COLOR
                    ) on Tap { second_action.tap_second(); }
                }
            }
        }
    };

    let first = app.world.find_by_id("branch_first").unwrap();
    let second = app.world.find_by_id("branch_second").unwrap();
    app.render().unwrap();
    assert!(branch::is_effectively_hidden(&app.world, second));
    assert_eq!(
        dispatch_tap(&mut app.world, root, 8.into(), 8.into(), 0),
        first
    );
    assert_eq!(state.tap_counts(), (1, 0));

    state.set_label(7);
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();
    assert_eq!(state.tap_counts(), (1, 0));
    assert_eq!(
        hit_test(&app.world, root, 8.into(), 8.into(), 128, 96),
        Some(first)
    );

    let action_allocations = tracked_allocations(|| state.select(true));
    let notify_allocations = tracked_allocations(|| flush_signal_dirty(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());
    assert_eq!(
        (action_allocations, notify_allocations, render_allocations),
        (0, 0, 0),
        "first cached branch reveal allocated"
    );
    assert!(branch::is_effectively_hidden(&app.world, first));
    assert_eq!(
        app.world.get::<Text>(second).unwrap().resolve(&app.world),
        "SECOND 007"
    );
    assert_eq!(
        dispatch_tap(&mut app.world, root, 8.into(), 8.into(), 1_000),
        second
    );
    assert_eq!(state.tap_counts(), (1, 1));

    state.select(false);
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();
    assert_eq!(
        dispatch_tap(&mut app.world, root, 8.into(), 8.into(), 2_000),
        first
    );
    assert_eq!(state.tap_counts(), (2, 1));
    assert_eq!(app.world.find_by_id("branch_first"), Some(first));
    assert_eq!(app.world.find_by_id("branch_second"), Some(second));
}
