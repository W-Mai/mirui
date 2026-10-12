use alloc::vec::Vec;
use core::ops::Range;

use super::{TextEditError, TextEditing, TextSelection};

#[derive(Clone, Default)]
pub(super) struct Preedit {
    display: Vec<u8>,
    explicit_capacity: Option<usize>,
    capacity: usize,
    active: bool,
    replaced: Range<usize>,
    inserted: usize,
    selection: Option<TextSelection>,
}

impl Preedit {
    fn capacity_for(&self, document: usize) -> Result<usize, TextEditError> {
        self.explicit_capacity
            .or_else(|| document.checked_mul(2))
            .ok_or(TextEditError::CapacityLimit {
                capacity: document,
                maximum: usize::MAX / 2,
            })
    }

    fn prepare(&mut self, document: usize) -> Result<(), TextEditError> {
        let capacity = self.capacity_for(document)?;
        let maximum = (u32::MAX as usize).min(isize::MAX as usize);
        let total = document
            .checked_add(capacity)
            .filter(|total| *total <= maximum)
            .ok_or(TextEditError::CapacityLimit {
                capacity,
                maximum: maximum.saturating_sub(document),
            })?;
        if total > self.display.capacity() {
            self.display
                .try_reserve_exact(total.saturating_sub(self.display.len()))
                .map_err(|_| TextEditError::ReserveFailed { capacity: total })?;
        }
        self.capacity = capacity;
        Ok(())
    }

    fn update(
        &mut self,
        body: &str,
        replaced: Range<usize>,
        text: &str,
        selection: Option<Range<usize>>,
    ) -> Result<(), TextEditError> {
        if text.len() > self.capacity {
            return Err(TextEditError::Capacity {
                required: text.len(),
                capacity: self.capacity,
            });
        }
        if let Some(range) = &selection {
            if range.start > range.end {
                return Err(TextEditError::InvalidRange {
                    start: range.start,
                    end: range.end,
                });
            }
            for offset in [range.start, range.end] {
                if !text.is_char_boundary(offset) {
                    return Err(TextEditError::InvalidBoundary { offset });
                }
            }
        }
        if text.chars().any(char::is_control) {
            return Err(TextEditError::ControlCharacter);
        }
        self.display.clear();
        self.display
            .extend_from_slice(&body.as_bytes()[..replaced.start]);
        self.display.extend_from_slice(text.as_bytes());
        self.display
            .extend_from_slice(&body.as_bytes()[replaced.end..]);
        self.selection = selection.map(|range| TextSelection {
            anchor: replaced.start + range.start,
            cursor: replaced.start + range.end,
        });
        self.replaced = replaced;
        self.inserted = text.len();
        self.active = true;
        Ok(())
    }
}

impl TextEditing {
    /// Override the preedit byte limit independently of the document capacity.
    pub fn with_preedit_capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        self.preedit.explicit_capacity = Some(capacity);
        self.prepare_preedit()?;
        Ok(self)
    }

    /// The byte limit for uncommitted text, defaulting to twice the document capacity.
    pub fn preedit_capacity(&self) -> usize {
        self.preedit.capacity_for(self.capacity()).unwrap_or(0)
    }

    pub(crate) fn prepare_preedit(&mut self) -> Result<(), TextEditError> {
        self.preedit.prepare(self.capacity())
    }

    pub(crate) fn update_preedit(
        &mut self,
        text: &str,
        selection: Option<Range<usize>>,
    ) -> Result<(), TextEditError> {
        if let Err(error) = self.prepare_preedit() {
            self.clear_preedit();
            return Err(error);
        }
        let replaced = self.selection().range();
        let result = match &self.document {
            Some(document) => {
                document.read(|body| self.preedit.update(body, replaced, text, selection))
            }
            None => self
                .local
                .as_ref()
                .unwrap()
                .read(|body| self.preedit.update(body, replaced, text, selection)),
        };
        if result.is_err() {
            self.clear_preedit();
        }
        result
    }

    pub(crate) fn clear_preedit(&mut self) -> bool {
        let active = self.preedit.active;
        self.preedit.active = false;
        self.preedit.display.clear();
        self.preedit.selection = None;
        active
    }

    pub(crate) fn read_display<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        if self.preedit.active {
            read(core::str::from_utf8(&self.preedit.display).expect("validated preedit is UTF-8"))
        } else {
            self.read(read)
        }
    }

    pub(crate) fn display_cursor(&self) -> Option<usize> {
        if self.preedit.active {
            self.preedit.selection.map(|selection| selection.cursor)
        } else {
            Some(self.cursor())
        }
    }

    pub(crate) fn display_selection(&self) -> Range<usize> {
        if self.preedit.active {
            self.preedit.selection.unwrap_or_default().range()
        } else {
            self.selection().range()
        }
    }

    pub(crate) fn preedit_range(&self) -> Option<Range<usize>> {
        self.preedit.active.then(|| {
            self.preedit.replaced.start..self.preedit.replaced.start + self.preedit.inserted
        })
    }

    pub(crate) fn document_offset(&self, display_offset: usize) -> usize {
        if !self.preedit.active || display_offset <= self.preedit.replaced.start {
            return display_offset;
        }
        let end = self.preedit.replaced.start + self.preedit.inserted;
        if display_offset <= end {
            self.preedit.replaced.end
        } else {
            display_offset - self.preedit.inserted + self.preedit.replaced.len()
        }
    }
}
