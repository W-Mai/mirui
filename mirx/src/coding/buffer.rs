#[derive(Clone, Copy, Debug)]
pub(super) enum BufferError {
    SizeOverflow,
    Truncated { offset: usize },
}

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    pub(super) position: usize,
}
impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.position == self.bytes.len()
    }
    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8], BufferError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(BufferError::SizeOverflow)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(BufferError::Truncated {
                offset: self.position,
            })?;
        self.position = end;
        Ok(bytes)
    }
    pub(super) fn byte(&mut self) -> Result<u8, BufferError> {
        Ok(self.take(1)?[0])
    }
}

pub(super) struct Emitter<'a> {
    pub(super) output: Option<&'a mut [u8]>,
    pub(super) position: usize,
}
impl Emitter<'_> {
    pub(super) fn count() -> Self {
        Self {
            output: None,
            position: 0,
        }
    }
    pub(super) fn put(&mut self, bytes: &[u8]) -> Result<(), BufferError> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or(BufferError::SizeOverflow)?;
        if let Some(output) = &mut self.output {
            output[self.position..end].copy_from_slice(bytes);
        }
        self.position = end;
        Ok(())
    }
}
