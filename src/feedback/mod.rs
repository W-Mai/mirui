//! Cursor + rotary input feedback overlays.
//!
//! Two ECS entities live under [`crate::widget::WidgetRoot`], one per
//! overlay kind. They are spawned by [`InputFeedbackPlugin`] (cursor
//! lazily, on first [`crate::event::PointerCursor`]) and dirty-tracked
//! through the standard per-entity `Dirty` + `PrevRect` mechanism.
//!
//! State the systems read/write lives in a single [`InputFeedback`]
//! resource; per-overlay layout (cursor follows the pointer, rotary
//! pins to the right edge) is the entity's `Style.layout`.

pub mod cursor;
pub mod input;
pub mod rotary;

use crate::types::{Fixed, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorFeedbackMode {
    #[default]
    Dot,
    MagneticRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorVisual {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    pub target: Option<crate::ecs::Entity>,
    pub target_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorFeedback {
    pub enabled: bool,
    pub mode: CursorFeedbackMode,
    pub current: CursorVisual,
    pub last_event_seq: u32,
    pub(crate) entity: Option<crate::ecs::Entity>,
}

impl Default for CursorFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CursorFeedbackMode::Dot,
            current: CursorVisual::default(),
            last_event_seq: 0,
            entity: None,
        }
    }
}

/// `entity` is monotonic: `None` until plugin's first `pre_render` spawns
/// it, then a fixed `Some(_)` for the lifetime of the app. Equality compares
/// it like the other fields, which is fine because it doesn't change after
/// spawn — if that ever stops being true, the dirty short-circuit in
/// `rotary_feedback_system` needs revisiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotaryFeedback {
    pub enabled: bool,
    pub progress: Fixed,
    pub target: Fixed,
    pub velocity: Fixed,
    pub direction: i8,
    pub opacity: Fixed,
    pub last_input_ms: u32,
    pub pulse: Fixed,
    pub last_input_seq: u32,
    pub(crate) entity: Option<crate::ecs::Entity>,
}

impl Default for RotaryFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            progress: Fixed::ZERO,
            target: Fixed::ZERO,
            velocity: Fixed::ZERO,
            direction: 0,
            opacity: Fixed::ZERO,
            last_input_ms: 0,
            pulse: Fixed::ZERO,
            last_input_seq: 0,
            entity: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFeedback {
    pub cursor: CursorFeedback,
    pub rotary: RotaryFeedback,
}

impl InputFeedback {
    pub fn enabled() -> Self {
        Self {
            cursor: CursorFeedback {
                enabled: true,
                ..CursorFeedback::default()
            },
            rotary: RotaryFeedback {
                enabled: true,
                ..RotaryFeedback::default()
            },
        }
    }
}

/// Accumulates input events between system runs. Cleared each time the
/// rotary system consumes them.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputFeedbackInput {
    pub rotary_delta: i16,
    pub wheel_delta_y: Fixed,
    pub click_pulse: bool,
    pub event_seq: u32,
}

/// Marker component on the cursor overlay entity.
pub struct OverlayCursor;

/// Marker component on the rotary overlay entity.
pub struct OverlayRotary;

/// Update an overlay entity's absolute layout to track a logical-pixel rect.
/// The flex pass picks this up next frame and writes the corresponding
/// `ComputedRect`, which is what the dirty walker and view renderer read.
pub(crate) fn write_overlay_layout(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    rect: Rect,
) {
    if let Some(style) = world.get_mut::<crate::widget::Style>(entity) {
        style.layout.left = crate::types::Dimension::Px(rect.x);
        style.layout.top = crate::types::Dimension::Px(rect.y);
        style.layout.width = crate::types::Dimension::Px(rect.w);
        style.layout.height = crate::types::Dimension::Px(rect.h);
    }
}
