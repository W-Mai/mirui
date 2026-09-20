use super::{AudioTone, CueId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioCommand {
    Play { cue: CueId, gain: u8 },
    Tone(AudioTone),
    Stop { cue: CueId },
    StopAll,
    SetMasterGain(u8),
    SetMuted(bool),
}

impl AudioCommand {
    const fn is_disposable(self) -> bool {
        matches!(self, Self::Play { .. } | Self::Tone(_))
    }

    const fn state_kind(self) -> u8 {
        match self {
            Self::SetMasterGain(_) => 1,
            Self::SetMuted(_) => 2,
            _ => 0,
        }
    }

    const fn is_priority(self) -> bool {
        !matches!(self, Self::Play { .. } | Self::Tone(_))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioOutputState {
    #[default]
    Starting,
    Locked,
    Ready,
    Suspended,
    Failed,
    Stopped,
}

pub struct AudioBus<const N: usize = 32> {
    queue: [Option<AudioCommand>; N],
    head: usize,
    len: usize,
    muted: bool,
    master_gain: u8,
    saturation_count: u32,
    failure_count: u32,
    state: AudioOutputState,
}

impl<const N: usize> Default for AudioBus<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> AudioBus<N> {
    pub const fn new() -> Self {
        Self {
            queue: [None; N],
            head: 0,
            len: 0,
            muted: false,
            master_gain: 220,
            saturation_count: 0,
            failure_count: 0,
            state: AudioOutputState::Starting,
        }
    }

    pub fn play(&mut self, cue: CueId) -> bool {
        self.play_with_gain(cue, u8::MAX)
    }

    pub fn play_with_gain(&mut self, cue: CueId, gain: u8) -> bool {
        self.enqueue(AudioCommand::Play { cue, gain })
    }

    pub fn tone(&mut self, tone: AudioTone) -> bool {
        self.enqueue(AudioCommand::Tone(tone))
    }

    pub fn stop(&mut self, cue: CueId) -> bool {
        self.enqueue(AudioCommand::Stop { cue })
    }

    pub fn stop_all(&mut self) -> bool {
        self.enqueue(AudioCommand::StopAll)
    }

    pub fn set_muted(&mut self, muted: bool) -> bool {
        self.muted = muted;
        self.enqueue(AudioCommand::SetMuted(muted))
    }

    pub fn toggle_muted(&mut self) -> bool {
        self.set_muted(!self.muted)
    }

    pub fn set_master_gain(&mut self, gain: u8) -> bool {
        self.master_gain = gain;
        self.enqueue(AudioCommand::SetMasterGain(gain))
    }

    pub const fn is_muted(&self) -> bool {
        self.muted
    }

    pub const fn master_gain(&self) -> u8 {
        self.master_gain
    }

    pub const fn pending(&self) -> usize {
        self.len
    }

    pub const fn saturation_count(&self) -> u32 {
        self.saturation_count
    }

    pub const fn failure_count(&self) -> u32 {
        self.failure_count
    }

    pub const fn state(&self) -> AudioOutputState {
        self.state
    }

    pub(crate) fn set_state(&mut self, state: AudioOutputState) {
        self.state = state;
    }

    pub(crate) fn record_failure(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
        self.state = AudioOutputState::Failed;
    }

    pub(crate) fn pop(&mut self) -> Option<AudioCommand> {
        if self.len == 0 || N == 0 {
            return None;
        }
        let command = self.queue[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        command
    }

    fn enqueue(&mut self, command: AudioCommand) -> bool {
        if N == 0 {
            self.saturation_count = self.saturation_count.saturating_add(1);
            return false;
        }
        let state_kind = command.state_kind();
        if state_kind != 0 {
            for offset in (0..self.len).rev() {
                let index = (self.head + offset) % N;
                if self.queue[index].is_some_and(|queued| queued.state_kind() == state_kind) {
                    self.queue[index] = Some(command);
                    return true;
                }
            }
        }
        if self.len == N {
            if command.is_priority()
                && let Some(offset) = (0..self.len).find(|offset| {
                    let index = (self.head + *offset) % N;
                    self.queue[index].is_some_and(AudioCommand::is_disposable)
                })
            {
                self.remove(offset);
            } else if command.is_priority() {
                let _ = self.pop();
            } else {
                self.saturation_count = self.saturation_count.saturating_add(1);
                return false;
            }
            self.saturation_count = self.saturation_count.saturating_add(1);
        }
        let tail = (self.head + self.len) % N;
        self.queue[tail] = Some(command);
        self.len += 1;
        true
    }

    fn remove(&mut self, offset: usize) {
        for current in offset..self.len - 1 {
            let from = (self.head + current + 1) % N;
            let to = (self.head + current) % N;
            self.queue[to] = self.queue[from].take();
        }
        let tail = (self.head + self.len - 1) % N;
        self.queue[tail] = None;
        self.len -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: CueId = CueId::new(1);
    const B: CueId = CueId::new(2);

    #[test]
    fn queue_preserves_command_order() {
        let mut bus = AudioBus::<4>::new();
        assert!(bus.play(A));
        assert!(bus.play(B));
        assert_eq!(bus.pop(), Some(AudioCommand::Play { cue: A, gain: 255 }));
        assert_eq!(bus.pop(), Some(AudioCommand::Play { cue: B, gain: 255 }));
        assert_eq!(bus.pop(), None);
    }

    #[test]
    fn priority_command_replaces_disposable_play_when_full() {
        let mut bus = AudioBus::<2>::new();
        bus.play(A);
        bus.play(B);
        assert!(bus.set_muted(true));
        assert_eq!(bus.saturation_count(), 1);
        assert_eq!(bus.pop(), Some(AudioCommand::Play { cue: B, gain: 255 }));
        assert_eq!(bus.pop(), Some(AudioCommand::SetMuted(true)));
    }

    #[test]
    fn state_updates_coalesce_before_dispatch() {
        let mut bus = AudioBus::<2>::new();
        bus.set_master_gain(10);
        bus.set_master_gain(90);
        bus.set_muted(true);
        assert_eq!(bus.pending(), 2);
        assert_eq!(bus.pop(), Some(AudioCommand::SetMasterGain(90)));
        assert_eq!(bus.pop(), Some(AudioCommand::SetMuted(true)));
    }

    #[test]
    fn zero_capacity_bus_fails_without_panicking() {
        let mut bus = AudioBus::<0>::new();
        assert!(!bus.play(A));
        assert_eq!(bus.saturation_count(), 1);
    }

    #[test]
    fn direct_tones_share_the_bounded_command_queue() {
        let mut bus = AudioBus::<1>::new();
        let tone = AudioTone::new(69, super::super::Waveform::Sine, 120, 180);
        assert!(bus.tone(tone));
        assert!(!bus.tone(tone));
        assert_eq!(bus.saturation_count(), 1);
        assert_eq!(bus.pop(), Some(AudioCommand::Tone(tone)));
    }
}
