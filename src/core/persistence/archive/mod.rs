//! Borrowed, validated field archives used by generated Model persistence code.

use alloc::vec::Vec;

use super::PersistenceError;

mod read;
mod write;

pub use read::{Archive, Value};
pub use write::Writer;

const MAGIC: &[u8; 4] = b"MPS1";
const MAX_DEPTH: usize = 32;

type Result<T> = core::result::Result<T, PersistenceError>;

/// Stable scalar tags and representations, independent of host pointer width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Scalar {
    Unit = 0,
    Bool = 1,
    U8 = 2,
    U16 = 3,
    U32 = 4,
    U64 = 5,
    U128 = 6,
    I8 = 7,
    I16 = 8,
    I32 = 9,
    I64 = 10,
    I128 = 11,
    Usize = 12,
    Isize = 13,
    F32 = 14,
    F64 = 15,
    Char = 16,
    String = 17,
}

impl Scalar {
    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Unit,
            1 => Self::Bool,
            2 => Self::U8,
            3 => Self::U16,
            4 => Self::U32,
            5 => Self::U64,
            6 => Self::U128,
            7 => Self::I8,
            8 => Self::I16,
            9 => Self::I32,
            10 => Self::I64,
            11 => Self::I128,
            12 => Self::Usize,
            13 => Self::Isize,
            14 => Self::F32,
            15 => Self::F64,
            16 => Self::Char,
            17 => Self::String,
            _ => return None,
        })
    }

    fn validate(self, bytes: &[u8]) -> Result<()> {
        let valid = match self {
            Self::Unit => bytes.is_empty(),
            Self::Bool => matches!(bytes, [0 | 1]),
            Self::U8 | Self::I8 => bytes.len() == 1,
            Self::U16 | Self::I16 => bytes.len() == 2,
            Self::U32 | Self::I32 | Self::F32 => bytes.len() == 4,
            Self::U64 | Self::I64 | Self::Usize | Self::Isize | Self::F64 => bytes.len() == 8,
            Self::U128 | Self::I128 => bytes.len() == 16,
            Self::Char => <[u8; 4]>::try_from(bytes)
                .ok()
                .and_then(|bytes| char::from_u32(u32::from_le_bytes(bytes)))
                .is_some(),
            Self::String => core::str::from_utf8(bytes).is_ok(),
        };
        valid.then_some(()).ok_or(PersistenceError::InvalidArchive)
    }
}

/// Static descriptors emitted by Model field codecs.
#[derive(Clone, Copy, Debug)]
pub enum Schema {
    Scalar(Scalar),
    Record(&'static [Field]),
    Option(&'static Schema),
    Sequence(&'static Schema),
    Array {
        len: u32,
        item: &'static Schema,
    },
    Map {
        key: &'static Schema,
        value: &'static Schema,
    },
    Set(&'static Schema),
}

/// Record fields must be ordered by UTF-8 name bytes.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub schema: &'static Schema,
}

const RECORD: u8 = 32;
const OPTION: u8 = 33;
const SEQUENCE: u8 = 34;
const ARRAY: u8 = 35;
const MAP: u8 = 36;
const SET: u8 = 37;

/// Encode into a reusable buffer. Failure discards the entire partial record.
pub fn encode(
    schema: &Schema,
    output: &mut Vec<u8>,
    value: impl FnOnce(&mut Writer<'_>) -> Result<()>,
) -> Result<()> {
    output.clear();
    let result = (|| {
        let mut writer = Writer { output };
        writer.bytes(MAGIC)?;
        writer.sized(|writer| writer.schema(schema, 1))?;
        writer.sized(value)?;
        Archive::parse(writer.output).map_err(|_| PersistenceError::Encoding)?;
        Ok(())
    })();
    if result.is_err() {
        output.clear();
    }
    result
}

#[cfg(test)]
mod tests;
