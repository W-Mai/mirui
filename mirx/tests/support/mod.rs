pub fn encode_chunks(chunks: &[(u16, u16, &[u8])]) -> Vec<u8> {
    const HEADER_LEN: usize = 44;
    const ENTRY_LEN: usize = 16;

    let payload_start = HEADER_LEN + chunks.len() * ENTRY_LEN;
    let payload_len: usize = chunks.iter().map(|(_, _, payload)| payload.len()).sum();
    let file_len = payload_start + payload_len;
    let mut bytes = vec![0; file_len];

    bytes[..4].copy_from_slice(b"MIRX");
    bytes[4] = 1;
    bytes[6] = 1;
    bytes[8..10].copy_from_slice(&(chunks.len() as u16).to_le_bytes());
    bytes[12..16].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&(file_len as u32).to_le_bytes());
    if let Some((chunk_type, _, _)) = chunks.first() {
        bytes[20..22].copy_from_slice(&chunk_type.to_le_bytes());
    }
    let header_crc = mirx::crc32(&bytes[..40]);
    bytes[40..44].copy_from_slice(&header_crc.to_le_bytes());

    let mut payload_offset = payload_start;
    for (index, (chunk_type, flags, payload)) in chunks.iter().enumerate() {
        let entry = HEADER_LEN + index * ENTRY_LEN;
        bytes[entry..entry + 2].copy_from_slice(&chunk_type.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&flags.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes[payload_offset..payload_offset + payload.len()].copy_from_slice(payload);
        payload_offset += payload.len();
    }

    bytes
}
