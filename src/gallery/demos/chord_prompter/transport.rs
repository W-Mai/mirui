/// Tick positions are computed from an absolute monotonic anchor, never frames.
pub const TICKS_PER_BEAT: u64 = 960;
const TIME_SCALE: u128 = 60_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Playback {
    Stopped,
    Paused,
    Running,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Boundary {
    Now,
    Beat,
    Bar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Tempo(u32),
    Seek(u64),
    Rhythm(Option<super::Pattern>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    Capacity,
    Tempo,
    Late,
    NotRunning,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    tick: u64,
    action: Action,
}

#[derive(Debug)]
pub struct Transport {
    anchor_us: u64,
    anchor_tick: u64,
    position: u64,
    milli_bpm: u32,
    beats_per_bar: u8,
    playback: Playback,
    rhythm: Option<super::Pattern>,
    queue: [Option<Pending>; 8],
    len: usize,
}

impl Transport {
    pub fn new(milli_bpm: u32, beats_per_bar: u8) -> Option<Self> {
        if !(1000..=999_000).contains(&milli_bpm) || !(1..=16).contains(&beats_per_bar) {
            return None;
        }
        Some(Self {
            anchor_us: 0,
            anchor_tick: 0,
            position: 0,
            milli_bpm,
            beats_per_bar,
            playback: Playback::Stopped,
            rhythm: None,
            queue: [None; 8],
            len: 0,
        })
    }

    pub const fn position(&self) -> u64 {
        self.position
    }
    pub const fn playback(&self) -> Playback {
        self.playback
    }
    pub const fn milli_bpm(&self) -> u32 {
        self.milli_bpm
    }
    pub const fn pending(&self) -> usize {
        self.len
    }

    pub const fn rhythm(&self) -> Option<super::Pattern> {
        self.rhythm
    }

    pub fn starts_at_us(&self) -> u64 {
        self.anchor_us
    }

    pub fn start(&mut self, now_us: u64) {
        if self.playback != Playback::Running {
            self.anchor_us = now_us;
            self.anchor_tick = self.position;
            self.playback = Playback::Running;
        }
    }

    pub fn pause(&mut self, now_us: u64) {
        self.advance(now_us);
        self.playback = Playback::Paused;
    }

    pub fn stop(&mut self) {
        self.position = 0;
        self.anchor_tick = 0;
        self.playback = Playback::Stopped;
        self.queue.fill(None);
        self.len = 0;
    }

    pub fn schedule(
        &mut self,
        now_us: u64,
        boundary: Boundary,
        action: Action,
    ) -> Result<u64, CommandError> {
        if let Action::Tempo(value) = action
            && !(1000..=999_000).contains(&value)
        {
            return Err(CommandError::Tempo);
        }
        self.advance(now_us);
        if boundary == Boundary::Now || self.playback != Playback::Running {
            self.apply(now_us, self.position, action);
            return Ok(self.position);
        }
        let quantum = TICKS_PER_BEAT
            * if boundary == Boundary::Bar {
                u64::from(self.beats_per_bar)
            } else {
                1
            };
        let tick = (self.position / quantum + 1).saturating_mul(quantum);
        self.enqueue(tick, action)?;
        Ok(tick)
    }

    pub fn schedule_at(
        &mut self,
        now_us: u64,
        tick: u64,
        action: Action,
    ) -> Result<u64, CommandError> {
        if let Action::Tempo(value) = action
            && !(1000..=999_000).contains(&value)
        {
            return Err(CommandError::Tempo);
        }
        self.advance(now_us);
        if tick < self.position {
            return Err(CommandError::Late);
        }
        if tick == self.position {
            self.apply(now_us, tick, action);
            return Ok(tick);
        }
        if self.playback != Playback::Running {
            return Err(CommandError::NotRunning);
        }
        self.enqueue(tick, action)?;
        Ok(tick)
    }

    fn enqueue(&mut self, tick: u64, action: Action) -> Result<(), CommandError> {
        if self.len == self.queue.len() {
            return Err(CommandError::Capacity);
        }
        // Stable insertion preserves receipt order for controls on one boundary.
        let mut index = self.len;
        while index > 0 && self.queue[index - 1].is_some_and(|entry| entry.tick > tick) {
            self.queue[index] = self.queue[index - 1];
            index -= 1;
        }
        self.queue[index] = Some(Pending { tick, action });
        self.len += 1;
        Ok(())
    }

    pub fn advance(&mut self, now_us: u64) -> u64 {
        if self.playback != Playback::Running {
            return self.position;
        }
        while let Some(entry) = self.queue[0] {
            let duration = (u128::from(entry.tick.saturating_sub(self.anchor_tick)) * TIME_SCALE)
                .div_ceil(u128::from(self.milli_bpm) * u128::from(TICKS_PER_BEAT));
            let deadline = self
                .anchor_us
                .saturating_add(duration.min(u128::from(u64::MAX)) as u64);
            if deadline > now_us {
                break;
            }
            self.queue.copy_within(1..self.len, 0);
            self.len -= 1;
            self.queue[self.len] = None;
            self.apply(deadline, entry.tick, entry.action);
        }
        let ticks = u128::from(now_us.saturating_sub(self.anchor_us))
            * u128::from(self.milli_bpm)
            * u128::from(TICKS_PER_BEAT)
            / TIME_SCALE;
        self.position = self
            .anchor_tick
            .saturating_add(ticks.min(u128::from(u64::MAX)) as u64);
        self.position
    }

    fn apply(&mut self, now_us: u64, tick: u64, action: Action) {
        self.anchor_us = now_us;
        self.anchor_tick = tick;
        match action {
            Action::Tempo(value) => self.milli_bpm = value,
            Action::Rhythm(value) => self.rhythm = value,
            Action::Seek(position) => {
                self.anchor_tick = position;
                // Coordinates after a seek belong to a different playback epoch.
                self.queue.fill(None);
                self.len = 0;
            }
        }
        self.position = self.anchor_tick;
    }
}
