use mirx::{
    ChunkFlags, ChunkId, ChunkType, CriticalAssumption, Document, PayloadInput, PrimaryHints,
    RawChunkInput, RawChunkPolicy, Reader, RelocationAssumption, ReservedBitsPolicy, parse_chunk,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedChunk {
    id: ChunkId,
    chunk_type: ChunkType,
    payload: Vec<u8>,
}

const fn relocatable_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::Infer,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn next_random(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

fn generated_payload(state: &mut u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|_| next_random(state).to_le_bytes()[2])
        .collect()
}

#[test]
fn generated_raw_sequences_rewrite_deterministically_and_reopen_every_payload() {
    for seed in 0..128u32 {
        let mut state = seed ^ 0x9e37_79b9;
        let count = (next_random(&mut state) as usize % 15) + 1;
        let mut document = Document::new_chunk();
        let mut expected = Vec::with_capacity(count);

        for index in 0..count {
            let chunk_type = ChunkType::new(0x8000 + index as u16).unwrap();
            let payload_len = next_random(&mut state) as usize % 33;
            let payload = generated_payload(&mut state, payload_len);
            let id = document
                .push_raw(RawChunkInput {
                    chunk_type,
                    flags: ChunkFlags::NONE,
                    payload: PayloadInput::Owned(payload.clone()),
                    policy: relocatable_policy(),
                })
                .unwrap();
            expected.push(ExpectedChunk {
                id,
                chunk_type,
                payload,
            });
        }

        if expected.len() > 1 {
            let moved = expected.pop().unwrap();
            document.move_before(moved.id, expected[0].id).unwrap();
            expected.insert(0, moved);
        }
        if expected.len() > 2 {
            let removed = expected.remove(1);
            document.remove(removed.id).unwrap();
        }
        let replacement = generated_payload(&mut state, 19);
        document
            .replace_raw(
                expected[0].id,
                PayloadInput::Owned(replacement.clone()),
                relocatable_policy(),
            )
            .unwrap();
        expected[0].payload = replacement;

        let primary = expected.last().unwrap();
        let hints = PrimaryHints::new(0xff, seed + 1, count as u32, 0);
        document.set_primary_with_hints(primary.id, hints).unwrap();

        let first = document.encode_with(&Default::default()).unwrap();
        let second = document.encode_with(&Default::default()).unwrap();
        assert_eq!(first, second, "seed {seed}");

        let reader = Reader::open(&first).unwrap();
        assert_eq!(reader.primary_hints(), hints, "seed {seed}");
        assert_eq!(
            reader.primary().unwrap().unwrap().chunk_type(),
            primary.chunk_type,
            "seed {seed}"
        );
        assert!(
            reader
                .chunks()
                .zip(expected.iter())
                .all(|(actual, expected)| {
                    actual.chunk_type() == expected.chunk_type
                        && actual.payload() == expected.payload
                })
        );

        let legacy = parse_chunk(&first).unwrap();
        assert_eq!(legacy.entries.len(), expected.len(), "seed {seed}");
        for expected in &expected {
            assert_eq!(
                legacy.chunk_payload(&first, expected.chunk_type.raw()),
                Some(expected.payload.as_slice()),
                "seed {seed}"
            );
        }
    }
}
