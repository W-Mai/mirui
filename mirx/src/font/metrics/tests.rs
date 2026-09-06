use super::*;

fn wire_fixed(bits: i32) -> Fixed {
    Fixed::from_le_bytes(bits.to_le_bytes())
}

#[test]
fn signed_fixed_records_preserve_precision_and_declared_line_spacing() {
    let glyph = GlyphMetrics::new(wire_fixed(-1), wire_fixed(i32::MIN), wire_fixed(i32::MAX));
    let mut bytes = [0x5a; GLYPH_METRICS_LEN + 1];
    assert_eq!(glyph.encode_record_into(&mut bytes), Ok(GLYPH_METRICS_LEN));
    assert_eq!(
        bytes,
        [255, 255, 255, 255, 0, 0, 0, 128, 255, 255, 255, 127, 0x5a]
    );
    assert_eq!(GlyphMetrics::from_record(&bytes), Ok(glyph));
    assert_eq!(glyph.advance(), wire_fixed(-1));
    assert_eq!(glyph.bearing_x(), wire_fixed(i32::MIN));
    assert_eq!(glyph.bearing_y(), wire_fixed(i32::MAX));
    for values in [[0, 0, 1], [i32::MAX, i32::MIN, 1], [2560, -768, 4096]] {
        let line = LineMetrics::new(
            wire_fixed(values[0]),
            wire_fixed(values[1]),
            wire_fixed(values[2]),
        )
        .unwrap();
        assert_eq!(line.ascent(), wire_fixed(values[0]));
        assert_eq!(line.descent(), wire_fixed(values[1]));
        assert_eq!(line.line_height(), wire_fixed(values[2]));
        let mut bytes = [0x5a; LINE_METRICS_LEN + 1];
        line.encode_record_into(&mut bytes).unwrap();
        for (i, field) in values.into_iter().enumerate() {
            assert_eq!(&bytes[i * 4..i * 4 + 4], field.to_le_bytes());
        }
        assert_eq!(bytes[LINE_METRICS_LEN], 0x5a);
        assert_eq!(LineMetrics::from_record(&bytes), Ok(line));
    }
}

#[test]
fn validation_precedes_writes_and_accepts_only_complete_tables() {
    let valid = LineMetrics::new(wire_fixed(10), wire_fixed(-3), wire_fixed(12)).unwrap();
    for (values, error) in [
        ([-1, 0, 1], MetricsError::NegativeAscent(wire_fixed(-1))),
        ([1, 1, 1], MetricsError::PositiveDescent(wire_fixed(1))),
        (
            [0, 0, 0],
            MetricsError::NonPositiveLineHeight(wire_fixed(0)),
        ),
        (
            [1, -1, -1],
            MetricsError::NonPositiveLineHeight(wire_fixed(-1)),
        ),
    ] {
        let mut bytes = [0; LINE_METRICS_LEN];
        for (i, field) in values.into_iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&i32::to_le_bytes(field));
        }
        assert_eq!(
            LineMetrics::new(
                wire_fixed(values[0]),
                wire_fixed(values[1]),
                wire_fixed(values[2])
            ),
            Err(error)
        );
        assert_eq!(LineMetrics::from_record(&bytes), Err(error));
        assert_eq!(MetricsTable::open(&bytes), Err(error));
    }
    for size in 0..LINE_METRICS_LEN {
        let mut bytes = [0x5a; LINE_METRICS_LEN];
        assert_eq!(
            valid.encode_record_into(&mut bytes[..size]),
            Err(MetricsError::BufferTooSmall {
                needed: LINE_METRICS_LEN,
                available: size,
            })
        );
        assert!(
            GlyphMetrics::default()
                .encode_record_into(&mut bytes[..size])
                .is_err()
        );
        assert_eq!(bytes, [0x5a; LINE_METRICS_LEN]);
        assert!(LineMetrics::from_record(&bytes[..size]).is_err());
        assert!(GlyphMetrics::from_record(&bytes[..size]).is_err());
        assert!(MetricsTable::open(&bytes[..size]).is_err());
    }
    let mut table = [0; LINE_METRICS_LEN + GLYPH_METRICS_LEN];
    valid.encode_record_into(&mut table).unwrap();
    for size in LINE_METRICS_LEN + 1..table.len() {
        assert_eq!(
            MetricsTable::open(&table[..size]),
            Err(MetricsError::PartialRecord { byte_len: size })
        );
    }
    assert!(
        MetricsTable::open(&table[..LINE_METRICS_LEN])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unaligned_tables_share_ordinals_and_read_without_scanning() {
    #[repr(align(4))]
    struct Bytes([u8; 1 + LINE_METRICS_LEN + 6 * GLYPH_METRICS_LEN]);
    let line = LineMetrics::new(wire_fixed(100), wire_fixed(-20), wire_fixed(150)).unwrap();
    let mut bytes = Bytes([0; 1 + LINE_METRICS_LEN + 6 * GLYPH_METRICS_LEN]);
    line.encode_record_into(&mut bytes.0[1..]).unwrap();
    let mut glyphs = [GlyphMetrics::default(); 6];
    for (index, glyph) in glyphs.iter_mut().enumerate() {
        *glyph = GlyphMetrics::new(
            wire_fixed(index as i32),
            wire_fixed(-1000),
            wire_fixed(90000),
        );
        glyph
            .encode_record_into(&mut bytes.0[1 + LINE_METRICS_LEN + index * GLYPH_METRICS_LEN..])
            .unwrap();
    }
    let table = MetricsTable::open(&bytes.0[1..]).unwrap();
    assert_eq!(table.as_bytes().as_ptr() as usize % 4, 1);
    assert_eq!(table.as_bytes().as_ptr(), bytes.0[1..].as_ptr());
    assert_eq!(table.line_metrics(), line);
    assert_eq!(table.len(), glyphs.len());
    assert!(!table.is_empty());
    assert_eq!(table.iter().count(), glyphs.len());
    assert_eq!(table.iter().last(), Some(glyphs[5]));
    for (index, glyph) in glyphs.into_iter().enumerate() {
        assert_eq!(table.get(index), Some(glyph));
    }
    for index in [glyphs.len(), usize::MAX, usize::MAX / GLYPH_METRICS_LEN] {
        assert_eq!(table.get(index), None);
    }
    let mut iter = table.into_iter();
    assert_eq!(iter.next(), Some(glyphs[0]));
    assert_eq!(iter.next_back(), Some(glyphs[5]));
    assert_eq!(iter.nth(1), Some(glyphs[2]));
    assert_eq!(iter.nth_back(1), Some(glyphs[3]));
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    for reverse in [false, true] {
        let mut iter = table.iter();
        assert_eq!(
            if reverse {
                iter.nth_back(usize::MAX)
            } else {
                iter.nth(usize::MAX)
            },
            None
        );
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }
}
