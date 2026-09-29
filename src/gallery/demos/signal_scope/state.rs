use crate::gallery::play::change::ChangeSet;
use crate::prelude::Fixed;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum TriggerEdge {
    #[default]
    Rising,
    Falling,
}

impl TriggerEdge {
    pub(super) const fn toggled(self) -> Self {
        match self {
            Self::Rising => Self::Falling,
            Self::Falling => Self::Rising,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScopeControl {
    Acquire,
    ChannelA,
    ChannelB,
    Stop,
    TimeScale,
    TriggerEdge,
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ScopeModel {
    pub(super) running: bool,
    pub(super) channel_b: bool,
    pub(super) time_scale: u8,
    pub(super) gain_a: u8,
    pub(super) trigger_level: Fixed,
    pub(super) trigger_edge: TriggerEdge,
    pub(super) trace_clock_ms: u32,
}

impl Default for ScopeModel {
    fn default() -> Self {
        Self {
            running: true,
            channel_b: true,
            time_scale: 2,
            gain_a: 2,
            trigger_level: Fixed::from_ratio(3, 10),
            trigger_edge: TriggerEdge::Rising,
            trace_clock_ms: 0,
        }
    }
}

#[crate::model]
impl ScopeModel {
    #[observe]
    pub(super) fn running(&self) -> bool {
        self.running
    }

    #[observe]
    pub(super) fn channel_b(&self) -> bool {
        self.channel_b
    }

    #[observe]
    pub(super) fn time_scale(&self) -> u8 {
        self.time_scale
    }

    #[observe]
    pub(super) fn trigger_edge(&self) -> TriggerEdge {
        self.trigger_edge
    }

    #[observe]
    pub(super) fn uart_symbol(&self) -> usize {
        self.uart_symbol_index(0)
    }

    pub(super) fn activate(&mut self, control: ScopeControl) -> ChangeSet {
        match control {
            ScopeControl::Acquire => self.running = !self.running,
            ScopeControl::ChannelA => self.gain_a = (self.gain_a + 1) % 4,
            ScopeControl::ChannelB => self.channel_b = !self.channel_b,
            ScopeControl::Stop => {
                if !self.running {
                    return ChangeSet::NONE;
                }
                self.running = false;
            }
            ScopeControl::TimeScale => self.time_scale = (self.time_scale + 1) % 4,
            ScopeControl::TriggerEdge => self.trigger_edge = self.trigger_edge.toggled(),
        }
        ChangeSet::VISUAL
    }

    pub(super) fn advance_ms(&mut self, delta_ms: u16) -> ChangeSet {
        if !self.running || delta_ms == 0 {
            return ChangeSet::NONE;
        }
        self.trace_clock_ms = self.trace_clock_ms.wrapping_add(u32::from(delta_ms));
        ChangeSet::VISUAL
    }
}
