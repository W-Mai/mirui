use super::{
    ARRAY, MAGIC, MAP, MAX_DEPTH, OPTION, PersistenceError, RECORD, Result, SEQUENCE, SET, Scalar,
    Schema,
};

/// A completely validated archive. All contained views borrow its input bytes.
#[derive(Clone, Copy, Debug)]
pub struct Archive<'a> {
    root: Value<'a>,
}

impl<'a> Archive<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let mut cursor = Cursor(bytes);
        if cursor.take(MAGIC.len())? != MAGIC {
            return Err(PersistenceError::InvalidArchive);
        }
        let schema = cursor.sized()?;
        let bytes = cursor.sized()?;
        cursor.end()?;
        validate_schema(schema, 1)?;
        validate_value(schema, bytes, 1)?;
        Ok(Self {
            root: Value { schema, bytes },
        })
    }

    pub fn root(self) -> Value<'a> {
        self.root
    }
}

/// A value accessible only after validation of the complete archive.
#[derive(Clone, Copy, Debug)]
pub struct Value<'a> {
    schema: &'a [u8],
    bytes: &'a [u8],
}

impl<'a> Value<'a> {
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Match types while allowing record fields to evolve independently.
    pub fn matches(self, expected: &Schema) -> bool {
        compatible(self.schema, expected, 1)
    }

    pub fn field(self, name: &str) -> Result<Option<Self>> {
        let Shape::Record { count, fields } = shape(self.schema)? else {
            return Err(PersistenceError::InvalidArchive);
        };
        let mut fields = Cursor(fields);
        let mut values = Cursor(self.bytes);
        for _ in 0..count {
            let field_name = fields.name()?;
            let schema = fields.sized()?;
            let bytes = values.sized()?;
            match field_name.cmp(name) {
                core::cmp::Ordering::Equal => return Ok(Some(Self { schema, bytes })),
                core::cmp::Ordering::Greater => break,
                core::cmp::Ordering::Less => {}
            }
        }
        Ok(None)
    }

    pub fn option(self) -> Result<Option<Self>> {
        let Shape::Option(schema) = shape(self.schema)? else {
            return Err(PersistenceError::InvalidArchive);
        };
        let mut bytes = Cursor(self.bytes);
        match bytes.byte()? {
            0 => Ok(None),
            1 => Ok(Some(Self {
                schema,
                bytes: bytes.0,
            })),
            _ => Err(PersistenceError::InvalidArchive),
        }
    }

    pub fn items(self) -> Result<Values<'a>> {
        let mut bytes = Cursor(self.bytes);
        let (schema, count) = match shape(self.schema)? {
            Shape::Sequence(schema) | Shape::Set(schema) => (schema, bytes.count()?),
            Shape::Array { len, item } => (item, len),
            _ => return Err(PersistenceError::InvalidArchive),
        };
        Ok(Values {
            schema,
            bytes,
            remaining: count as usize,
        })
    }
}

/// Iterates validated collection values without building a temporary index.
pub struct Values<'a> {
    schema: &'a [u8],
    bytes: Cursor<'a>,
    remaining: usize,
}

impl<'a> Iterator for Values<'a> {
    type Item = Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(Value {
            schema: self.schema,
            bytes: self.bytes.sized().expect("validated archive item"),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for Values<'_> {}
impl core::iter::FusedIterator for Values<'_> {}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let (bytes, rest) = self
            .0
            .split_at_checked(len)
            .ok_or(PersistenceError::InvalidArchive)?;
        self.0 = rest;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn count(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn sized(&mut self) -> Result<&'a [u8]> {
        let len = usize::try_from(self.count()?).map_err(|_| PersistenceError::InvalidArchive)?;
        self.take(len)
    }

    fn name(&mut self) -> Result<&'a str> {
        let bytes = self.take(2)?;
        let len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        core::str::from_utf8(self.take(len)?).map_err(|_| PersistenceError::InvalidArchive)
    }

    fn bounded_count(&self, count: u32, minimum: usize) -> Result<()> {
        if u64::from(count) > (self.0.len() / minimum) as u64 {
            return Err(PersistenceError::InvalidArchive);
        }
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.0
            .is_empty()
            .then_some(())
            .ok_or(PersistenceError::InvalidArchive)
    }
}

enum Shape<'a> {
    Scalar(Scalar),
    Record { count: u32, fields: &'a [u8] },
    Option(&'a [u8]),
    Sequence(&'a [u8]),
    Array { len: u32, item: &'a [u8] },
    Map { key: &'a [u8], value: &'a [u8] },
    Set(&'a [u8]),
}

fn shape(schema: &[u8]) -> Result<Shape<'_>> {
    let mut cursor = Cursor(schema);
    let tag = cursor.byte()?;
    let shape = match tag {
        RECORD => {
            let count = cursor.count()?;
            cursor.bounded_count(count, 7)?;
            return Ok(Shape::Record {
                count,
                fields: cursor.0,
            });
        }
        OPTION => Shape::Option(cursor.sized()?),
        SEQUENCE => Shape::Sequence(cursor.sized()?),
        SET => Shape::Set(cursor.sized()?),
        ARRAY => Shape::Array {
            len: cursor.count()?,
            item: cursor.sized()?,
        },
        MAP => Shape::Map {
            key: cursor.sized()?,
            value: cursor.sized()?,
        },
        _ => Shape::Scalar(Scalar::from_tag(tag).ok_or(PersistenceError::InvalidArchive)?),
    };
    cursor.end()?;
    Ok(shape)
}

fn validate_schema(schema: &[u8], depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(PersistenceError::InvalidArchive);
    }
    match shape(schema)? {
        Shape::Scalar(_) => Ok(()),
        Shape::Record { count, fields } => {
            let mut fields = Cursor(fields);
            let mut previous = None;
            for _ in 0..count {
                let name = fields.name()?;
                if previous.is_some_and(|previous| previous >= name) {
                    return Err(PersistenceError::InvalidArchive);
                }
                previous = Some(name);
                validate_schema(fields.sized()?, depth + 1)?;
            }
            fields.end()
        }
        Shape::Option(item)
        | Shape::Sequence(item)
        | Shape::Set(item)
        | Shape::Array { item, .. } => validate_schema(item, depth + 1),
        Shape::Map { key, value } => {
            validate_schema(key, depth + 1)?;
            validate_schema(value, depth + 1)
        }
    }
}

fn validate_value(schema: &[u8], bytes: &[u8], depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(PersistenceError::InvalidArchive);
    }
    let mut values = Cursor(bytes);
    match shape(schema)? {
        Shape::Scalar(scalar) => return scalar.validate(bytes),
        Shape::Record { count, fields } => {
            values.bounded_count(count, 4)?;
            let mut fields = Cursor(fields);
            for _ in 0..count {
                fields.name()?;
                validate_value(fields.sized()?, values.sized()?, depth + 1)?;
            }
        }
        Shape::Option(item) => match values.byte()? {
            0 => {}
            1 => return validate_value(item, values.0, depth + 1),
            _ => return Err(PersistenceError::InvalidArchive),
        },
        Shape::Sequence(item) | Shape::Set(item) => {
            let count = values.count()?;
            validate_items(&mut values, item, count, depth)?;
        }
        Shape::Array { len, item } => validate_items(&mut values, item, len, depth)?,
        Shape::Map { key, value } => {
            let count = values.count()?;
            values.bounded_count(count, 8)?;
            for _ in 0..count {
                validate_value(key, values.sized()?, depth + 1)?;
                validate_value(value, values.sized()?, depth + 1)?;
            }
        }
    }
    values.end()
}

fn validate_items(values: &mut Cursor<'_>, item: &[u8], count: u32, depth: usize) -> Result<()> {
    values.bounded_count(count, 4)?;
    for _ in 0..count {
        validate_value(item, values.sized()?, depth + 1)?;
    }
    Ok(())
}

fn compatible(schema: &[u8], expected: &Schema, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match (shape(schema), expected) {
        (Ok(Shape::Scalar(actual)), Schema::Scalar(expected)) => actual == *expected,
        (Ok(Shape::Record { .. }), Schema::Record(_)) => true,
        (Ok(Shape::Option(actual)), Schema::Option(expected))
        | (Ok(Shape::Sequence(actual)), Schema::Sequence(expected))
        | (Ok(Shape::Set(actual)), Schema::Set(expected)) => {
            compatible(actual, expected, depth + 1)
        }
        (
            Ok(Shape::Array {
                len: actual_len,
                item: actual,
            }),
            Schema::Array { len, item },
        ) => actual_len == *len && compatible(actual, item, depth + 1),
        (
            Ok(Shape::Map {
                key: actual_key,
                value: actual_value,
            }),
            Schema::Map { key, value },
        ) => compatible(actual_key, key, depth + 1) && compatible(actual_value, value, depth + 1),
        _ => false,
    }
}
