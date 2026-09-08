pub(crate) struct VectorChunkHeader;

impl VectorChunkHeader {
    pub(crate) const MAGIC: u8 = 0x03;
    pub(crate) const SIZE: usize = 8;
}
