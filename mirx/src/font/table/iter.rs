use super::{RepresentationRecord, RepresentationTable};
use core::iter::FusedIterator;

/// Exact-size record iteration with direct bidirectional skips.
#[derive(Clone, Debug)]
pub struct RepresentationIter<'a> {
    table: RepresentationTable<'a>,
    front: usize,
    back: usize,
}

impl<'a> RepresentationIter<'a> {
    pub(super) const fn new(table: RepresentationTable<'a>) -> Self {
        Self {
            table,
            front: 0,
            back: table.len(),
        }
    }
}

impl Iterator for RepresentationIter<'_> {
    type Item = RepresentationRecord;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.table.get(index)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
    fn count(self) -> usize {
        self.len()
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for RepresentationIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.table.get(self.back)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n;
        self.next_back()
    }
}

impl ExactSizeIterator for RepresentationIter<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for RepresentationIter<'_> {}
