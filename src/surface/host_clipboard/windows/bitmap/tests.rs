use super::*;

#[test]
fn native_bgra_storage_declares_straight_alpha_and_top_down_rows() {
    let dib = bgra_storage(3, 2).unwrap();
    assert_eq!(dib.len(), V5_HEADER + 3 * 2 * 4);
    assert_eq!(word(&dib, 8).unwrap() as i32, -2);
    assert_eq!(word(&dib, 52).unwrap(), 0xff00_0000);
    assert_eq!(word(&dib, 56).unwrap(), 0x7352_4742);
    assert_eq!(Layout::read(&dib).unwrap().pixels, V5_HEADER);
    assert!(dib[V5_HEADER..].iter().all(|byte| *byte == 0));
    assert!(bgra_storage(0, 1).is_err());
    assert!(bgra_storage(1, u32::MAX).is_err());
    assert!(bgra_storage(i32::MAX as u32, i32::MAX as u32).is_err());
}

fn dib(header: usize, width: i32, height: i32, depth: u16, compression: u32) -> Vec<u8> {
    let mut data = vec![0; header];
    data[0..4].copy_from_slice(&(header as u32).to_le_bytes());
    data[4..8].copy_from_slice(&width.to_le_bytes());
    data[8..12].copy_from_slice(&height.to_le_bytes());
    data[12..14].copy_from_slice(&1u16.to_le_bytes());
    data[14..16].copy_from_slice(&depth.to_le_bytes());
    data[16..20].copy_from_slice(&compression.to_le_bytes());
    data
}

#[test]
fn packed_rows_preserve_orientation_and_exclude_allocation_padding() {
    for height in [-2, 2] {
        for depth in [1, 4, 8, 16, 24, 32] {
            let mut data = dib(INFO_HEADER, 3, height, depth, 0);
            let palette = if depth <= 8 { 4 * (1 << depth) } else { 0 };
            let image = (3usize * depth as usize).div_ceil(32) * 4 * 2;
            data.resize(INFO_HEADER + palette + image, 0xa7);
            let payload = data.len();
            data.extend_from_slice(&[0xcd; 17]);
            let bmp = to_bmp(&data).unwrap();
            assert_eq!(&bmp[..2], b"BM");
            assert_eq!(word(&bmp, 2).unwrap() as usize, FILE_HEADER + payload);
            assert_eq!(
                word(&bmp, 10).unwrap() as usize,
                FILE_HEADER + INFO_HEADER + palette
            );
            assert_eq!(&bmp[FILE_HEADER..], &data[..payload]);
            for end in 0..payload {
                assert!(to_bmp(&data[..end]).is_err(), "depth={depth}, end={end}");
            }
        }
    }
}

#[test]
fn color_masks_and_compressed_palette_data_have_distinct_offsets() {
    for header in [INFO_HEADER, V4_HEADER, V5_HEADER] {
        let mut data = dib(header, 1, -1, 32, 3);
        let extra = usize::from(header == INFO_HEADER) * 12;
        data.resize(header + extra + 4, 0);
        for (index, mask) in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff]
            .iter()
            .enumerate()
        {
            data[40 + index * 4..44 + index * 4].copy_from_slice(&mask.to_le_bytes());
        }
        if header >= V4_HEADER {
            data[52..56].copy_from_slice(&0xff00_0000u32.to_le_bytes());
        }
        let bmp = to_bmp(&data).unwrap();
        assert_eq!(
            word(&bmp, 10).unwrap() as usize,
            FILE_HEADER + header + extra
        );
        assert_eq!(&bmp[FILE_HEADER..], &data);
    }
    for (depth, compression) in [(8, 1), (4, 2)] {
        let mut data = dib(INFO_HEADER, 2, 1, depth, compression);
        data[20..24].copy_from_slice(&4u32.to_le_bytes());
        data[32..36].copy_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&[0; 8]);
        data.extend_from_slice(&[2, 1, 0, 1]);
        assert_eq!(word(&to_bmp(&data).unwrap(), 10).unwrap(), 62);
        data[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(to_bmp(&data).is_err());
    }
}

#[test]
fn embedded_profiles_are_bounded_and_linked_profiles_never_open_files() {
    let mut data = dib(V5_HEADER, 1, 1, 32, 0);
    data[56..60].copy_from_slice(&PROFILE_EMBEDDED.to_le_bytes());
    data[112..116].copy_from_slice(&128u32.to_le_bytes());
    data[116..120].copy_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(b"prof");
    data.extend_from_slice(&[0xff; 8]);
    assert_eq!(to_bmp(&data).unwrap().len(), FILE_HEADER + 132);
    data[112..116].copy_from_slice(&124u32.to_le_bytes());
    assert!(to_bmp(&data).is_err());
    data[112..116].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(to_bmp(&data).is_err());
    data[56..60].copy_from_slice(&PROFILE_LINKED.to_le_bytes());
    assert!(matches!(to_bmp(&data), Err(ClipboardError::Unsupported)));
}

#[test]
fn malformed_geometry_is_rejected_before_allocating_a_bitmap() {
    for (width, height, depth, compression) in [
        (0, 1, 24, 0),
        (-1, 1, 24, 0),
        (1, 0, 24, 0),
        (1, 1, 3, 0),
        (1, 1, 24, 3),
        (1, 1, 32, 1),
        (i32::MAX, i32::MIN, 32, 0),
    ] {
        let data = dib(INFO_HEADER, width, height, depth, compression);
        assert!(to_bmp(&data).is_err());
    }
    let mut data = dib(INFO_HEADER, 1, 1, 8, 0);
    data[32..36].copy_from_slice(&257u32.to_le_bytes());
    data.resize(INFO_HEADER + 1028 + 4, 0);
    assert!(to_bmp(&data).is_err());
    data[32..36].copy_from_slice(&1u32.to_le_bytes());
    data[20..24].copy_from_slice(&3u32.to_le_bytes());
    assert!(to_bmp(&data).is_err());
    data[20..24].copy_from_slice(&4u32.to_le_bytes());
    data[12..14].copy_from_slice(&2u16.to_le_bytes());
    assert!(to_bmp(&data).is_err());
}
