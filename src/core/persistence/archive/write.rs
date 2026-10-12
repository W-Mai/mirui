use super::{
    ARRAY, Field, MAP, MAX_DEPTH, OPTION, PersistenceError, RECORD, Result, SEQUENCE, SET, Schema,
};
use alloc::vec::Vec;

/// Fallible writes into the persistence service's retained encoding buffer.
pub struct Writer<'a> {
    pub(super) output: &'a mut Vec<u8>,
}

impl Writer<'_> {
    pub fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.output
            .try_reserve(bytes.len())
            .map_err(|_| PersistenceError::Capacity)?;
        self.output.extend_from_slice(bytes);
        Ok(())
    }

    pub fn count(&mut self, count: usize) -> Result<()> {
        let count = u32::try_from(count).map_err(|_| PersistenceError::Encoding)?;
        self.bytes(&count.to_le_bytes())
    }

    pub fn sized(&mut self, encode: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let start = self.output.len();
        let result = (|| {
            self.bytes(&[0; 4])?;
            encode(self)?;
            let len = u32::try_from(self.output.len() - start - 4)
                .map_err(|_| PersistenceError::Encoding)?;
            self.output[start..start + 4].copy_from_slice(&len.to_le_bytes());
            Ok(())
        })();
        if result.is_err() {
            self.output.truncate(start);
        }
        result
    }

    pub(super) fn schema(&mut self, schema: &Schema, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(PersistenceError::Encoding);
        }
        match schema {
            Schema::Scalar(scalar) => self.bytes(&[*scalar as u8]),
            Schema::Record(fields) => {
                self.bytes(&[RECORD])?;
                self.count(fields.len())?;
                let mut previous = None;
                for Field { name, schema } in *fields {
                    if previous.is_some_and(|previous| previous >= *name) {
                        return Err(PersistenceError::Encoding);
                    }
                    previous = Some(*name);
                    let len = u16::try_from(name.len()).map_err(|_| PersistenceError::Encoding)?;
                    self.bytes(&len.to_le_bytes())?;
                    self.bytes(name.as_bytes())?;
                    self.sized(|writer| writer.schema(schema, depth + 1))?;
                }
                Ok(())
            }
            Schema::Option(item) | Schema::Sequence(item) | Schema::Set(item) => {
                let tag = match schema {
                    Schema::Option(_) => OPTION,
                    Schema::Sequence(_) => SEQUENCE,
                    _ => SET,
                };
                self.bytes(&[tag])?;
                self.sized(|writer| writer.schema(item, depth + 1))
            }
            Schema::Array { len, item } => {
                self.bytes(&[ARRAY])?;
                self.bytes(&len.to_le_bytes())?;
                self.sized(|writer| writer.schema(item, depth + 1))
            }
            Schema::Map { key, value } => {
                self.bytes(&[MAP])?;
                self.sized(|writer| writer.schema(key, depth + 1))?;
                self.sized(|writer| writer.schema(value, depth + 1))
            }
        }
    }
}
