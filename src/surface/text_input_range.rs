use core::ops::Range;

pub(super) fn utf8_selection(text: &str, range: Range<usize>, utf16: bool) -> Option<Range<usize>> {
    let mut units = 0;
    let mut first = None;
    for (byte, character) in text.char_indices() {
        if units == range.start {
            first = Some(byte);
        }
        if units == range.end {
            return Some(first?..byte);
        }
        units += if utf16 { character.len_utf16() } else { 1 };
    }
    if units == range.start {
        first = Some(text.len());
    }
    (units == range.end)
        .then(|| first.map(|first| first..text.len()))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_preserve_unicode_boundaries_without_allocating() {
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert_eq!(utf8_selection("a🙂你z", 1..3, true), Some(1..5));
                assert_eq!(utf8_selection("a🙂你z", 1..3, false), Some(1..8));
                assert_eq!(utf8_selection("a🙂你z", 2..2, true), None);
                assert_eq!(utf8_selection("a🙂你z", 5..5, true), Some(9..9));
                assert_eq!(
                    utf8_selection("a🙂你z", Range { start: 3, end: 1 }, true),
                    None
                );
                assert_eq!(utf8_selection("a🙂你z", 0..6, true), None);
                assert_eq!(utf8_selection("", 0..0, true), Some(0..0));
            }),
            0
        );
    }
}
