use crate::prelude::{Fixed, Signal};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InteractionLabState {
    pub(super) single: u16,
    pub(super) double: u16,
    pub(super) triple: u16,
    pub(super) long: u16,
    pub(super) switch_on: bool,
    pub(super) switch_changes: u16,
    pub(super) checkbox_on: bool,
    pub(super) checkbox_changes: u16,
    pub(super) drag_x: Fixed,
    pub(super) drag_y: Fixed,
    pub(super) errored: bool,
    pub(super) disabled: bool,
    pub(super) allow_bubble: bool,
    pub(super) child_taps: u16,
    pub(super) parent_taps: u16,
}

impl Default for InteractionLabState {
    fn default() -> Self {
        Self {
            single: 0,
            double: 0,
            triple: 0,
            long: 0,
            switch_on: false,
            switch_changes: 0,
            checkbox_on: false,
            checkbox_changes: 0,
            drag_x: Fixed::ZERO,
            drag_y: Fixed::ZERO,
            errored: true,
            disabled: true,
            allow_bubble: false,
            child_taps: 0,
            parent_taps: 0,
        }
    }
}

#[derive(Clone)]
pub(super) struct InteractionModel {
    pub(super) state: Signal<InteractionLabState>,
}

impl Default for InteractionModel {
    fn default() -> Self {
        Self {
            state: Signal::new(InteractionLabState::default()),
        }
    }
}

impl InteractionModel {
    pub(super) fn signal(&self) -> Signal<InteractionLabState> {
        self.state.clone()
    }
}

pub(super) enum InteractionAction {
    Single,
    Double,
    Triple,
    Long,
    Switch(bool),
    Checkbox(bool),
    Drag(Fixed, Fixed),
    ResetDrag,
    ToggleError,
    ToggleDisabled,
    ToggleBubble,
    ChildTap,
    ParentTap,
}

impl InteractionAction {
    pub(super) fn publish(self, signal: &Signal<InteractionLabState>) {
        let current = signal.get_untracked();
        let mut next = current;
        match self {
            Self::Single => next.single = next.single.saturating_add(1),
            Self::Double => next.double = next.double.saturating_add(1),
            Self::Triple => next.triple = next.triple.saturating_add(1),
            Self::Long => next.long = next.long.saturating_add(1),
            Self::Switch(on) => {
                next.switch_on = on;
                next.switch_changes = next.switch_changes.saturating_add(1);
            }
            Self::Checkbox(on) => {
                next.checkbox_on = on;
                next.checkbox_changes = next.checkbox_changes.saturating_add(1);
            }
            Self::Drag(x, y) => {
                next.drag_x = x;
                next.drag_y = y;
            }
            Self::ResetDrag => {
                next.drag_x = Fixed::ZERO;
                next.drag_y = Fixed::ZERO;
            }
            Self::ToggleError => next.errored = !next.errored,
            Self::ToggleDisabled => next.disabled = !next.disabled,
            Self::ToggleBubble => next.allow_bubble = !next.allow_bubble,
            Self::ChildTap => next.child_taps = next.child_taps.saturating_add(1),
            Self::ParentTap => next.parent_taps = next.parent_taps.saturating_add(1),
        }
        if next != current {
            signal.set(next);
        }
    }
}

pub(super) fn model_signal(cx: &mut crate::ui::UiScope<'_>) -> Signal<InteractionLabState> {
    if cx.world_mut().resource::<InteractionModel>().is_none() {
        cx.world_mut().insert_resource(InteractionModel::default());
    }
    cx.world_mut()
        .resource::<InteractionModel>()
        .map(InteractionModel::signal)
        .expect("Interaction Lab model")
}
