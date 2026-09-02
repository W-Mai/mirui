pub(crate) fn slice(bytes: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    let end = offset.checked_add(len)?;
    bytes.get(offset..end)
}

pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let raw: [u8; 2] = slice(bytes, offset, 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(raw))
}

pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw: [u8; 4] = slice(bytes, offset, 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

pub(crate) fn write_u16_le(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
