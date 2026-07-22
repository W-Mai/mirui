mod source;

use alloc::vec::Vec;

use source::{Origin, SourceRange};

use crate::payload::image::ImageMeta;
use crate::{
    DocumentError, FLAT_HEADER_LEN, ImageView, Layout, ReadError, ReadOptions, Reader,
    VERSION_MAJOR, VERSION_MINOR,
};

const DEFAULT_MAX_CHUNKS: u16 = 256;

/// Immutable MIRX file metadata retained by an editable document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetaRef {
    version_major: u8,
    version_minor: u8,
    file_flags: u8,
}

impl FileMetaRef {
    pub const fn version_major(self) -> u8 {
        self.version_major
    }

    pub const fn version_minor(self) -> u8 {
        self.version_minor
    }

    pub const fn file_flags(self) -> u8 {
        self.file_flags
    }

    pub const fn has_future_semantics(self) -> bool {
        self.version_minor > VERSION_MINOR || self.file_flags != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileMeta {
    version_major: u8,
    version_minor: u8,
    file_flags: u8,
}

impl FileMeta {
    const CURRENT: Self = Self {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        file_flags: 0,
    };

    const fn as_ref(self) -> FileMetaRef {
        FileMetaRef {
            version_major: self.version_major,
            version_minor: self.version_minor,
            file_flags: self.file_flags,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FlatRecord {
    image: ImageMeta,
    main: SourceRange,
    extra: Option<SourceRange>,
}

/// Source layout retained until its editable records are materialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocumentState {
    NewChunk,
    SourceFlat(FlatRecord),
    OpaqueFlat,
    SourceChunk,
}

impl DocumentState {
    const fn layout(self) -> Layout {
        match self {
            Self::SourceFlat(_) | Self::OpaqueFlat => Layout::Flat,
            Self::NewChunk | Self::SourceChunk => Layout::Chunk,
        }
    }
}

#[derive(Clone, Copy)]
struct OpenedDocument {
    logical_len: usize,
    file: FileMeta,
    state: DocumentState,
}

/// Source-backed editable MIRX document.
///
/// Opening from a slice borrows the exact input. Opening from a `Vec` moves the
/// allocation into the document. Neither path copies source bytes.
pub struct Document<'a> {
    origin: Origin<'a>,
    logical_len: usize,
    file: FileMeta,
    state: DocumentState,
    dirty: bool,
}

impl<'a> Document<'a> {
    /// Opens a document while borrowing its exact source bytes.
    pub fn open(source: &'a [u8]) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Borrowed(source))
    }

    /// Opens a document by moving its exact source allocation without copying.
    pub fn from_vec(source: Vec<u8>) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Owned(source))
    }

    /// Creates an empty MIRX 1.0 CHUNK document.
    pub const fn new_chunk() -> Self {
        Self {
            origin: Origin::New,
            logical_len: 0,
            file: FileMeta::CURRENT,
            state: DocumentState::NewChunk,
            dirty: true,
        }
    }

    pub const fn layout(&self) -> Layout {
        self.state.layout()
    }

    pub const fn file_meta(&self) -> FileMetaRef {
        self.file.as_ref()
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns the validated image planes of a current-semantics FLAT document.
    ///
    /// Future-semantics FLAT sources remain opaque and return `None`.
    pub fn flat_image(&self) -> Option<ImageView<'_>> {
        let DocumentState::SourceFlat(record) = self.state else {
            return None;
        };
        let main = self.origin.resolve(record.main)?;
        let extra = match record.extra {
            Some(range) => Some(self.origin.resolve(range)?),
            None => None,
        };
        Some(ImageView::from_validated_planes(record.image, main, extra))
    }

    fn from_origin(origin: Origin<'a>) -> Result<Self, DocumentError> {
        let opened = {
            let source = origin
                .source()
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
            inspect_source(source)?
        };
        let document = Self {
            origin,
            logical_len: opened.logical_len,
            file: opened.file,
            state: opened.state,
            dirty: false,
        };

        if !document.origin_contains_logical_source() {
            return Err(DocumentError::Read(ReadError::SizeOverflow));
        }
        Ok(document)
    }

    fn origin_contains_logical_source(&self) -> bool {
        let Some(source) = self.origin.source() else {
            return self.logical_len == 0;
        };
        let Some(range) = SourceRange::checked(0, self.logical_len, source.len()) else {
            return false;
        };
        self.origin.resolve(range).is_some()
    }
}

fn inspect_source(source: &[u8]) -> Result<OpenedDocument, DocumentError> {
    let options = ReadOptions::new().with_max_chunks(DEFAULT_MAX_CHUNKS);
    let reader = Reader::open_with(source, &options)?;
    let header = reader.file_header();
    let state = match (reader.layout(), reader.has_future_semantics()) {
        (Layout::Flat, true) => DocumentState::OpaqueFlat,
        (Layout::Flat, false) => DocumentState::SourceFlat(inspect_current_flat(&reader)?),
        (Layout::Chunk, _) => DocumentState::SourceChunk,
    };
    Ok(OpenedDocument {
        logical_len: reader.logical_len(),
        file: FileMeta {
            version_major: header.version_major,
            version_minor: header.version_minor,
            file_flags: header.flags,
        },
        state,
    })
}

fn inspect_current_flat(reader: &Reader<'_>) -> Result<FlatRecord, DocumentError> {
    let image = reader
        .flat_image()
        .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
    let logical_len = reader.logical_len();
    let main_len = image.main().len();
    let main = SourceRange::checked(FLAT_HEADER_LEN, main_len, logical_len)
        .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
    let extra_start = FLAT_HEADER_LEN
        .checked_add(main_len)
        .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
    let (extra, end) = match image.extra() {
        Some(bytes) => {
            let range = SourceRange::checked(extra_start, bytes.len(), logical_len)
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
            let end = extra_start
                .checked_add(bytes.len())
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
            (Some(range), end)
        }
        None => (None, extra_start),
    };
    if end != logical_len {
        return Err(DocumentError::Read(ReadError::SizeOverflow));
    }

    Ok(FlatRecord {
        image: ImageMeta {
            width: image.width(),
            height: image.height(),
            stride: image.stride(),
            format: image.format(),
        },
        main,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::{
        ColorFormat, FlatImageInput, crc32, encode_chunks, encode_flat, header::chunk_type,
    };

    fn flat_source() -> Vec<u8> {
        encode_flat(&FlatImageInput {
            width: 2,
            height: 1,
            stride: 2,
            format: ColorFormat::A8,
            main: &[7, 9],
            extra: None,
        })
    }

    fn sized_flat_source(format: ColorFormat, width: u32, height: u32, stride: u32) -> Vec<u8> {
        let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
        let extra_len = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
        let main = vec![0x5a; main_len];
        let extra = vec![0xa5; extra_len];
        encode_flat(&FlatImageInput {
            width,
            height,
            stride,
            format,
            main: &main,
            extra: if extra.is_empty() { None } else { Some(&extra) },
        })
    }

    fn take_error(result: Result<Document<'_>, DocumentError>) -> DocumentError {
        match result {
            Ok(_) => panic!("expected document open failure"),
            Err(error) => error,
        }
    }

    #[test]
    fn flat_image_borrows_padded_rgb565_from_borrowed_and_owned_sources() {
        let borrowed_source = sized_flat_source(ColorFormat::RGB565, 3, 2, 8);
        let borrowed_pointer = borrowed_source[FLAT_HEADER_LEN..].as_ptr();
        let borrowed = Document::open(&borrowed_source).unwrap();
        let borrowed_image = borrowed.flat_image().unwrap();
        assert_eq!(borrowed_image.width(), 3);
        assert_eq!(borrowed_image.height(), 2);
        assert_eq!(borrowed_image.stride(), 8);
        assert_eq!(borrowed_image.format(), ColorFormat::RGB565);
        assert_eq!(borrowed_image.main().len(), 16);
        assert_eq!(borrowed_image.main().as_ptr(), borrowed_pointer);
        assert_eq!(borrowed_image.extra(), None);

        let owned_source = sized_flat_source(ColorFormat::RGB565, 3, 2, 8);
        let owned_pointer = owned_source[FLAT_HEADER_LEN..].as_ptr();
        let owned = Document::from_vec(owned_source).unwrap();
        let owned_image = owned.flat_image().unwrap();
        assert_eq!(owned_image.main().len(), 16);
        assert_eq!(owned_image.main().as_ptr(), owned_pointer);
        assert_eq!(owned_image.extra(), None);

        assert_eq!(borrowed.layout(), owned.layout());
        assert!(matches!(borrowed.state, DocumentState::SourceFlat(_)));
        assert!(matches!(owned.state, DocumentState::SourceFlat(_)));
        assert_eq!(borrowed.file_meta(), owned.file_meta());
        assert_eq!(borrowed.logical_len, owned.logical_len);
        assert!(!borrowed.is_dirty());
        assert!(!owned.is_dirty());
    }

    #[test]
    fn owned_source_ranges_survive_document_moves() {
        fn move_document(document: Document<'_>) -> Document<'_> {
            document
        }

        let source = sized_flat_source(ColorFormat::RGB565A8, 3, 2, 8);
        let main_pointer = source[FLAT_HEADER_LEN..FLAT_HEADER_LEN + 16].as_ptr();
        let extra_pointer = source[FLAT_HEADER_LEN + 16..].as_ptr();
        let document = Document::from_vec(source).unwrap();
        let document = move_document(document);
        let image = document.flat_image().unwrap();

        assert_eq!(image.format(), ColorFormat::RGB565A8);
        assert_eq!(image.main().len(), 16);
        assert_eq!(image.main().as_ptr(), main_pointer);
        assert_eq!(image.extra().unwrap().len(), 6);
        assert_eq!(image.extra().unwrap().as_ptr(), extra_pointer);
    }

    #[test]
    fn indexed_flat_image_resolves_main_and_palette_source_ranges() {
        let source = sized_flat_source(ColorFormat::I4, 3, 2, 2);
        let main_pointer = source[FLAT_HEADER_LEN..FLAT_HEADER_LEN + 4].as_ptr();
        let palette_pointer = source[FLAT_HEADER_LEN + 4..].as_ptr();
        let document = Document::open(&source).unwrap();
        let image = document.flat_image().unwrap();

        assert_eq!(image.format(), ColorFormat::I4);
        assert_eq!(image.main().len(), 4);
        assert_eq!(image.main().as_ptr(), main_pointer);
        assert_eq!(image.extra().unwrap().len(), 64);
        assert_eq!(image.extra().unwrap().as_ptr(), palette_pointer);
    }

    #[test]
    fn empty_flat_planes_resolve_at_valid_boundaries() {
        let empty_source = sized_flat_source(ColorFormat::A8, 0, 0, 0);
        let empty_document = Document::open(&empty_source).unwrap();
        let empty_image = empty_document.flat_image().unwrap();
        assert!(empty_image.main().is_empty());
        assert_eq!(
            empty_image.main().as_ptr(),
            empty_source[FLAT_HEADER_LEN..].as_ptr()
        );
        assert_eq!(empty_image.extra(), None);

        let palette_source = sized_flat_source(ColorFormat::I4, 3, 0, 2);
        let palette_document = Document::open(&palette_source).unwrap();
        let palette_image = palette_document.flat_image().unwrap();
        assert!(palette_image.main().is_empty());
        assert_eq!(
            palette_image.main().as_ptr(),
            palette_source[FLAT_HEADER_LEN..FLAT_HEADER_LEN].as_ptr()
        );
        assert_eq!(palette_image.extra().unwrap().len(), 64);
        assert_eq!(
            palette_image.extra().unwrap().as_ptr(),
            palette_source[FLAT_HEADER_LEN..].as_ptr()
        );
    }

    #[test]
    fn borrowed_and_owned_fail_with_the_same_reader_error() {
        let malformed = vec![0; 8];
        let borrowed = take_error(Document::open(&malformed));
        let owned = take_error(Document::from_vec(malformed));

        assert_eq!(borrowed, DocumentError::Read(ReadError::BadMagic));
        assert_eq!(owned, borrowed);
    }

    #[test]
    fn new_chunk_has_current_editable_dirty_state_without_a_source() {
        let document = Document::new_chunk();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.state, DocumentState::NewChunk);
        assert_eq!(
            document.file_meta(),
            FileMetaRef {
                version_major: VERSION_MAJOR,
                version_minor: VERSION_MINOR,
                file_flags: 0,
            }
        );
        assert!(document.is_dirty());
        assert_eq!(document.logical_len, 0);
        assert!(document.origin.source().is_none());
        assert!(document.origin_contains_logical_source());
        assert_eq!(document.flat_image(), None);
    }

    #[test]
    fn future_flat_sources_are_retained_as_opaque_state() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut source = flat_source();
            source[5] = minor;
            source[7] = flags;
            let checksum = crc32(&source[..24]);
            source[24..28].copy_from_slice(&checksum.to_le_bytes());

            let document = Document::open(&source).unwrap();
            assert_eq!(document.layout(), Layout::Flat);
            assert_eq!(document.state, DocumentState::OpaqueFlat);
            assert!(!document.is_dirty());
            assert_eq!(document.file_meta().version_minor(), minor);
            assert_eq!(document.file_meta().file_flags(), flags);
            assert!(document.file_meta().has_future_semantics());
            assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
            assert_eq!(document.flat_image(), None);
        }
    }

    #[test]
    fn future_chunk_sources_keep_their_source_state_and_metadata() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut source = encode_chunks(&[]);
            source[5] = minor;
            source[7] = flags;
            let checksum = crc32(&source[..40]);
            source[40..44].copy_from_slice(&checksum.to_le_bytes());

            let document = Document::open(&source).unwrap();
            assert_eq!(document.layout(), Layout::Chunk);
            assert_eq!(document.state, DocumentState::SourceChunk);
            assert!(!document.is_dirty());
            assert_eq!(document.file_meta().version_minor(), minor);
            assert_eq!(document.file_meta().file_flags(), flags);
            assert!(document.file_meta().has_future_semantics());
            assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
        }
    }

    #[test]
    fn document_default_accepts_256_chunks_and_rejects_257() {
        let chunks = vec![(chunk_type::META, 0, &[][..]); 256];
        let source = encode_chunks(&chunks);
        assert!(Document::open(&source).is_ok());

        let chunks = vec![(chunk_type::META, 0, &[][..]); 257];
        let source = encode_chunks(&chunks);
        assert_eq!(
            take_error(Document::open(&source)),
            DocumentError::Read(ReadError::TooManyChunks {
                count: 257,
                limit: DEFAULT_MAX_CHUNKS,
            })
        );
    }

    #[test]
    fn opened_chunk_stays_source_backed_until_nodes_are_materialized() {
        let source = encode_chunks(&[]);
        let document = Document::open(&source).unwrap();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.state, DocumentState::SourceChunk);
        assert!(!document.is_dirty());
        assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
        assert_eq!(document.flat_image(), None);
    }

    #[test]
    fn document_open_uses_exact_length_policy() {
        for mut source in [flat_source(), encode_chunks(&[])] {
            let logical_len = source.len();
            source.extend_from_slice(&[1, 2, 3]);

            assert_eq!(
                take_error(Document::open(&source)),
                DocumentError::Read(ReadError::TrailingBytes {
                    logical_len,
                    actual_len: source.len(),
                })
            );
        }
    }
}
