use core::iter::FusedIterator;

use crate::types::Color;

/// Borrowed view over ordered straight-alpha RGBA colors.
///
/// Each color occupies four bytes in `[r, g, b, a]` order. Iteration decodes
/// colors by value without allocating or changing the color channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorTableView<'a> {
    rgba: &'a [u8],
}

impl<'a> ColorTableView<'a> {
    pub(crate) fn from_rgba_bytes(rgba: &'a [u8]) -> Option<Self> {
        rgba.chunks_exact(4)
            .remainder()
            .is_empty()
            .then_some(Self { rgba })
    }

    /// Returns the number of colors in the table.
    pub const fn len(&self) -> usize {
        self.rgba.len() / 4
    }

    /// Returns whether the table contains no colors.
    pub const fn is_empty(&self) -> bool {
        self.rgba.is_empty()
    }

    /// Returns the exact ordered straight-alpha RGBA bytes.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.rgba
    }

    /// Decodes the color at `index` without allocating.
    pub fn get(&self, index: usize) -> Option<Color> {
        let start = index.checked_mul(4)?;
        let end = start.checked_add(4)?;
        self.rgba.get(start..end).map(decode_rgba)
    }

    /// Iterates over the colors in wire order without allocating.
    pub fn iter(&self) -> ColorTableIter<'a> {
        ColorTableIter {
            remaining: self.rgba,
        }
    }
}

impl<'a> IntoIterator for ColorTableView<'a> {
    type Item = Color;
    type IntoIter = ColorTableIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over a borrowed straight-alpha RGBA color table.
#[derive(Clone, Debug)]
pub struct ColorTableIter<'a> {
    remaining: &'a [u8],
}

impl Iterator for ColorTableIter<'_> {
    type Item = Color;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let (rgba, remaining) = self.remaining.split_at(4);
        self.remaining = remaining;
        Some(decode_rgba(rgba))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let Some(offset) = n.checked_mul(4) else {
            self.remaining = &[];
            return None;
        };
        if offset >= self.remaining.len() {
            self.remaining = &[];
            return None;
        }
        self.remaining = &self.remaining[offset..];
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining.len() / 4;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for ColorTableIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let split = self.remaining.len() - 4;
        let (remaining, rgba) = self.remaining.split_at(split);
        self.remaining = remaining;
        Some(decode_rgba(rgba))
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.remaining = &[];
            return None;
        }
        let retained = self.remaining.len() - n * 4;
        self.remaining = &self.remaining[..retained];
        self.next_back()
    }
}

impl ExactSizeIterator for ColorTableIter<'_> {}
impl FusedIterator for ColorTableIter<'_> {}

fn decode_rgba(rgba: &[u8]) -> Color {
    debug_assert_eq!(rgba.len(), 4);
    Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

#[cfg(test)]
mod tests {
    use core::mem::{needs_drop, size_of};

    use super::*;

    #[test]
    fn view_preserves_bytes_order_duplicates_and_straight_alpha() {
        let rgba = [
            0x10, 0x20, 0x30, 0x40, 0xaa, 0xbb, 0xcc, 0xdd, 0x10, 0x20, 0x30, 0x40,
        ];
        let view = ColorTableView::from_rgba_bytes(&rgba).unwrap();

        assert_eq!(view.len(), 3);
        assert!(!view.is_empty());
        assert_eq!(view.as_bytes(), rgba);
        assert_eq!(view.as_bytes().as_ptr(), rgba.as_ptr());
        assert_eq!(view.get(0), Some(Color::rgba(0x10, 0x20, 0x30, 0x40)));
        assert_eq!(view.get(1), Some(Color::rgba(0xaa, 0xbb, 0xcc, 0xdd)));
        assert_eq!(view.get(2), view.get(0));
        assert_eq!(view.get(usize::MAX), None);
        assert_eq!(view.get(3), None);

        let mut colors = view.iter();
        assert_eq!(colors.len(), 3);
        assert_eq!(colors.next(), view.get(0));
        assert_eq!(colors.next_back(), view.get(2));
        assert_eq!(colors.next(), view.get(1));
        assert_eq!(colors.next(), None);
        assert_eq!(colors.next_back(), None);
        assert_eq!(colors.next(), None);

        let mut first = view.iter();
        let mut second = first.clone();
        assert_eq!(first.next(), view.get(0));
        assert_eq!(first.nth_back(1), view.get(1));
        assert_eq!(second.nth(1), view.get(1));
        assert_eq!(second.next_back(), view.get(2));

        assert!(!needs_drop::<ColorTableView<'_>>());
        assert!(size_of::<ColorTableView<'_>>() <= 16);
        assert!(!needs_drop::<ColorTableIter<'_>>());
        assert!(size_of::<ColorTableIter<'_>>() <= 16);
    }

    #[test]
    fn constructor_rejects_partial_colors_and_accepts_empty_tables() {
        assert_eq!(ColorTableView::from_rgba_bytes(&[1, 2, 3]), None);
        let empty = ColorTableView::from_rgba_bytes(&[]).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.iter().len(), 0);
    }
}
