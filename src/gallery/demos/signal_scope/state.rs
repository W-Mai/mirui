use crate::core::reactive::Signal;
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
pub(super) struct ScopeState {
    pub(super) running: bool,
    pub(super) channel_b: bool,
    pub(super) time_scale: u8,
    pub(super) gain_a: u8,
    pub(super) trigger_level: Fixed,
    pub(super) trigger_edge: TriggerEdge,
    pub(super) trace_clock_ms: u32,
    pub(super) revision: u32,
}

impl Default for ScopeState {
    fn default() -> Self {
        Self {
            running: true,
            channel_b: true,
            time_scale: 2,
            gain_a: 2,
            trigger_level: Fixed::from_ratio(3, 10),
            trigger_edge: TriggerEdge::Rising,
            trace_clock_ms: 0,
            revision: 0,
        }
    }
}

impl ScopeState {
    pub(super) fn tick(&mut self, delta_ms: u16) -> bool {
        if !self.running || delta_ms == 0 {
            return false;
        }
        self.trace_clock_ms = self.trace_clock_ms.wrapping_add(u32::from(delta_ms));
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[derive(Clone)]
pub(super) struct ScopeModel {
    pub(super) state: Signal<ScopeState>,
}

impl Default for ScopeModel {
    fn default() -> Self {
        Self {
            state: Signal::new(ScopeState::default()),
        }
    }
}

impl ScopeModel {
    pub(super) fn snapshot(&self) -> ScopeState {
        self.state.get_untracked()
    }

    fn update(&self, update: impl FnOnce(&mut ScopeState)) {
        let mut state = self.snapshot();
        update(&mut state);
        state.revision = state.revision.wrapping_add(1);
        self.state.set(state);
    }
}

#[derive(Clone, Copy)]
pub(super) enum ScopeAction {
    ToggleRun,
    ToggleChannelB,
    Stop,
    ToggleEdge,
    TimeScale,
    GainA,
}

impl ScopeAction {
    pub(super) fn publish(self, model: &ScopeModel) {
        model.update(|state| match self {
            Self::ToggleRun => state.running = !state.running,
            Self::ToggleChannelB => state.channel_b = !state.channel_b,
            Self::Stop => state.running = false,
            Self::ToggleEdge => state.trigger_edge = state.trigger_edge.toggled(),
            Self::TimeScale => state.time_scale = (state.time_scale + 1) % 4,
            Self::GainA => state.gain_a = (state.gain_a + 1) % 4,
        });
    }
}
