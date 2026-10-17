use alloc::{sync::Arc, vec::Vec};

use crate::ui::clipboard::{ClipboardData, ClipboardError};

pub(super) struct Outgoing {
    snapshot: Arc<ClipboardData>,
    format: usize,
    offset: usize,
    chunk: usize,
    finished: bool,
}

impl Outgoing {
    pub(super) fn new(
        snapshot: Arc<ClipboardData>,
        format: usize,
        chunk: usize,
    ) -> Result<Self, ClipboardError> {
        let [item] = snapshot.items() else {
            return Err(ClipboardError::Unsupported);
        };
        let value = item
            .formats()
            .get(format)
            .ok_or(ClipboardError::InvalidData)?;
        u32::try_from(value.bytes().len()).map_err(|_| ClipboardError::Capacity)?;
        if chunk == 0 {
            return Err(ClipboardError::Capacity);
        }
        Ok(Self {
            snapshot,
            format,
            offset: 0,
            chunk,
            finished: false,
        })
    }

    pub(super) fn length(&self) -> u32 {
        self.bytes().len() as u32
    }

    fn bytes(&self) -> &[u8] {
        self.snapshot.items()[0].formats()[self.format].bytes()
    }

    pub(super) fn next(&mut self) -> Option<&[u8]> {
        if self.finished {
            return None;
        }
        let start = self.offset;
        let length = self.bytes().len();
        self.offset = start.saturating_add(self.chunk).min(length);
        self.finished = start == length;
        Some(&self.bytes()[start..self.offset])
    }
}

pub(super) struct Property {
    pub(super) kind: u32,
    pub(super) format: u8,
    pub(super) bytes: Vec<u8>,
}

#[derive(Default, PartialEq, Eq)]
enum State {
    #[default]
    Reading,
    Complete,
    Failed,
}

#[derive(Default)]
pub(super) struct PropertyReader {
    property: Option<Property>,
    remaining: u32,
    state: State,
}

impl PropertyReader {
    pub(super) fn append(
        &mut self,
        kind: u32,
        format: u8,
        items: u32,
        remaining: u32,
        bytes: &[u8],
    ) -> Result<(), ClipboardError> {
        let result = self.append_inner(kind, format, items, remaining, bytes);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }

    fn append_inner(
        &mut self,
        kind: u32,
        format: u8,
        items: u32,
        remaining: u32,
        bytes: &[u8],
    ) -> Result<(), ClipboardError> {
        if self.state != State::Reading || kind == 0 || !matches!(format, 8 | 16 | 32) {
            return Err(ClipboardError::InvalidData);
        }
        let length = items
            .checked_mul(u32::from(format / 8))
            .ok_or(ClipboardError::Capacity)?;
        if bytes.len() != length as usize
            || !remaining.is_multiple_of(u32::from(format / 8))
            || (remaining != 0 && (length == 0 || length % 4 != 0))
        {
            return Err(ClipboardError::InvalidData);
        }
        let total = length
            .checked_add(remaining)
            .ok_or(ClipboardError::Capacity)?;
        if let Some(property) = &self.property
            && (property.kind != kind || property.format != format || total != self.remaining)
        {
            return Err(ClipboardError::InvalidData);
        }
        let property = self.property.get_or_insert_with(|| Property {
            kind,
            format,
            bytes: Vec::new(),
        });
        property
            .bytes
            .try_reserve(bytes.len())
            .map_err(|_| ClipboardError::Capacity)?;
        property.bytes.extend_from_slice(bytes);
        self.remaining = remaining;
        if remaining == 0 {
            self.state = State::Complete;
        }
        Ok(())
    }

    pub(super) fn next_offset(&self) -> Result<Option<u32>, ClipboardError> {
        if self.state == State::Failed {
            return Err(ClipboardError::InvalidData);
        }
        if self.state == State::Complete {
            return Ok(None);
        }
        let bytes = self.property.as_ref().map_or(0, |p| p.bytes.len());
        u32::try_from(bytes / 4)
            .map(Some)
            .map_err(|_| ClipboardError::Capacity)
    }

    pub(super) fn finish(self) -> Result<Property, ClipboardError> {
        if self.state != State::Complete {
            return Err(ClipboardError::InvalidData);
        }
        self.property.ok_or(ClipboardError::InvalidData)
    }
}

pub(super) struct Incoming {
    lower_bound: u32,
    property: Option<Property>,
    state: State,
}

impl Incoming {
    pub(super) fn new(lower_bound: u32) -> Self {
        Self {
            lower_bound,
            property: None,
            state: State::Reading,
        }
    }

    pub(super) fn append(&mut self, property: Property) -> Result<bool, ClipboardError> {
        let result = self.append_inner(property);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }

    fn append_inner(&mut self, property: Property) -> Result<bool, ClipboardError> {
        if self.state != State::Reading {
            return Err(ClipboardError::InvalidData);
        }
        let empty = property.bytes.is_empty();
        if let Some(current) = &mut self.property {
            if current.kind != property.kind || current.format != property.format {
                return Err(ClipboardError::InvalidData);
            }
            current
                .bytes
                .try_reserve(property.bytes.len())
                .map_err(|_| ClipboardError::Capacity)?;
            current.bytes.extend_from_slice(&property.bytes);
        } else {
            self.property = Some(property);
        }
        if empty {
            let property = self.property.as_ref().unwrap();
            if property.bytes.len() < self.lower_bound as usize {
                return Err(ClipboardError::InvalidData);
            }
            self.state = State::Complete;
        }
        Ok(self.state == State::Complete)
    }

    pub(super) fn finish(self) -> Result<Property, ClipboardError> {
        if self.state != State::Complete {
            return Err(ClipboardError::InvalidData);
        }
        self.property.ok_or(ClipboardError::InvalidData)
    }
}

#[cfg(test)]
#[path = "transfer/tests.rs"]
mod tests;
