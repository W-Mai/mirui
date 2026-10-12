use super::*;
use alloc::vec;

const BOOL: Schema = Schema::Scalar(Scalar::Bool);
const BYTE: Schema = Schema::Scalar(Scalar::U8);
const TEXT: Schema = Schema::Scalar(Scalar::String);
const SETTINGS: Schema = Schema::Record(&[
    Field {
        name: "dark_mode",
        schema: &BOOL,
    },
    Field {
        name: "volume",
        schema: &BYTE,
    },
]);
const TODO: Schema = Schema::Record(&[
    Field {
        name: "done",
        schema: &BOOL,
    },
    Field {
        name: "title",
        schema: &TEXT,
    },
]);

fn settings(output: &mut Vec<u8>) {
    encode(&SETTINGS, output, |writer| {
        writer.sized(|writer| writer.bytes(&[1]))?;
        writer.sized(|writer| writer.bytes(&[80]))
    })
    .unwrap();
}

fn raw(schema: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = Writer { output: &mut bytes };
    writer.bytes(MAGIC).unwrap();
    writer.sized(|writer| writer.bytes(schema)).unwrap();
    writer.sized(|writer| writer.bytes(value)).unwrap();
    bytes
}

#[test]
fn fields_match_by_name_and_archive_type_not_position() {
    let mut bytes = Vec::new();
    settings(&mut bytes);
    assert_eq!(bytes.len(), 56);
    let root = Archive::parse(&bytes).unwrap().root();
    let volume = root.field("volume").unwrap().unwrap();
    assert!(volume.matches(&BYTE));
    assert!(!volume.matches(&BOOL));
    assert_eq!(volume.bytes(), &[80]);
    assert_eq!(root.field("dark_mode").unwrap().unwrap().bytes(), &[1]);
    assert!(root.field("new_field").unwrap().is_none());
    assert!(root.matches(&Schema::Record(&[])));
    assert!(!root.matches(&Schema::Sequence(&BYTE)));
}

#[test]
fn homogeneous_members_share_schema_and_reuse_encoding_storage() {
    const ROOT: Schema = Schema::Record(&[Field {
        name: "items",
        schema: &Schema::Sequence(&TODO),
    }]);
    let mut bytes = Vec::new();
    let mut save = || {
        encode(&ROOT, &mut bytes, |writer| {
            writer.sized(|writer| {
                writer.count(100)?;
                for _ in 0..100 {
                    writer.sized(|writer| {
                        writer.sized(|writer| writer.bytes(&[0]))?;
                        writer.sized(|writer| writer.bytes(b"hello"))
                    })?;
                }
                Ok(())
            })
        })
        .unwrap();
    };
    save();
    assert_eq!(crate::test_support::tracked_allocations(&mut save), 0);
    assert_eq!(bytes.len(), 1869);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let root = Archive::parse(&bytes).unwrap().root();
            assert!(
                root.field("items")
                    .unwrap()
                    .unwrap()
                    .matches(&Schema::Sequence(&TODO))
            );
        }),
        0
    );
}

#[test]
fn empty_values_retain_container_and_element_types() {
    let mut bytes = Vec::new();
    let cases = [
        (Schema::Option(&BOOL), Schema::Option(&BYTE), &[0][..]),
        (
            Schema::Sequence(&BOOL),
            Schema::Sequence(&BYTE),
            &[0; 4][..],
        ),
        (Schema::Set(&BOOL), Schema::Set(&BYTE), &[0; 4][..]),
        (
            Schema::Array {
                len: 0,
                item: &BOOL,
            },
            Schema::Array {
                len: 0,
                item: &BYTE,
            },
            &[][..],
        ),
        (
            Schema::Map {
                key: &TEXT,
                value: &BOOL,
            },
            Schema::Map {
                key: &TEXT,
                value: &BYTE,
            },
            &[0; 4][..],
        ),
    ];
    for (schema, different, value) in cases {
        encode(&schema, &mut bytes, |writer| writer.bytes(value)).unwrap();
        let root = Archive::parse(&bytes).unwrap().root();
        assert!(root.matches(&schema));
        assert!(!root.matches(&different));
        assert!(!root.matches(&BYTE));
    }
}

#[test]
fn validation_rejects_bad_values_even_in_unrequested_fields() {
    let mut bytes = Vec::new();
    settings(&mut bytes);
    let value = bytes.len() - 10;
    bytes[value + 4] = 2;
    assert!(Archive::parse(&bytes).is_err());
    settings(&mut bytes);
    for len in 0..bytes.len() {
        assert!(Archive::parse(&bytes[..len]).is_err());
    }
    bytes.push(0);
    assert!(Archive::parse(&bytes).is_err());
}

#[test]
fn record_names_are_unique_ordered_utf8() {
    for names in [["a", "a"], ["b", "a"]] {
        let mut schema = vec![RECORD, 2, 0, 0, 0];
        for name in names {
            schema.extend_from_slice(&[1, 0]);
            schema.extend_from_slice(name.as_bytes());
            schema.extend_from_slice(&[1, 0, 0, 0, Scalar::Bool as u8]);
        }
        assert!(Archive::parse(&raw(&schema, &[1, 0, 0, 0, 0, 1, 0, 0, 0, 1])).is_err());
    }
    let schema = [
        RECORD,
        1,
        0,
        0,
        0,
        1,
        0,
        255,
        1,
        0,
        0,
        0,
        Scalar::Bool as u8,
    ];
    assert!(Archive::parse(&raw(&schema, &[1, 0, 0, 0, 1])).is_err());
}

#[test]
fn invalid_scalar_encodings_and_unknown_tags_are_rejected() {
    let invalid = [
        (Scalar::Bool, &[2][..]),
        (Scalar::Unit, &[0][..]),
        (Scalar::U16, &[0][..]),
        (Scalar::Char, &[0, 216, 0, 0][..]),
        (Scalar::String, &[255][..]),
    ];
    for (scalar, bytes) in invalid {
        assert!(Archive::parse(&raw(&[scalar as u8], bytes)).is_err());
    }
    assert!(Archive::parse(&raw(&[255], &[])).is_err());
    assert!(Archive::parse(&raw(&[Scalar::U8 as u8, 0], &[1])).is_err());
    let nan = f64::NAN.to_bits().to_le_bytes();
    let bytes = raw(&[Scalar::F64 as u8], &nan);
    assert_eq!(Archive::parse(&bytes).unwrap().root().bytes(), nan);
}

#[test]
fn impossible_counts_and_nested_trailing_data_are_rejected() {
    let mut bytes = Vec::new();
    for schema in [
        Schema::Sequence(&BOOL),
        Schema::Set(&BOOL),
        Schema::Map {
            key: &BYTE,
            value: &BOOL,
        },
    ] {
        let mut schema_bytes = Vec::new();
        Writer {
            output: &mut schema_bytes,
        }
        .schema(&schema, 1)
        .unwrap();
        assert!(Archive::parse(&raw(&schema_bytes, &u32::MAX.to_le_bytes())).is_err());
    }
    for value in [&[0, 0][..], &[2][..], &[1, 1, 0][..]] {
        assert!(
            encode(&Schema::Option(&BOOL), &mut bytes, |writer| writer
                .bytes(value))
            .is_err()
        );
        assert!(bytes.is_empty());
    }
    assert!(Archive::parse(&raw(&[RECORD, 255, 255, 255, 255], &[])).is_err());
}

#[test]
fn depth_limit_applies_to_schema_even_when_values_are_empty() {
    let mut schema = vec![Scalar::Bool as u8];
    let mut value = vec![1];
    for depth in 1..=MAX_DEPTH {
        assert!(Archive::parse(&raw(&schema, &value)).is_ok());
        let mut next = vec![OPTION];
        next.extend_from_slice(&(schema.len() as u32).to_le_bytes());
        next.extend_from_slice(&schema);
        schema = next;
        value.insert(0, 1);
        if depth == MAX_DEPTH {
            assert!(Archive::parse(&raw(&schema, &value)).is_err());
            assert!(Archive::parse(&raw(&schema, &[0])).is_err());
        }
    }
    static CYCLE: Schema = Schema::Option(&CYCLE);
    let mut bytes = Vec::new();
    assert!(encode(&CYCLE, &mut bytes, |writer| writer.bytes(&[0])).is_err());
    assert!(bytes.is_empty());
}

#[test]
fn failed_encoding_clears_partial_record_but_retains_capacity() {
    let mut bytes = Vec::with_capacity(128);
    let capacity = bytes.capacity();
    let error = encode(&SETTINGS, &mut bytes, |writer| {
        writer.sized(|writer| writer.bytes(&[1]))?;
        Err(PersistenceError::Encoding)
    });
    assert_eq!(error, Err(PersistenceError::Encoding));
    assert!(bytes.is_empty());
    assert_eq!(bytes.capacity(), capacity);
    settings(&mut bytes);
    assert!(Archive::parse(&bytes).is_ok());
}
