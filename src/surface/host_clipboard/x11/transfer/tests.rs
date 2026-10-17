use super::*;
use crate::ui::clipboard::{ClipboardFormat, ClipboardItem};

fn snapshot(bytes: &[u8]) -> Arc<ClipboardData> {
    let mut item = ClipboardItem::with_capacity(1).unwrap();
    item.push(ClipboardFormat::new("application/x-test", bytes.to_vec()))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, bytes.len()).unwrap();
    data.push(item).unwrap();
    Arc::new(data)
}

fn property(kind: u32, format: u8, bytes: &[u8]) -> Property {
    let mut reader = PropertyReader::default();
    reader
        .append(
            kind,
            format,
            (bytes.len() * 8 / usize::from(format)) as u32,
            0,
            bytes,
        )
        .unwrap();
    reader.finish().unwrap()
}

#[test]
fn accepted_transfers_keep_their_original_snapshot_without_copying_bytes() {
    let mut current = snapshot(&[1, 2, 0, 0, 255]);
    let pointer = current.items()[0].formats()[0].bytes().as_ptr();
    let mut a = Outgoing::new(current.clone(), 0, 2).unwrap();
    let mut b = Outgoing::new(current.clone(), 0, 3).unwrap();
    current = snapshot(&[9]);
    assert_eq!(a.length(), 5);
    assert_eq!(a.next().unwrap().as_ptr(), pointer);
    assert_eq!(b.next().unwrap(), &[1, 2, 0]);
    assert_eq!(a.next().unwrap(), &[0, 0]);
    assert_eq!(b.next().unwrap(), &[0, 255]);
    assert_eq!(a.next().unwrap(), &[255]);
    assert_eq!(a.next().unwrap(), &[]);
    assert!(a.next().is_none());
    assert_eq!(b.next().unwrap(), &[]);
    assert!(b.next().is_none());
    assert_eq!(current.items()[0].formats()[0].bytes(), &[9]);
}

#[test]
fn empty_binary_transfer_has_one_terminator_and_never_invents_padding() {
    let source = snapshot(&[]);
    assert!(Outgoing::new(source.clone(), 0, 0).is_err());
    assert!(Outgoing::new(source.clone(), 1, 4).is_err());
    let mut sender = Outgoing::new(source, 0, 4).unwrap();
    assert_eq!(sender.length(), 0);
    assert_eq!(sender.next(), Some(&[][..]));
    assert_eq!(sender.next(), None);
    let mut receiver = Incoming::new(0);
    assert!(receiver.append(property(7, 8, &[])).unwrap());
    assert!(receiver.finish().unwrap().bytes.is_empty());
}

#[test]
fn property_pagination_uses_four_byte_offsets_and_exact_wire_lengths() {
    for format in [8, 16, 32] {
        let mut reader = PropertyReader::default();
        assert_eq!(reader.next_offset().unwrap(), Some(0));
        reader
            .append(7, format, 32 / u32::from(format), 4, &[1, 2, 3, 4])
            .unwrap();
        assert_eq!(reader.next_offset().unwrap(), Some(1));
        reader
            .append(7, format, 32 / u32::from(format), 0, &[5, 6, 7, 8])
            .unwrap();
        assert_eq!(reader.next_offset().unwrap(), None);
        assert_eq!(reader.finish().unwrap().bytes, [1, 2, 3, 4, 5, 6, 7, 8]);
    }
    let mut reader = PropertyReader::default();
    reader.append(7, 8, 4, 1, &[0; 4]).unwrap();
    reader.append(7, 8, 1, 0, &[255]).unwrap();
    assert_eq!(reader.finish().unwrap().bytes, [0, 0, 0, 0, 255]);
}

#[test]
fn changing_property_metadata_or_remaining_length_rejects_the_snapshot() {
    for (kind, format, items, remaining, bytes) in [
        (8, 8, 4, 0, &[0; 4][..]),
        (7, 16, 2, 0, &[0; 4][..]),
        (7, 8, 4, 4, &[0; 4][..]),
        (7, 8, 3, 1, &[0; 3][..]),
        (7, 8, 4, 0, &[0; 3][..]),
        (7, 8, 0, 4, &[][..]),
    ] {
        let mut reader = PropertyReader::default();
        reader.append(7, 8, 4, 4, &[0; 4]).unwrap();
        assert!(
            reader
                .append(kind, format, items, remaining, bytes)
                .is_err()
        );
        assert!(reader.next_offset().is_err());
        assert!(reader.append(7, 8, 4, 0, &[0; 4]).is_err());
        assert!(reader.finish().is_err());
    }
    assert!(PropertyReader::default().append(0, 8, 0, 0, &[]).is_err());
    assert!(PropertyReader::default().append(7, 24, 0, 0, &[]).is_err());
    assert!(
        PropertyReader::default()
            .append(7, 16, 2, 1, &[0; 4])
            .is_err()
    );
}

#[test]
fn incremental_size_is_a_lower_bound_and_zero_property_is_the_terminator() {
    let mut incoming = Incoming::new(2);
    assert!(!incoming.append(property(7, 8, &[1, 2])).unwrap());
    assert!(!incoming.append(property(7, 8, &[0, 255, 0])).unwrap());
    assert!(incoming.append(property(7, 8, &[])).unwrap());
    assert_eq!(incoming.finish().unwrap().bytes, [1, 2, 0, 255, 0]);
    let mut truncated = Incoming::new(3);
    truncated.append(property(7, 8, &[1, 2])).unwrap();
    assert!(truncated.append(property(7, 8, &[])).is_err());
    assert!(truncated.finish().is_err());
}

#[test]
fn incremental_type_and_format_cannot_change_between_chunks_or_at_termination() {
    for next in [
        property(8, 8, &[]),
        property(7, 16, &[]),
        property(8, 8, &[1]),
    ] {
        let mut incoming = Incoming::new(0);
        incoming.append(property(7, 8, &[1, 0])).unwrap();
        assert!(incoming.append(next).is_err());
        assert!(incoming.append(property(7, 8, &[])).is_err());
        assert!(incoming.finish().is_err());
    }
}
