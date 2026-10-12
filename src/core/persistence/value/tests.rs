use super::*;
use crate::core::persistence::archive::{self, Archive, Field};
use alloc::{string::ToString, vec};

fn save<T: PersistValue>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    archive::encode(&T::SCHEMA, &mut bytes, |writer| value.encode(writer)).unwrap();
    bytes
}

fn restore<T: PersistValue>(target: &mut T, bytes: &[u8]) -> Result<()> {
    let prepared = target.prepare(Archive::parse(bytes)?.root())?;
    target.apply(prepared);
    Ok(())
}

#[test]
fn scalar_types_round_trip_without_coercion() {
    fn same_type<T>(_: &T, _: &T) {}
    macro_rules! round_trip {
        ($($value:expr),* $(,)?) => {$(
            let original = $value;
            let bytes = save(&original);
            let mut restored = Default::default();
            same_type(&original, &restored);
            restore(&mut restored, &bytes).unwrap();
            assert_eq!(restored, original);
        )*};
    }
    round_trip!(
        (),
        true,
        '界',
        u8::MAX,
        u16::MAX,
        u32::MAX,
        u64::MAX,
        u128::MAX,
        i8::MIN,
        i16::MIN,
        i32::MIN,
        i64::MIN,
        i128::MIN,
        usize::MAX,
        isize::MIN,
        -12.5_f32,
        1.5_f64,
        "saved text".to_string()
    );
    let bytes = save(&1_u32);
    let value = Archive::parse(&bytes).unwrap().root();
    assert_eq!(0_i32.prepare(value), Err(PersistenceError::InvalidArchive));
    assert_eq!(0_f32.prepare(value), Err(PersistenceError::InvalidArchive));
    let bytes = save(&1_usize);
    assert_eq!(
        Archive::parse(&bytes).unwrap().root().bytes(),
        &1_u64.to_le_bytes()
    );
    let nan = f64::from_bits(0xfff8_0000_0000_1234);
    let mut decoded = 0.0_f64;
    restore(&mut decoded, &save(&nan)).unwrap();
    assert_eq!(decoded.to_bits(), nan.to_bits());
}

#[derive(Debug, Default, PartialEq)]
struct Record {
    saved: u8,
    transient: u8,
}

impl PersistValue for Record {
    const SCHEMA: Schema = Schema::Record(&[Field {
        name: "saved",
        schema: &u8::SCHEMA,
    }]);
    type Prepared = Option<u8>;

    fn encode(&self, writer: &mut Writer<'_>) -> Result<()> {
        writer.sized(|writer| self.saved.encode(writer))
    }
    fn prepare(&self, value: Value<'_>) -> Result<Self::Prepared> {
        check::<Self>(value)?;
        prepare_field(&self.saved, value, "saved")
    }
    fn apply(&mut self, prepared: Self::Prepared) {
        if let Some(saved) = prepared {
            self.saved = saved;
        }
    }
}

#[test]
fn field_matching_preserves_missing_and_incompatible_values() {
    let mut current = Record {
        saved: 80,
        transient: 90,
    };
    let mut bytes = Vec::new();
    archive::encode(&Schema::Record(&[]), &mut bytes, |_| Ok(())).unwrap();
    restore(&mut current, &bytes).unwrap();
    assert_eq!(
        current,
        Record {
            saved: 80,
            transient: 90
        }
    );
    const DIFFERENT: Schema = Schema::Record(&[Field {
        name: "saved",
        schema: &bool::SCHEMA,
    }]);
    archive::encode(&DIFFERENT, &mut bytes, |writer| {
        writer.sized(|writer| true.encode(writer))
    })
    .unwrap();
    restore(&mut current, &bytes).unwrap();
    assert_eq!(current.saved, 80);
    restore(
        &mut current,
        &save(&Record {
            saved: 10,
            transient: 1,
        }),
    )
    .unwrap();
    assert_eq!(
        current,
        Record {
            saved: 10,
            transient: 90
        }
    );
}

#[test]
fn option_updates_existing_fields_and_defaults_new_instances() {
    let bytes = save(&Some(Record {
        saved: 42,
        transient: 7,
    }));
    let mut existing = Some(Record {
        saved: 1,
        transient: 99,
    });
    let prepared = existing
        .prepare(Archive::parse(&bytes).unwrap().root())
        .unwrap();
    assert_eq!(existing.as_ref().unwrap().saved, 1);
    existing.apply(prepared);
    assert_eq!(
        existing,
        Some(Record {
            saved: 42,
            transient: 99
        })
    );
    let mut new: Option<Record> = None;
    restore(&mut new, &bytes).unwrap();
    assert_eq!(
        new,
        Some(Record {
            saved: 42,
            transient: 0
        })
    );
    restore(&mut existing, &save(&None::<Record>)).unwrap();
    assert!(existing.is_none());
}

#[test]
fn arrays_prepare_on_stack_and_preserve_nonpersisted_fields() {
    let source = [
        Record {
            saved: 1,
            transient: 0,
        },
        Record {
            saved: 2,
            transient: 0,
        },
    ];
    let bytes = save(&source);
    let mut current = [
        Record {
            saved: 8,
            transient: 10,
        },
        Record {
            saved: 9,
            transient: 20,
        },
    ];
    assert_eq!(
        crate::test_support::tracked_allocations(|| restore(&mut current, &bytes).unwrap()),
        0
    );
    assert_eq!(
        current,
        [
            Record {
                saved: 1,
                transient: 10
            },
            Record {
                saved: 2,
                transient: 20
            }
        ]
    );
    let value = Archive::parse(&bytes).unwrap().root();
    assert!(<[Record; 0]>::default().prepare(value).is_err());
    restore::<[Record; 0]>(&mut [], &save(&[] as &[Record; 0])).unwrap();
}

#[test]
fn sequence_replacement_is_prepared_before_touching_current_data() {
    let bytes = save(&vec!["alpha".to_string(), "beta".to_string()]);
    let mut current = vec!["unchanged".to_string()];
    let prepared = current
        .prepare(Archive::parse(&bytes).unwrap().root())
        .unwrap();
    assert_eq!(current, ["unchanged"]);
    assert_eq!(
        crate::test_support::tracked_allocations(|| current.apply(prepared)),
        0
    );
    assert_eq!(current, ["alpha", "beta"]);
    restore(&mut current, &save(&Vec::<String>::new())).unwrap();
    assert!(current.is_empty());
}

#[test]
fn prepared_strings_can_outlive_the_archive_without_apply_allocation() {
    let mut current = "old".to_string();
    let prepared = {
        let bytes = save(&"replacement".to_string());
        current
            .prepare(Archive::parse(&bytes).unwrap().root())
            .unwrap()
    };
    assert_eq!(current, "old");
    assert_eq!(
        crate::test_support::tracked_allocations(|| current.apply(prepared)),
        0
    );
    assert_eq!(current, "replacement");
}

#[test]
fn invalid_archive_cannot_partially_replace_a_sequence() {
    let mut bytes = save(&vec!["alpha".to_string(), "beta".to_string()]);
    *bytes.last_mut().unwrap() = 255;
    let mut current = vec!["preserved".to_string()];
    assert_eq!(
        restore(&mut current, &bytes),
        Err(PersistenceError::InvalidArchive)
    );
    assert_eq!(current, ["preserved"]);
}

#[test]
fn validated_values_iterator_is_exact_and_fused() {
    let bytes = save(&[3_u8, 4]);
    let mut values = Archive::parse(&bytes).unwrap().root().items().unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values.next().unwrap().bytes(), [3]);
    assert_eq!(values.len(), 1);
    assert_eq!(values.next().unwrap().bytes(), [4]);
    assert_eq!(values.len(), 0);
    assert!(values.next().is_none());
    assert!(values.next().is_none());
}
