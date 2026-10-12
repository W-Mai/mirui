use crate::input::event::input::{
    InputEvent, KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_ESCAPE, KEY_HOME, KEY_LEFT, KEY_RETURN,
    KEY_RIGHT, KeyModifiers, PointerKind, WHEEL_LINE_PIXELS,
};
use crate::types::Fixed;
use alloc::string::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextInputAction {
    Preview,
    Commit,
    Ignore,
}

pub(super) fn text_input_action(composing: bool, input_type: &str) -> TextInputAction {
    if composing {
        TextInputAction::Preview
    } else if matches!(input_type, "insertText" | "insertFromPaste") {
        TextInputAction::Commit
    } else {
        TextInputAction::Ignore
    }
}

pub(super) fn keyboard_input(
    key: String,
    pressed: bool,
    modifiers: KeyModifiers,
    repeat: bool,
    composing: bool,
) -> Option<InputEvent> {
    let shortcut = crate::input::event::input::selection_shortcut_key(&key, modifiers);
    if composing && shortcut.is_none() {
        return None;
    }
    let code = match key.as_str() {
        "Backspace" => Some(KEY_BACKSPACE),
        "Delete" => Some(KEY_DELETE),
        "ArrowLeft" => Some(KEY_LEFT),
        "ArrowRight" => Some(KEY_RIGHT),
        "Home" => Some(KEY_HOME),
        "End" => Some(KEY_END),
        "Enter" => Some(KEY_RETURN),
        "Escape" => Some(KEY_ESCAPE),
        _ => shortcut,
    };
    code.map(|code| InputEvent::Key {
        code,
        pressed,
        modifiers: Some(modifiers),
        repeat: Some(repeat),
    })
}

#[derive(Clone, Copy)]
pub(super) enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
    Enter,
    Leave,
}

pub(super) struct WebPointers {
    slots: super::pointer_slots::PointerSlots<i32, 16>,
    down: u32,
    outside: u32,
}

impl WebPointers {
    pub(super) fn new() -> Self {
        Self {
            slots: super::pointer_slots::PointerSlots::new(),
            down: 0,
            outside: 0,
        }
    }

    pub(super) fn clear(&mut self) {
        self.slots.clear();
        self.down = 0;
        self.outside = 0;
    }

    pub(super) fn translate(
        &mut self,
        native_id: i32,
        kind: PointerKind,
        phase: PointerPhase,
    ) -> Option<u8> {
        let id = if kind == PointerKind::Mouse {
            0
        } else {
            match phase {
                PointerPhase::Down => self.slots.insert(native_id)?,
                PointerPhase::Move | PointerPhase::Enter if kind == PointerKind::Pen => {
                    self.slots.insert(native_id)?
                }
                _ => self.slots.get(native_id)?,
            }
        };
        let mask = 1 << id;
        match phase {
            PointerPhase::Down => {
                if self.down & mask != 0 {
                    return None;
                }
                self.down |= mask;
                self.outside &= !mask;
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                if self.down & mask == 0 {
                    return None;
                }
                self.down &= !mask;
                if kind != PointerKind::Mouse
                    && (kind != PointerKind::Pen
                        || self.outside & mask != 0
                        || matches!(phase, PointerPhase::Cancel))
                {
                    self.slots.remove(native_id);
                }
            }
            PointerPhase::Leave => {
                self.outside |= mask;
                if self.down & mask == 0 && kind != PointerKind::Mouse {
                    self.slots.remove(native_id);
                }
            }
            PointerPhase::Enter => self.outside &= !mask,
            PointerPhase::Move => {}
        }
        Some(id)
    }
}

pub(super) fn wheel_delta(
    dx: f64,
    dy: f64,
    mode: u32,
    css_size: (f64, f64),
    logical_size: Option<(u16, u16)>,
) -> Option<(Fixed, Fixed)> {
    let (width, height) = css_size;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (logical_width, logical_height) = logical_size
        .map(|(width, height)| (f64::from(width), f64::from(height)))
        .unwrap_or(css_size);
    let (unit_x, unit_y) = match mode {
        0 => (logical_width / width, logical_height / height), // DOM_DELTA_PIXEL
        1 => {
            let step = f64::from(WHEEL_LINE_PIXELS.to_f32());
            (step, step)
        }
        2 => (logical_width, logical_height), // DOM_DELTA_PAGE
        _ => return None,
    };
    Some((
        Fixed::from_f32((dx * unit_x) as f32),
        Fixed::from_f32((-dy * unit_y) as f32),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ordinary_dom_insertions_commit_outside_composition() {
        for kind in ["insertText", "insertFromPaste"] {
            assert_eq!(text_input_action(false, kind), TextInputAction::Commit);
            assert_eq!(text_input_action(true, kind), TextInputAction::Preview);
        }
        for kind in [
            "insertCompositionText",
            "insertFromComposition",
            "deleteCompositionText",
        ] {
            assert_eq!(text_input_action(true, kind), TextInputAction::Preview);
            assert_eq!(text_input_action(false, kind), TextInputAction::Ignore);
        }
        for kind in [
            "historyUndo",
            "deleteContentBackward",
            "insertReplacementText",
            "unknown",
        ] {
            assert_eq!(text_input_action(false, kind), TextInputAction::Ignore);
        }
    }

    #[test]
    fn selection_shortcuts_are_available_during_composition_without_changing_metadata() {
        use crate::input::event::input::{KEY_A, KEY_C, KEY_V, KEY_X};
        for (key, expected) in [
            ("a", KEY_A),
            ("A", KEY_A),
            ("c", KEY_C),
            ("C", KEY_C),
            ("v", KEY_V),
            ("V", KEY_V),
            ("x", KEY_X),
            ("X", KEY_X),
        ] {
            for modifiers in [
                KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                KeyModifiers {
                    meta: true,
                    ..Default::default()
                },
            ] {
                for composing in [false, true] {
                    for pressed in [false, true] {
                        assert!(
                            matches!(keyboard_input(key.into(), pressed, modifiers, true, composing), Some(InputEvent::Key { code, pressed: actual, modifiers: Some(actual_modifiers), repeat: Some(true) }) if code == expected && actual == pressed && actual_modifiers == modifiers)
                        );
                    }
                }
            }
            for modifiers in [
                KeyModifiers::default(),
                KeyModifiers {
                    control: true,
                    alt: true,
                    ..Default::default()
                },
                KeyModifiers {
                    meta: true,
                    shift: true,
                    ..Default::default()
                },
            ] {
                assert!(keyboard_input(key.into(), true, modifiers, false, false).is_none());
            }
        }
    }

    #[test]
    fn composing_keys_do_not_escape_into_editing_or_shortcuts() {
        for key in [
            "Enter",
            "Backspace",
            "Delete",
            "ArrowLeft",
            "ArrowRight",
            "Home",
            "End",
            "Escape",
            "a",
            "中",
            "Process",
        ] {
            for pressed in [false, true] {
                assert!(
                    keyboard_input(key.into(), pressed, KeyModifiers::default(), false, true)
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn editing_keys_keep_metadata_and_characters_belong_to_dom_input() {
        let modifiers = KeyModifiers {
            shift: true,
            ..Default::default()
        };
        for pressed in [false, true] {
            assert!(
                matches!(keyboard_input("Enter".into(), pressed, modifiers, true, false),
                Some(InputEvent::Key { code: KEY_RETURN, pressed: actual, modifiers: Some(actual_modifiers), repeat: Some(true) })
                if actual == pressed && actual_modifiers == modifiers)
            );
        }
        for key in ["a", "中", "Dead", "Process", "Unidentified"] {
            for pressed in [false, true] {
                assert!(keyboard_input(key.into(), pressed, modifiers, false, false).is_none());
            }
        }
    }

    #[test]
    fn pointer_identity_survives_colliding_native_ids_without_allocations() {
        use PointerPhase::*;
        let mut pointers = WebPointers::new();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert_eq!(pointers.translate(1, PointerKind::Mouse, Down), Some(0));
                assert_eq!(pointers.translate(1, PointerKind::Touch, Down), Some(1));
                assert_eq!(pointers.translate(256, PointerKind::Touch, Down), Some(2));
                assert_eq!(pointers.translate(1, PointerKind::Touch, Down), None);
                assert_eq!(pointers.translate(256, PointerKind::Touch, Move), Some(2));
                assert_eq!(pointers.translate(1, PointerKind::Touch, Up), Some(1));
                assert_eq!(pointers.translate(1, PointerKind::Touch, Cancel), None);
                assert_eq!(pointers.translate(1, PointerKind::Touch, Move), None);
                assert_eq!(pointers.translate(511, PointerKind::Touch, Down), Some(1));
                assert_eq!(pointers.translate(256, PointerKind::Touch, Cancel), Some(2));
                assert_eq!(pointers.translate(1, PointerKind::Mouse, Up), Some(0));
                assert_eq!(pointers.translate(1, PointerKind::Mouse, Cancel), None);
            }),
            0
        );
    }

    #[test]
    fn hover_slots_remain_stable_during_capture_and_release_on_leave() {
        use PointerPhase::*;
        let mut pointers = WebPointers::new();
        assert_eq!(pointers.translate(80, PointerKind::Pen, Move), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Down), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Leave), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Enter), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Up), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Move), Some(1));
        assert_eq!(pointers.translate(80, PointerKind::Pen, Leave), Some(1));
        assert_eq!(pointers.translate(90, PointerKind::Pen, Move), Some(1));
        assert_eq!(pointers.translate(90, PointerKind::Pen, Down), Some(1));
        assert_eq!(pointers.translate(90, PointerKind::Pen, Cancel), Some(1));
        assert_eq!(pointers.translate(90, PointerKind::Pen, Cancel), None);
    }

    #[test]
    fn full_pointer_map_does_not_accept_unstarted_touch_after_a_slot_frees() {
        use PointerPhase::*;
        let mut pointers = WebPointers::new();
        for native in 1..=16 {
            assert_eq!(
                pointers.translate(native, PointerKind::Touch, Down),
                Some(native as u8)
            );
        }
        assert_eq!(pointers.translate(17, PointerKind::Touch, Down), None);
        assert_eq!(pointers.translate(1, PointerKind::Touch, Up), Some(1));
        assert_eq!(pointers.translate(17, PointerKind::Touch, Move), None);
        assert_eq!(pointers.translate(17, PointerKind::Touch, Up), None);
        assert_eq!(pointers.translate(18, PointerKind::Touch, Down), Some(1));
        pointers.clear();
        assert_eq!(pointers.translate(18, PointerKind::Touch, Up), None);
        assert_eq!(pointers.translate(2, PointerKind::Touch, Move), None);
        assert_eq!(pointers.translate(19, PointerKind::Touch, Down), Some(1));
    }

    #[test]
    fn leaving_during_capture_keeps_the_slot_until_release() {
        use PointerPhase::*;
        let mut pointers = WebPointers::new();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert_eq!(pointers.translate(80, PointerKind::Pen, Enter), Some(1));
                assert_eq!(pointers.translate(80, PointerKind::Pen, Down), Some(1));
                assert_eq!(pointers.translate(80, PointerKind::Pen, Leave), Some(1));
                assert_eq!(pointers.translate(81, PointerKind::Touch, Down), Some(2));
                assert_eq!(pointers.translate(80, PointerKind::Pen, Up), Some(1));
                assert_eq!(pointers.translate(80, PointerKind::Pen, Cancel), None);
                assert_eq!(pointers.translate(82, PointerKind::Pen, Enter), Some(1));
                assert_eq!(pointers.translate(83, PointerKind::Unknown, Enter), None);
                assert_eq!(pointers.translate(84, PointerKind::Touch, Enter), None);
            }),
            0
        );
    }

    #[test]
    fn unknown_pointer_category_does_not_invent_hover_capability() {
        use PointerPhase::*;
        let mut pointers = WebPointers::new();
        assert_eq!(pointers.translate(9, PointerKind::Unknown, Move), None);
        assert_eq!(pointers.translate(9, PointerKind::Unknown, Down), Some(1));
        assert_eq!(pointers.translate(9, PointerKind::Unknown, Move), Some(1));
        assert_eq!(pointers.translate(9, PointerKind::Unknown, Up), Some(1));
        assert_eq!(pointers.translate(9, PointerKind::Unknown, Move), None);
    }

    #[test]
    fn pixel_delta_scales_once_and_retains_subpixel_momentum() {
        assert_eq!(
            wheel_delta(0.25, -0.125, 0, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_ratio(1, 2), Fixed::from_ratio(3, 8)))
        );
        assert_eq!(
            wheel_delta(0.5, 0.25, 0, (800.0, 600.0), Some((400, 300))),
            Some((Fixed::from_ratio(1, 4), Fixed::from_ratio(-1, 8)))
        );
        // Scaling happens before conversion to the fixed-point lattice.
        assert_eq!(
            wheel_delta(0.001, 0.001, 0, (10.0, 10.0), Some((200, 200))),
            Some((Fixed::from_f32(0.02), Fixed::from_f32(-0.02)))
        );
    }

    #[test]
    fn line_and_page_units_do_not_inherit_css_pixel_scaling() {
        assert_eq!(
            wheel_delta(1.0, -2.0, 1, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_int(20), Fixed::from_int(40)))
        );
        assert_eq!(
            wheel_delta(-0.5, 1.0, 2, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_int(-400), Fixed::from_int(-900)))
        );
    }

    #[test]
    fn unscaled_canvas_uses_its_current_css_extent() {
        assert_eq!(
            wheel_delta(0.25, 0.5, 0, (400.0, 300.0), None),
            Some((Fixed::from_ratio(1, 4), Fixed::from_ratio(-1, 2)))
        );
        assert_eq!(
            wheel_delta(1.0, 1.0, 2, (400.0, 300.0), None),
            Some((Fixed::from_int(400), Fixed::from_int(-300)))
        );
    }

    #[test]
    fn hidden_canvas_and_unknown_units_do_not_generate_wheel_motion() {
        for mode in 0..=2 {
            for css_size in [(0.0, 300.0), (400.0, 0.0)] {
                assert_eq!(
                    wheel_delta(1.0, 1.0, mode, css_size, Some((800, 600))),
                    None
                );
            }
        }
        assert_eq!(wheel_delta(1.0, 1.0, 3, (400.0, 300.0), None), None);
    }
}
