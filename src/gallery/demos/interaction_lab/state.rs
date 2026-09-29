use crate::prelude::Fixed;

#[crate::model]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct InteractionModel {
    single: u16,
    double: u16,
    triple: u16,
    long: u16,
    switch_on: bool,
    switch_changes: u16,
    checkbox_on: bool,
    checkbox_changes: u16,
    drag_x: Fixed,
    drag_y: Fixed,
    errored: bool,
    disabled: bool,
    allow_bubble: bool,
    child_taps: u16,
    parent_taps: u16,
}

impl Default for InteractionModel {
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

#[crate::model]
impl InteractionModel {
    #[observe]
    pub(super) fn single(&self) -> u16 {
        self.single
    }

    #[observe]
    pub(super) fn double(&self) -> u16 {
        self.double
    }

    #[observe]
    pub(super) fn triple(&self) -> u16 {
        self.triple
    }

    #[observe]
    pub(super) fn long(&self) -> u16 {
        self.long
    }

    #[observe]
    pub(super) fn switch_on(&self) -> bool {
        self.switch_on
    }

    #[observe]
    pub(super) fn switch_changes(&self) -> u16 {
        self.switch_changes
    }

    #[observe]
    pub(super) fn checkbox_on(&self) -> bool {
        self.checkbox_on
    }

    #[observe]
    pub(super) fn checkbox_changes(&self) -> u16 {
        self.checkbox_changes
    }

    #[observe]
    pub(super) fn drag_x(&self) -> Fixed {
        self.drag_x
    }

    #[observe]
    pub(super) fn drag_y(&self) -> Fixed {
        self.drag_y
    }

    #[observe]
    pub(super) fn errored(&self) -> bool {
        self.errored
    }

    #[observe]
    pub(super) fn disabled(&self) -> bool {
        self.disabled
    }

    #[observe]
    pub(super) fn allow_bubble(&self) -> bool {
        self.allow_bubble
    }

    #[observe]
    pub(super) fn child_taps(&self) -> u16 {
        self.child_taps
    }

    #[observe]
    pub(super) fn parent_taps(&self) -> u16 {
        self.parent_taps
    }

    pub(super) fn record_single(&mut self) {
        increment(&mut self.single)
    }

    pub(super) fn record_double(&mut self) {
        increment(&mut self.double)
    }

    pub(super) fn record_triple(&mut self) {
        increment(&mut self.triple)
    }

    pub(super) fn record_long_press(&mut self) {
        increment(&mut self.long)
    }

    pub(super) fn set_switch(&mut self, on: bool) {
        if self.switch_on == on {
            return;
        }
        self.switch_on = on;
        self.switch_changes = self.switch_changes.saturating_add(1);
    }

    pub(super) fn set_checkbox(&mut self, on: bool) {
        if self.checkbox_on == on {
            return;
        }
        self.checkbox_on = on;
        self.checkbox_changes = self.checkbox_changes.saturating_add(1);
    }

    pub(super) fn set_drag(&mut self, x: Fixed, y: Fixed) {
        if self.drag_x == x && self.drag_y == y {
            return;
        }
        self.drag_x = x;
        self.drag_y = y;
    }

    pub(super) fn reset_drag(&mut self) {
        self.set_drag(Fixed::ZERO, Fixed::ZERO);
    }

    pub(super) fn toggle_error(&mut self) {
        self.errored = !self.errored;
    }

    pub(super) fn toggle_disabled(&mut self) {
        self.disabled = !self.disabled;
    }

    pub(super) fn toggle_bubble(&mut self) {
        self.allow_bubble = !self.allow_bubble;
    }

    pub(super) fn record_child_tap(&mut self) {
        increment(&mut self.child_taps)
    }

    pub(super) fn record_parent_tap(&mut self) {
        increment(&mut self.parent_taps)
    }
}

fn increment(value: &mut u16) {
    let next = value.saturating_add(1);
    if next == *value {
        return;
    }
    *value = next;
}
