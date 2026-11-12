use super::{Action, Boundary, Pattern, ScoreLimits, SessionHandle, TICKS_PER_BEAT};
use crate::core::model::ModelHandle;
use alloc::vec::Vec;

/// Service-local application errors, separate from GATT transport acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LinkError {
    None = 0,
    Sequence = 1,
    Generation = 2,
    Packet = 3,
    Capacity = 4,
    Transfer = 5,
    Checksum = 6,
    Score = 7,
    Control = 8,
    Glyph = 9,
}

/// A seek argument that drops the queued jump instead of adding one.
pub const CANCEL_JUMP: u32 = u32::MAX;

/// Fixed-size GATT packets share one ordered command stream per connection.
pub struct DeviceLink {
    generation: u32,
    next_seq: u16,
    last: Option<[u8; 20]>,
    reply: [u8; 20],
    last_reply: [u8; 20],
    staging: Vec<u8>,
    expected_bytes: usize,
    expected_checksum: u32,
    receiving: bool,
}

pub fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x01000193)
    })
}

fn get32(packet: &[u8; 20], offset: usize) -> u32 {
    u32::from_le_bytes(
        packet[offset..offset + 4]
            .try_into()
            .expect("fixed packet field"),
    )
}

impl DeviceLink {
    /// Use a fresh boot identity so requests from a restarted board are stale.
    pub fn new(boot_identity: u32) -> Self {
        let mut link = Self {
            generation: boot_identity.max(1),
            next_seq: 0,
            last: None,
            reply: [0; 20],
            last_reply: [0; 20],
            staging: Vec::with_capacity(ScoreLimits::DEVICE.bytes),
            expected_bytes: 0,
            expected_checksum: 0,
            receiving: false,
        };
        link.fill_reply(0, u16::MAX, LinkError::None, 0, 0);
        link
    }

    pub fn reconnect(&mut self) {
        self.next_seq = 0;
        self.last = None;
        self.staging.clear();
        self.receiving = false;
        self.fill_reply(0, u16::MAX, LinkError::None, 0, 0);
    }

    pub fn receipt(&self) -> [u8; 20] {
        self.reply
    }

    pub fn status(&self, session: &SessionHandle, now_us: u64) -> [u8; 20] {
        session.advance(now_us);
        let mut packet = [0u8; 20];
        packet[0] = 1;
        packet[1] = u8::from(session.playing())
            | (u8::from(session.pending() > 0) << 1)
            | (u8::from(session.starts_at_us() > now_us && session.playing()) << 2)
            | (u8::from(self.receiving) << 3);
        packet[1] |= (Pattern::ALL
            .iter()
            .position(|pattern| *pattern == session.pattern())
            .unwrap_or(0) as u8)
            << 4;
        packet[2] = session.beats();
        packet[3] = session.unit();
        packet[4..8].copy_from_slice(&self.generation.to_le_bytes());
        packet[8..12].copy_from_slice(&(session.position() as u32).to_le_bytes());
        packet[12..16].copy_from_slice(&session.milli_bpm().to_le_bytes());
        packet[16..20].copy_from_slice(&((now_us / 1000) as u32).to_le_bytes());
        packet
    }

    pub fn write(&mut self, session: &SessionHandle, packet: [u8; 20], now_us: u64) -> [u8; 20] {
        if self.last == Some(packet) {
            self.reply = self.last_reply;
            return self.reply;
        }
        let seq = u16::from_le_bytes([packet[1], packet[2]]);
        if seq != self.next_seq {
            self.fill_reply(packet[0], seq, LinkError::Sequence, 0, 0);
            return self.reply;
        }
        self.next_seq = self.next_seq.wrapping_add(1);
        self.last = Some(packet);
        session.advance(now_us);
        let result = self.apply(session, &packet, now_us);
        let (error, tick, start_us) = match result {
            Ok((tick, start)) => (LinkError::None, tick, start),
            Err(error) => (error, session.position(), 0),
        };
        self.fill_reply(packet[0], seq, error, tick, start_us);
        self.last_reply = self.reply;
        self.reply
    }

    fn fill_reply(&mut self, opcode: u8, seq: u16, error: LinkError, tick: u64, start_us: u64) {
        self.reply = [0; 20];
        self.reply[0] = error as u8;
        self.reply[1] = opcode;
        self.reply[2..4].copy_from_slice(&seq.to_le_bytes());
        self.reply[4..8].copy_from_slice(&self.generation.to_le_bytes());
        self.reply[8..12].copy_from_slice(&(tick as u32).to_le_bytes());
        self.reply[12..16].copy_from_slice(&((start_us / 1000) as u32).to_le_bytes());
        self.reply[16..18].copy_from_slice(&self.next_seq.to_le_bytes());
    }

    fn apply(
        &mut self,
        session: &SessionHandle,
        packet: &[u8; 20],
        now_us: u64,
    ) -> Result<(u64, u64), LinkError> {
        let opcode = packet[0];
        if opcode == 0x11 {
            let offset = usize::from(u16::from_le_bytes([packet[3], packet[4]]));
            let bytes = usize::from(packet[5]);
            if !self.receiving
                || bytes == 0
                || bytes > 14
                || offset != self.staging.len()
                || offset + bytes > self.expected_bytes
            {
                return Err(LinkError::Transfer);
            }
            self.staging.extend_from_slice(&packet[6..6 + bytes]);
            return Ok((session.position(), 0));
        }
        if get32(packet, 3) != self.generation {
            return Err(LinkError::Generation);
        }
        let arg = get32(packet, 8);
        match opcode {
            1 => {
                let delay = u16::try_from(arg)
                    .ok()
                    .filter(|value| *value <= 5000)
                    .ok_or(LinkError::Packet)?;
                let mut start = 0;
                ModelHandle::update(session, |data| start = data.play_at(now_us, delay));
                return Ok((session.position(), start));
            }
            2 => session.pause(now_us),
            3 => session.stop(),
            4..=6 => {
                let boundary = match packet[7] {
                    0 => Boundary::Now,
                    1 => Boundary::Beat,
                    2 => Boundary::Bar,
                    _ => return Err(LinkError::Packet),
                };
                if opcode == 5 && arg == CANCEL_JUMP {
                    session.cancel_jump();
                    return Ok((session.position(), 0));
                }
                let action = match opcode {
                    4 => Action::Tempo(arg),
                    5 => {
                        if arg >= u32::from(session.bars()) {
                            return Err(LinkError::Packet);
                        }
                        Action::Seek(u64::from(arg) * u64::from(session.beats()) * TICKS_PER_BEAT)
                    }
                    _ => Action::Rhythm(if arg == 255 {
                        None
                    } else {
                        Some(*Pattern::ALL.get(arg as usize).ok_or(LinkError::Packet)?)
                    }),
                };
                let target = get32(packet, 12);
                let mut tick = 0;
                ModelHandle::try_update(session, |data| {
                    tick = data.control(
                        now_us,
                        action,
                        boundary,
                        (target != 0).then_some(u64::from(target)),
                    )?;
                    Ok::<(), super::CommandError>(())
                })
                .map_err(|_| LinkError::Control)?;
                return Ok((tick, 0));
            }
            0x10 => {
                let bytes = arg as usize;
                if bytes == 0 || bytes > ScoreLimits::DEVICE.bytes {
                    return Err(LinkError::Capacity);
                }
                self.staging.clear();
                self.expected_bytes = bytes;
                self.expected_checksum = get32(packet, 12);
                self.receiving = true;
            }
            0x12 => {
                if !self.receiving || self.staging.len() != self.expected_bytes {
                    return Err(LinkError::Transfer);
                }
                if checksum(&self.staging) != self.expected_checksum {
                    return Err(LinkError::Checksum);
                }
                let source = core::str::from_utf8(&self.staging).map_err(|_| LinkError::Score)?;
                session.load(source).map_err(|error| match error {
                    super::ScoreError::UnsupportedCharacter(_) => LinkError::Glyph,
                    _ => LinkError::Score,
                })?;
                self.generation = self.generation.wrapping_add(1).max(1);
                self.receiving = false;
                self.staging.clear();
            }
            _ => return Err(LinkError::Packet),
        }
        Ok((session.position(), 0))
    }
}
