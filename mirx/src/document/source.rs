use alloc::{borrow::Cow, vec::Vec};

/// A validated half-open byte range inside one contiguous byte backing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SourceRange {
    start: usize,
    end: usize,
}

impl SourceRange {
    pub(super) const fn checked(start: usize, len: usize, source_len: usize) -> Option<Self> {
        match start.checked_add(len) {
            Some(end) if end <= source_len => Some(Self { start, end }),
            _ => None,
        }
    }

    pub(super) fn get(self, source: &[u8]) -> Option<&[u8]> {
        source.get(self.start..self.end)
    }

    pub(super) const fn start(self) -> usize {
        self.start
    }
}

/// Ownership of the exact source supplied when a document is opened.
pub(super) enum Origin<'a> {
    New,
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> Origin<'a> {
    pub(super) fn source(&self) -> Option<&[u8]> {
        match self {
            Self::New => None,
            Self::Borrowed(source) => Some(source),
            Self::Owned(source) => Some(source),
        }
    }

    pub(super) fn resolve(&self, range: SourceRange) -> Option<&[u8]> {
        range.get(self.source()?)
    }

    pub(super) fn into_cow(self) -> Option<Cow<'a, [u8]>> {
        match self {
            Self::New => None,
            Self::Borrowed(source) => Some(Cow::Borrowed(source)),
            Self::Owned(source) => Some(Cow::Owned(source)),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn checked_ranges_accept_boundaries_and_reject_escape() {
        let source = [10, 20, 30, 40];

        let full = SourceRange::checked(0, source.len(), source.len()).unwrap();
        assert_eq!(full.get(&source), Some(source.as_slice()));

        let middle = SourceRange::checked(1, 2, source.len()).unwrap();
        assert_eq!(middle.get(&source), Some(&source[1..3]));

        let empty_at_end = SourceRange::checked(source.len(), 0, source.len()).unwrap();
        assert_eq!(empty_at_end.get(&source), Some(&source[source.len()..]));

        assert_eq!(
            SourceRange::checked(source.len() + 1, 0, source.len()),
            None
        );
        assert_eq!(SourceRange::checked(3, 2, source.len()), None);
        assert_eq!(SourceRange::checked(usize::MAX, 1, usize::MAX), None);
    }

    #[test]
    fn borrowed_and_owned_origins_resolve_the_same_ranges() {
        let borrowed_bytes = [1, 2, 3, 4, 5];
        let owned_bytes = vec![1, 2, 3, 4, 5];
        let range = SourceRange::checked(1, 3, borrowed_bytes.len()).unwrap();

        let borrowed = Origin::Borrowed(&borrowed_bytes);
        let owned = Origin::Owned(owned_bytes);
        assert_eq!(borrowed.resolve(range), Some(&borrowed_bytes[1..4]));
        assert_eq!(owned.resolve(range), Some(&borrowed_bytes[1..4]));
        assert_eq!(Origin::New.resolve(range), None);
    }
}
