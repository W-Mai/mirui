use super::*;

#[test]
fn native_paths_preserve_order_duplicates_and_unicode_without_padding() {
    let paths = [
        r"C:\文 件\🦀.png",
        r"\\server\share\a#b.txt",
        r"C:\文 件\🦀.png",
    ];
    let wide: Vec<Vec<u16>> = paths
        .iter()
        .map(|path| path.encode_utf16().collect())
        .collect();
    let mut block = encode(&wide).unwrap();
    let length = block.len();
    block.extend_from_slice(&[0xff; 17]);
    let decoded = Paths::read(&block).unwrap();
    assert!(decoded.is_unicode());
    let names: Vec<String> = decoded
        .iter()
        .map(|path| {
            let units: Vec<u16> = path
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect();
            String::from_utf16(&units).unwrap()
        })
        .collect();
    assert_eq!(names, paths);
    for end in 0..length {
        assert!(Paths::read(&block[..end]).is_err(), "end={end}");
    }
}

#[test]
fn ansi_paths_are_borrowed_for_native_codepage_conversion() {
    let mut block = vec![0; HEADER];
    block[..4].copy_from_slice(&((HEADER + 3) as u32).to_le_bytes());
    block.extend_from_slice(&[0xff; 3]);
    block.extend_from_slice(b"C:\\caf\xe9.txt\0D:\\b.txt\0\0");
    block.extend_from_slice(b"not a filename\0");
    let paths = Paths::read(&block).unwrap();
    assert!(!paths.is_unicode());
    assert_eq!(
        paths.iter().collect::<Vec<_>>(),
        [b"C:\\caf\xe9.txt".as_slice(), b"D:\\b.txt"]
    );
}

#[test]
fn path_boundaries_and_surrogates_are_validated_before_exposing_entries() {
    let wide = [vec![b'C' as u16, b':' as u16, b'\\' as u16, 0xd800]];
    assert!(encode(&wide).is_err());
    assert!(encode(&[vec![0]]).is_err());
    assert!(encode(&[Vec::<u16>::new()]).is_err());
    assert!(encode(&[] as &[Vec<u16>]).is_err());
    let mut block = encode(&[vec![0x100]]).unwrap();
    assert_eq!(Paths::read(&block).unwrap().iter().next().unwrap(), &[0, 1]);
    block[HEADER..HEADER + 2].copy_from_slice(&0xd800u16.to_le_bytes());
    assert!(Paths::read(&block).is_err());
    block[..4].copy_from_slice(&19u32.to_le_bytes());
    assert!(Paths::read(&block).is_err());
    block[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Paths::read(&block).is_err());
}

#[test]
fn empty_native_lists_still_require_a_double_terminator() {
    for unit in [1, 2] {
        let mut block = vec![0; HEADER];
        block[..4].copy_from_slice(&(HEADER as u32).to_le_bytes());
        block[16] = u8::from(unit == 2);
        block.resize(HEADER + unit, 0);
        assert!(Paths::read(&block).is_err());
        block.resize(HEADER + 2 * unit, 0);
        assert_eq!(Paths::read(&block).unwrap().iter().count(), 0);
    }
}
