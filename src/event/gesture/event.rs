use crate::ecs::Entity;
use crate::types::{Fixed, Fixed64};

#[derive(Clone, Debug)]
pub enum GestureEvent {
    Tap {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    LongPress {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    DragStart {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    DragMove {
        x: Fixed,
        y: Fixed,
        dx: Fixed,
        dy: Fixed,
        target: Entity,
    },
    DragEnd {
        x: Fixed,
        y: Fixed,
        vx: Fixed,
        vy: Fixed,
        target: Entity,
    },
    Pinch {
        x: Fixed,
        y: Fixed,
        scale_delta: Fixed64,
        target: Entity,
    },
    Rotate {
        x: Fixed,
        y: Fixed,
        angle: Fixed,
        target: Entity,
    },
}

impl GestureEvent {
    pub fn target(&self) -> Entity {
        match self {
            Self::Tap { target, .. }
            | Self::LongPress { target, .. }
            | Self::DragStart { target, .. }
            | Self::DragMove { target, .. }
            | Self::DragEnd { target, .. }
            | Self::Pinch { target, .. }
            | Self::Rotate { target, .. } => *target,
        }
    }
}
