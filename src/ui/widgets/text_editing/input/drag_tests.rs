use super::*;
use crate::input::event::gesture::{DragCancel, DragMove, DragStart};
use crate::input::event::input::{InputEvent, PointerKind};
use crate::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::widgets::{TextDocument, TextInput};

#[crate::compose]
fn panel(document: model!(TextDocument)) -> Entity {
    ui! {
        TextInput (width: 120, height: 30, font_size: 10) [
            TextEditing::new(document),
        ]
    }
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn fixture() -> (TestApp, Entity) {
    let mut app = crate::app::App::headless(128, 64);
    app.with_default_widgets().with_default_systems();
    app.add_system(sim_input_system::system());
    let document = app.add_model(TextDocument::new());
    document.set_text("ABCDEF").unwrap();
    let root = app.spawn_root().id();
    let node = app.compose(root, panel(document));
    app.tick();
    app.render().unwrap();
    (app, node)
}

#[test]
fn editor_drag_policy_preserves_explicit_component_configuration() {
    use crate::input::event::scroll::TouchAction;
    for policy in [None, Some(TouchAction::None), Some(TouchAction::PanX)] {
        let mut app = App::headless(128, 64);
        app.with_default_widgets();
        let node = app.spawn_root().id();
        app.world.insert(node, TextEditing::default());
        if let Some(policy) = policy {
            app.world.insert(node, policy);
        }
        crate::ui::lifecycle::prepare(&mut app.world);
        assert_eq!(
            app.world.get::<TouchAction>(node),
            Some(&policy.unwrap_or(TouchAction::PanY))
        );
    }
}

#[test]
fn pointer_drag_selects_in_both_directions_and_repaints_without_changing_text() {
    let (mut app, node) = fixture();
    for (start, end, expected) in [(2, 110, (0, 6)), (110, 2, (6, 0))] {
        let event = |phase, x: i32| match phase {
            0 => InputEvent::PointerDown {
                kind: PointerKind::Mouse,
                id: 0,
                x: x.into(),
                y: 8.into(),
            },
            1 => InputEvent::PointerMove {
                kind: PointerKind::Mouse,
                id: 0,
                x: x.into(),
                y: 8.into(),
            },
            _ => InputEvent::PointerUp {
                kind: PointerKind::Mouse,
                id: 0,
                x: x.into(),
                y: 8.into(),
            },
        };
        app.add_resource(SimulatedInput::new(vec![
            SimCommand::at(0, event(0, start)),
            SimCommand::at(0, event(1, end)),
            SimCommand::at(0, event(2, end)),
        ]));
        app.tick();
        let editor = app.world.get::<TextEditing>(node).unwrap();
        assert_eq!(
            editor.selection(),
            TextSelection {
                anchor: expected.0,
                cursor: expected.1
            }
        );
        assert!(editor.drag_revision.is_none());
        editor.read(|text| assert_eq!(text, "ABCDEF"));
        assert_eq!(
            app.world.resource::<FocusState>().unwrap().focused,
            Some(node)
        );
        app.render_dirty().unwrap();
        let partial: Vec<_> = (0..64)
            .flat_map(|y| (0..128).map(move |x| (x, y)))
            .map(|(x, y)| app.backend.framebuffer().get_pixel(x, y))
            .collect();
        app.render().unwrap();
        for y in 0..64 {
            for x in 0..128 {
                assert_eq!(
                    partial[(y * 128 + x) as usize],
                    app.backend.framebuffer().get_pixel(x, y)
                );
            }
        }
    }
}

#[test]
fn drag_uses_prepared_storage_and_stops_after_external_edits_focus_loss_or_cancel() {
    let (mut app, node) = fixture();
    let start = drag_start::callback::<crate::__macro_support::NodeContext>()
        .prepare(
            &mut app.world,
            crate::input::event::view_input::fixture_capacity(),
        )
        .unwrap();
    let movement = drag_move::callback::<crate::__macro_support::NodeContext>()
        .prepare(
            &mut app.world,
            crate::input::event::view_input::fixture_capacity(),
        )
        .unwrap();
    let cancel = drag_cancel::callback::<crate::__macro_support::NodeContext>()
        .prepare(
            &mut app.world,
            crate::input::event::view_input::fixture_capacity(),
        )
        .unwrap();
    let begin = DragStart {
        x: 2.into(),
        y: 8.into(),
        target: node,
    };
    let moved = DragMove {
        x: 110.into(),
        y: 8.into(),
        dx: 108.into(),
        dy: Fixed::ZERO,
        target: node,
    };
    start.invoke(&mut app.world, &begin, node).unwrap();
    movement.invoke(&mut app.world, &moved, node).unwrap();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            start.invoke(&mut app.world, &begin, node).unwrap();
            movement.invoke(&mut app.world, &moved, node).unwrap();
        }),
        0
    );
    for invalidation in 0..4 {
        app.render().unwrap();
        start.invoke(&mut app.world, &begin, node).unwrap();
        match invalidation {
            0 => {
                app.world
                    .get_mut::<TextEditing>(node)
                    .unwrap()
                    .set_selection(TextSelection {
                        anchor: 1,
                        cursor: 1,
                    })
                    .unwrap();
            }
            1 => {
                apply(&mut app.world, node, Edit::Replace("Z")).unwrap();
            }
            2 => {
                app.world.resource_mut::<FocusState>().unwrap().focused = None;
            }
            _ => {
                cancel
                    .invoke(
                        &mut app.world,
                        &DragCancel {
                            x: 2.into(),
                            y: 8.into(),
                            target: node,
                        },
                        node,
                    )
                    .unwrap();
            }
        }
        let selection = app.world.get::<TextEditing>(node).unwrap().selection();
        movement.invoke(&mut app.world, &moved, node).unwrap();
        let editor = app.world.get::<TextEditing>(node).unwrap();
        assert_eq!(editor.selection(), selection);
        assert!(editor.drag_revision.is_none());
    }
}
