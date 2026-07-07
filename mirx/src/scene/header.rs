#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorChunkHeader {
    pub magic: u8,
    pub version: u8,
    pub scale: u8,
    pub flags: u8,
    pub payload_crc32: u32,
}

impl VectorChunkHeader {
    pub const MAGIC: u8 = 0x03;
    pub const SIZE: usize = 8;
    pub const FLAG_HAS_RESOURCE_TABLE: u8 = 1 << 0;
}
