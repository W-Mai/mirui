use crate::{
    crc32::Crc32,
    media::{
        DataIntegrity, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaSectionFlags, MediaSectionKind,
    },
    wire::{write_u16_le, write_u32_le},
};

enum Destination<'a> {
    Buffer(&'a mut [u8]),
    Comparison { bytes: &'a [u8], equal: bool },
}

enum ChecksumScope {
    Metadata,
    WholeData,
    IndexedData,
}

pub(super) struct PayloadOutput<'a> {
    destination: Destination<'a>,
    cursor: usize,
    crc: Crc32,
    metadata_crc: Crc32,
    scope: ChecksumScope,
}

impl<'a> PayloadOutput<'a> {
    pub(super) fn buffer(bytes: &'a mut [u8]) -> Self {
        Self {
            destination: Destination::Buffer(bytes),
            cursor: 0,
            crc: Crc32::new(),
            metadata_crc: Crc32::new(),
            scope: ChecksumScope::Metadata,
        }
    }

    pub(super) fn comparison(bytes: &'a [u8]) -> Self {
        Self {
            destination: Destination::Comparison { bytes, equal: true },
            cursor: 0,
            crc: Crc32::new(),
            metadata_crc: Crc32::new(),
            scope: ChecksumScope::Metadata,
        }
    }

    pub(super) fn header(&mut self, bytes: &[u8; MEDIA_HEADER_LEN]) {
        self.write(&bytes[..4]);
        self.cursor = MEDIA_HEADER_LEN;
    }

    pub(super) fn position(&self) -> usize {
        self.cursor
    }

    pub(super) fn begin_data(&mut self, integrity: DataIntegrity<'_>) {
        self.scope = match integrity {
            DataIntegrity::Whole => ChecksumScope::WholeData,
            DataIntegrity::Indexed(_) => ChecksumScope::IndexedData,
        };
    }

    fn write_at(&mut self, offset: usize, bytes: &[u8]) {
        let end = offset + bytes.len();
        match &mut self.destination {
            Destination::Buffer(out) => out[offset..end].copy_from_slice(bytes),
            Destination::Comparison {
                bytes: candidate,
                equal,
            } => {
                *equal &= candidate[offset..end] == *bytes;
            }
        }
    }

    pub(super) fn write(&mut self, bytes: &[u8]) {
        self.write_at(self.cursor, bytes);
        match self.scope {
            ChecksumScope::WholeData => self.crc.update(bytes),
            ChecksumScope::Metadata => self.metadata_crc.update(bytes),
            ChecksumScope::IndexedData => {}
        }
        self.cursor += bytes.len();
    }

    pub(super) fn pad_to(&mut self, offset: usize) {
        debug_assert!(self.cursor <= offset);
        const ZERO: [u8; 64] = [0; 64];
        while self.cursor < offset {
            self.write(&ZERO[..(offset - self.cursor).min(ZERO.len())]);
        }
    }

    pub(super) fn section(&mut self, kind: MediaSectionKind, offset: usize, size: usize) {
        let mut entry = [0; MEDIA_SECTION_LEN];
        write_u16_le(&mut entry, 0, kind.raw());
        write_u16_le(&mut entry, 2, MediaSectionFlags::REQUIRED.bits());
        write_u32_le(&mut entry, 4, offset as u32);
        write_u32_le(&mut entry, 8, size as u32);
        self.write(&entry);
    }

    pub(super) fn finish(mut self) -> bool {
        debug_assert!(!matches!(self.scope, ChecksumScope::Metadata));
        if matches!(self.scope, ChecksumScope::WholeData) {
            let crc = core::mem::replace(&mut self.crc, Crc32::new()).finish();
            self.scope = ChecksumScope::Metadata;
            self.write(&crc.to_le_bytes());
        }
        let metadata_crc = core::mem::replace(&mut self.metadata_crc, Crc32::new()).finish();
        self.write_at(4, &metadata_crc.to_le_bytes());
        match self.destination {
            Destination::Buffer(_) => true,
            Destination::Comparison { equal, .. } => equal,
        }
    }
}
