use crate::core::storage::Storage;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::echo::{Direction, EchoCommand, EchoModel};
use crate::gallery::play::tidal::{Perk, TideCommand, TideModel};

const MAGIC: [u8; 4] = *b"MAH1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 12;
const TRAILER_LEN: usize = 4;

pub(crate) const TIDAL_REPLAY_CAPACITY: usize = 512;
pub(crate) const ECHO_REPLAY_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ReplayKind {
    Tidal = 1,
    Echo = 2,
}

impl ReplayKind {
    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Tidal),
            2 => Some(Self::Echo),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReplayEvent {
    opcode: u8,
    argument: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplayError {
    Full,
    BufferTooSmall {
        required: usize,
        available: usize,
    },
    Truncated,
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidKind(u8),
    WrongKind {
        expected: ReplayKind,
        actual: ReplayKind,
    },
    InvalidLength,
    InvalidChecksum,
    InvalidEvent {
        index: u16,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayLog<const N: usize> {
    events: [ReplayEvent; N],
    seed: u32,
    len: u16,
    kind: ReplayKind,
}

impl<const N: usize> ReplayLog<N> {
    pub(crate) const fn new(kind: ReplayKind, seed: u32) -> Self {
        Self {
            events: [ReplayEvent {
                opcode: 0,
                argument: 0,
            }; N],
            seed: if seed == 0 { 1 } else { seed },
            len: 0,
            kind,
        }
    }

    pub(crate) const fn len(&self) -> u16 {
        self.len
    }

    pub(crate) const fn is_full(&self) -> bool {
        self.len() == u16::MAX || self.len() as usize == N
    }

    pub(crate) const fn encoded_len(&self) -> usize {
        HEADER_LEN + self.len() as usize * 2 + TRAILER_LEN
    }

    pub(crate) fn record_tide(&mut self, command: TideCommand) -> Result<(), ReplayError> {
        if self.kind != ReplayKind::Tidal {
            return Err(ReplayError::WrongKind {
                expected: ReplayKind::Tidal,
                actual: self.kind,
            });
        }
        let event = encode_tide(command);
        if !valid_event(ReplayKind::Tidal, event) {
            return Err(ReplayError::InvalidEvent { index: self.len });
        }
        self.push(event)
    }

    pub(crate) fn record_echo(&mut self, command: EchoCommand) -> Result<(), ReplayError> {
        if self.kind != ReplayKind::Echo {
            return Err(ReplayError::WrongKind {
                expected: ReplayKind::Echo,
                actual: self.kind,
            });
        }
        let event = encode_echo(command);
        if !valid_event(ReplayKind::Echo, event) {
            return Err(ReplayError::InvalidEvent { index: self.len });
        }
        self.push(event)
    }

    fn push(&mut self, event: ReplayEvent) -> Result<(), ReplayError> {
        if self.is_full() {
            return Err(ReplayError::Full);
        }
        self.events[self.len as usize] = event;
        self.len += 1;
        Ok(())
    }

    pub(crate) fn encode_into(&self, output: &mut [u8]) -> Result<usize, ReplayError> {
        let required = self.encoded_len();
        if output.len() < required {
            return Err(ReplayError::BufferTooSmall {
                required,
                available: output.len(),
            });
        }
        output[..4].copy_from_slice(&MAGIC);
        output[4] = VERSION;
        output[5] = self.kind as u8;
        output[6..8].copy_from_slice(&self.len.to_le_bytes());
        output[8..12].copy_from_slice(&self.seed.to_le_bytes());
        for (index, event) in self.events[..self.len as usize].iter().enumerate() {
            let offset = HEADER_LEN + index * 2;
            output[offset] = event.opcode;
            output[offset + 1] = event.argument;
        }
        let checksum_offset = required - TRAILER_LEN;
        let checksum = crc32(&output[..checksum_offset]);
        output[checksum_offset..required].copy_from_slice(&checksum.to_le_bytes());
        Ok(required)
    }

    pub(crate) fn decode(bytes: &[u8], expected: ReplayKind) -> Result<Self, ReplayError> {
        if bytes.len() < HEADER_LEN + TRAILER_LEN {
            return Err(ReplayError::Truncated);
        }
        if bytes[..4] != MAGIC {
            return Err(ReplayError::InvalidMagic);
        }
        if bytes[4] != VERSION {
            return Err(ReplayError::UnsupportedVersion(bytes[4]));
        }
        let kind = ReplayKind::from_code(bytes[5]).ok_or(ReplayError::InvalidKind(bytes[5]))?;
        if kind != expected {
            return Err(ReplayError::WrongKind {
                expected,
                actual: kind,
            });
        }
        let count = u16::from_le_bytes([bytes[6], bytes[7]]);
        if count as usize > N {
            return Err(ReplayError::Full);
        }
        let expected_len = HEADER_LEN + count as usize * 2 + TRAILER_LEN;
        if bytes.len() != expected_len {
            return Err(ReplayError::InvalidLength);
        }
        let checksum_offset = expected_len - TRAILER_LEN;
        let stored = u32::from_le_bytes(
            bytes[checksum_offset..]
                .try_into()
                .map_err(|_| ReplayError::Truncated)?,
        );
        if stored != crc32(&bytes[..checksum_offset]) {
            return Err(ReplayError::InvalidChecksum);
        }
        let seed = u32::from_le_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| ReplayError::Truncated)?,
        );
        let mut log = Self::new(kind, seed);
        for index in 0..count {
            let offset = HEADER_LEN + index as usize * 2;
            let event = ReplayEvent {
                opcode: bytes[offset],
                argument: bytes[offset + 1],
            };
            if !valid_event(kind, event) {
                return Err(ReplayError::InvalidEvent { index });
            }
            log.events[index as usize] = event;
        }
        log.len = count;
        Ok(log)
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec![0; self.encoded_len()];
        let len = self
            .encode_into(&mut bytes)
            .expect("exact replay buffer length");
        bytes.truncate(len);
        bytes
    }
}

pub(crate) type TidalReplayLog = ReplayLog<TIDAL_REPLAY_CAPACITY>;
pub(crate) type EchoReplayLog = ReplayLog<ECHO_REPLAY_CAPACITY>;

#[cfg(all(
    feature = "persistence",
    not(test),
    target_arch = "wasm32",
    feature = "web-canvas"
))]
pub(crate) fn gallery_storage(name: &str) -> crate::core::storage::StorageHandle {
    use crate::core::storage::{LocalStorageStorage, MemoryStorage};
    LocalStorageStorage::with_prefix(name)
        .map_or_else(|| MemoryStorage::new().into_handle(), Storage::into_handle)
}

#[cfg(all(
    feature = "persistence",
    feature = "std",
    not(test),
    not(all(target_arch = "wasm32", feature = "web-canvas"))
))]
pub(crate) fn gallery_storage(name: &str) -> crate::core::storage::StorageHandle {
    let mut path = std::env::temp_dir();
    path.push(name);
    crate::core::storage::FileStorage::open(path).into_handle()
}

#[cfg(all(feature = "persistence", not(feature = "std")))]
pub(crate) fn gallery_storage(_name: &str) -> crate::core::storage::StorageHandle {
    crate::core::storage::MemoryStorage::new().into_handle()
}

#[cfg(all(feature = "persistence", test, feature = "std"))]
pub(crate) fn gallery_storage(_name: &str) -> crate::core::storage::StorageHandle {
    crate::core::storage::MemoryStorage::new().into_handle()
}

pub(crate) fn replay_tide<const N: usize>(log: &ReplayLog<N>) -> Result<TideModel, ReplayError> {
    if log.kind != ReplayKind::Tidal {
        return Err(ReplayError::WrongKind {
            expected: ReplayKind::Tidal,
            actual: log.kind,
        });
    }
    let mut model = TideModel::new(log.seed);
    for (index, event) in log.events[..log.len as usize].iter().copied().enumerate() {
        let command = decode_tide(event).ok_or(ReplayError::InvalidEvent {
            index: index as u16,
        })?;
        if !model
            .apply_command(command)
            .contains(ChangeSet::PERSISTENCE)
        {
            return Err(ReplayError::InvalidEvent {
                index: index as u16,
            });
        }
    }
    Ok(model)
}

pub(crate) fn replay_echo<const N: usize>(log: &ReplayLog<N>) -> Result<EchoModel, ReplayError> {
    if log.kind != ReplayKind::Echo {
        return Err(ReplayError::WrongKind {
            expected: ReplayKind::Echo,
            actual: log.kind,
        });
    }
    let mut model = EchoModel::new(log.seed);
    for (index, event) in log.events[..log.len as usize].iter().copied().enumerate() {
        let command = decode_echo(event).ok_or(ReplayError::InvalidEvent {
            index: index as u16,
        })?;
        if !model
            .apply_command(command)
            .contains(ChangeSet::PERSISTENCE)
        {
            return Err(ReplayError::InvalidEvent {
                index: index as u16,
            });
        }
    }
    Ok(model)
}

fn encode_tide(command: TideCommand) -> ReplayEvent {
    match command {
        TideCommand::Place { index, choice } => ReplayEvent {
            opcode: 0,
            argument: index | choice << 6,
        },
        TideCommand::Reroll => ReplayEvent {
            opcode: 1,
            argument: 0,
        },
        TideCommand::Undo => ReplayEvent {
            opcode: 2,
            argument: 0,
        },
        TideCommand::Continue { perk } => ReplayEvent {
            opcode: 3,
            argument: perk.map_or(0, |value| value as u8 + 1),
        },
    }
}

fn decode_tide(event: ReplayEvent) -> Option<TideCommand> {
    match event.opcode {
        0 => {
            let index = event.argument & 0x3f;
            let choice = event.argument >> 6;
            (index < 36 && choice < 3).then_some(TideCommand::Place { index, choice })
        }
        1 if event.argument == 0 => Some(TideCommand::Reroll),
        2 if event.argument == 0 => Some(TideCommand::Undo),
        3 => {
            let perk = if event.argument == 0 {
                None
            } else {
                Some(Perk::from_code(event.argument - 1)?)
            };
            Some(TideCommand::Continue { perk })
        }
        _ => None,
    }
}

fn encode_echo(command: EchoCommand) -> ReplayEvent {
    match command {
        EchoCommand::Step(direction) => ReplayEvent {
            opcode: 0,
            argument: direction.wire_code(),
        },
        EchoCommand::Rewind => ReplayEvent {
            opcode: 1,
            argument: 0,
        },
        EchoCommand::Restart => ReplayEvent {
            opcode: 2,
            argument: 0,
        },
        EchoCommand::Clear => ReplayEvent {
            opcode: 3,
            argument: 0,
        },
        EchoCommand::Undo => ReplayEvent {
            opcode: 4,
            argument: 0,
        },
        EchoCommand::Continue => ReplayEvent {
            opcode: 5,
            argument: 0,
        },
    }
}

fn decode_echo(event: ReplayEvent) -> Option<EchoCommand> {
    match event.opcode {
        0 => Direction::try_from_code(event.argument).map(EchoCommand::Step),
        1 if event.argument == 0 => Some(EchoCommand::Rewind),
        2 if event.argument == 0 => Some(EchoCommand::Restart),
        3 if event.argument == 0 => Some(EchoCommand::Clear),
        4 if event.argument == 0 => Some(EchoCommand::Undo),
        5 if event.argument == 0 => Some(EchoCommand::Continue),
        _ => None,
    }
}

fn valid_event(kind: ReplayKind, event: ReplayEvent) -> bool {
    match kind {
        ReplayKind::Tidal => decode_tide(event).is_some(),
        ReplayKind::Echo => decode_echo(event).is_some(),
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::MemoryStorage;

    fn first_tide_place(model: &TideModel) -> TideCommand {
        for index in 0..36 {
            if model.valid(index) {
                return TideCommand::Place {
                    index: index as u8,
                    choice: 0,
                };
            }
        }
        panic!("generated island has no valid placement");
    }

    fn first_echo_step(seed: u32) -> EchoCommand {
        for direction in [
            Direction::Up,
            Direction::Right,
            Direction::Down,
            Direction::Left,
            Direction::Wait,
        ] {
            let mut copy = EchoModel::new(seed);
            if copy.step(direction).contains(ChangeSet::PERSISTENCE) {
                return EchoCommand::Step(direction);
            }
        }
        panic!("generated room has no valid step");
    }

    #[test]
    fn tide_codec_round_trips_and_replays_without_heap_growth() {
        let seed = 73;
        let mut source = TideModel::new(seed);
        let mut log = ReplayLog::<32>::new(ReplayKind::Tidal, seed);
        let place = first_tide_place(&source);
        for command in [place, TideCommand::Undo, place] {
            assert!(
                source
                    .apply_command(command)
                    .contains(ChangeSet::PERSISTENCE)
            );
            log.record_tide(command).unwrap();
        }
        let mut bytes = [0_u8; 80];
        let len = log.encode_into(&mut bytes).unwrap();
        let decoded = ReplayLog::<32>::decode(&bytes[..len], ReplayKind::Tidal).unwrap();
        let restored = replay_tide(&decoded).unwrap();
        assert_eq!(restored.turn(), source.turn());
        assert_eq!(restored.score(), source.score());
        assert_eq!(restored.history_len(), source.history_len());
        assert_eq!(decoded.len(), 3);
    }

    #[test]
    fn echo_codec_round_trips_and_replays() {
        let seed = 91;
        let mut source = EchoModel::new(seed);
        let mut log = ReplayLog::<32>::new(ReplayKind::Echo, seed);
        let step = first_echo_step(seed);
        for command in [step, EchoCommand::Undo, step, EchoCommand::Rewind] {
            assert!(
                source
                    .apply_command(command)
                    .contains(ChangeSet::PERSISTENCE)
            );
            log.record_echo(command).unwrap();
        }
        let mut bytes = [0_u8; 80];
        let len = log.encode_into(&mut bytes).unwrap();
        let decoded = ReplayLog::<32>::decode(&bytes[..len], ReplayKind::Echo).unwrap();
        let restored = replay_echo(&decoded).unwrap();
        assert_eq!(restored.tick(), source.tick());
        assert_eq!(restored.position(), source.position());
        assert_eq!(restored.ghost_count(), source.ghost_count());
        assert_eq!(restored.route_len(), source.route_len());
    }

    #[test]
    fn checksum_kind_capacity_and_semantics_are_validated() {
        let mut log = ReplayLog::<2>::new(ReplayKind::Echo, 5);
        log.record_echo(EchoCommand::Step(Direction::Wait)).unwrap();
        let mut bytes = [0_u8; 32];
        let len = log.encode_into(&mut bytes).unwrap();
        assert!(matches!(
            ReplayLog::<2>::decode(&bytes[..len], ReplayKind::Tidal),
            Err(ReplayError::WrongKind { .. })
        ));
        bytes[HEADER_LEN] ^= 1;
        let Err(error) = ReplayLog::<2>::decode(&bytes[..len], ReplayKind::Echo) else {
            panic!("corrupt checksum was accepted");
        };
        assert_eq!(error, ReplayError::InvalidChecksum);
        let invalid = ReplayLog::<2> {
            events: [
                ReplayEvent {
                    opcode: 5,
                    argument: 0,
                },
                ReplayEvent::default(),
            ],
            seed: 5,
            len: 1,
            kind: ReplayKind::Echo,
        };
        let Err(error) = replay_echo(&invalid) else {
            panic!("semantically invalid replay was accepted");
        };
        assert_eq!(error, ReplayError::InvalidEvent { index: 0 });
    }

    #[test]
    fn caller_owned_scratch_writes_through_storage_seam() {
        let mut log = ReplayLog::<4>::new(ReplayKind::Tidal, 11);
        let model = TideModel::new(11);
        log.record_tide(first_tide_place(&model)).unwrap();
        let mut scratch = [0_u8; 32];
        let mut storage = MemoryStorage::new();
        let len = log.encode_into(&mut scratch).unwrap();
        storage.write("afterhours", &scratch[..len]);
        let stored = storage.read("afterhours").unwrap();
        assert_eq!(stored.as_slice(), &scratch[..len]);
        assert!(ReplayLog::<4>::decode(&stored, ReplayKind::Tidal).is_ok());
    }

    #[test]
    fn fixed_log_reports_capacity_and_exact_scratch_requirement() {
        let mut log = ReplayLog::<1>::new(ReplayKind::Echo, 1);
        log.record_echo(EchoCommand::Step(Direction::Wait)).unwrap();
        assert_eq!(
            log.record_echo(EchoCommand::Step(Direction::Wait)),
            Err(ReplayError::Full)
        );
        let mut short = [0_u8; HEADER_LEN + TRAILER_LEN + 1];
        assert_eq!(
            log.encode_into(&mut short),
            Err(ReplayError::BufferTooSmall {
                required: HEADER_LEN + TRAILER_LEN + 2,
                available: HEADER_LEN + TRAILER_LEN + 1,
            })
        );
        assert!(core::mem::size_of::<ReplayLog<64>>() <= 144);
    }
}
