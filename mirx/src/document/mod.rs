#[cfg(test)]
mod atomic_tests;
#[cfg(test)]
mod compatibility_tests;
mod demotion;
#[cfg(test)]
mod demotion_tests;
mod descriptor;
#[cfg(test)]
mod flat_tests;
#[cfg(test)]
mod hint_tests;
mod options;
mod payload;
#[cfg(test)]
mod policy_tests;
mod primary;
#[cfg(test)]
mod promotion_tests;
mod query;
mod raw;
mod reorder;
mod source;
mod writer;

pub use options::{CompatibilityPolicy, EncodeOptions, LayoutPolicy, OpenOptions, RawTypePolicy};
pub use query::{ChunkIter, ChunksOfType, DocumentChunkRef, PayloadOrigin};
pub use raw::{
    CriticalAssumption, PayloadInput, RawChunkInput, RawChunkPolicy, RelocationAssumption,
    RemovedChunkMeta, ReservedBitsPolicy,
};

use alloc::{borrow::Cow, vec::Vec};

use descriptor::grant_open_descriptor;
use primary::open_primary_hint_state;
use source::{Origin, SourceRange};

use crate::payload::image::{ImageAssetParts, ImageMeta, validate_image_planes};
use crate::reader::{PreflightStatus, preflight_chunk, require_understood_critical};
use crate::{
    ChunkFlags, ChunkId, ChunkType, DocumentError, EditError, FLAT_HEADER_LEN, ImageAsset,
    ImageView, Layout, PrimaryHints, ReadError, ReadOptions, Reader, VERSION_MAJOR, VERSION_MINOR,
};

#[cfg(test)]
const DEFAULT_MAX_CHUNKS: u16 = OpenOptions::DEFAULT_MAX_CHUNKS;

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

#[derive(Debug, Eq, PartialEq)]
enum PlaneStorage<'a> {
    SourceRange(SourceRange),
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> PlaneStorage<'a> {
    fn from_cow(plane: Cow<'a, [u8]>) -> Self {
        match plane {
            Cow::Borrowed(bytes) => Self::Borrowed(bytes),
            Cow::Owned(bytes) => Self::Owned(bytes),
        }
    }

    fn resolve<'document>(
        &'document self,
        origin: &'document Origin<'_>,
    ) -> Option<&'document [u8]> {
        match self {
            Self::SourceRange(range) => origin.resolve(*range),
            Self::Borrowed(bytes) => Some(bytes),
            Self::Owned(bytes) => Some(bytes.as_slice()),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum FlatStorage<'a> {
    Planes {
        main: PlaneStorage<'a>,
        extra: Option<PlaneStorage<'a>>,
    },
    Payload {
        backing: PayloadStorage<'a>,
        main: SourceRange,
        extra: Option<SourceRange>,
    },
}

impl FlatStorage<'_> {
    fn resolve_planes<'document>(
        &'document self,
        origin: &'document Origin<'_>,
    ) -> Option<(&'document [u8], Option<&'document [u8]>)> {
        match self {
            Self::Planes { main, extra } => {
                let main = main.resolve(origin)?;
                let extra = match extra {
                    Some(plane) => Some(plane.resolve(origin)?),
                    None => None,
                };
                Some((main, extra))
            }
            Self::Payload {
                backing,
                main,
                extra,
            } => {
                let payload = backing.resolve_contiguous(origin)?;
                let main = main.get(payload)?;
                let extra = match extra {
                    Some(range) => Some(range.get(payload)?),
                    None => None,
                };
                Some((main, extra))
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FlatRecord<'a> {
    image: ImageMeta,
    storage: FlatStorage<'a>,
}

impl FlatRecord<'_> {
    fn from_planes<'a>(
        image: ImageMeta,
        main: PlaneStorage<'a>,
        extra: Option<PlaneStorage<'a>>,
    ) -> FlatRecord<'a> {
        FlatRecord {
            image,
            storage: FlatStorage::Planes { main, extra },
        }
    }

    fn from_payload<'a>(
        image: ImageMeta,
        backing: PayloadStorage<'a>,
        main: SourceRange,
        extra: Option<SourceRange>,
    ) -> FlatRecord<'a> {
        debug_assert!(!matches!(backing, PayloadStorage::PromotedFlat));
        FlatRecord {
            image,
            storage: FlatStorage::Payload {
                backing,
                main,
                extra,
            },
        }
    }

    fn resolve_planes<'document>(
        &'document self,
        origin: &'document Origin<'_>,
    ) -> Option<(&'document [u8], Option<&'document [u8]>)> {
        self.storage.resolve_planes(origin)
    }

    #[cfg(test)]
    fn plane_storage(&self) -> Option<(&PlaneStorage<'_>, Option<&PlaneStorage<'_>>)> {
        match &self.storage {
            FlatStorage::Planes { main, extra } => Some((main, extra.as_ref())),
            FlatStorage::Payload { .. } => None,
        }
    }

    #[cfg(test)]
    fn main_storage(&self) -> &PlaneStorage<'_> {
        self.plane_storage()
            .expect("expected plane-backed FLAT storage")
            .0
    }

    #[cfg(test)]
    fn extra_storage(&self) -> Option<&PlaneStorage<'_>> {
        self.plane_storage()
            .expect("expected plane-backed FLAT storage")
            .1
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ChunkNode<'a> {
    id: ChunkId,
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: PayloadStorage<'a>,
    capability: RewriteCapability,
}

#[derive(Debug, Eq, PartialEq)]
struct ChunkSet<'a> {
    chunks: Vec<ChunkNode<'a>>,
    primary: Option<ChunkId>,
    primary_hints: PrimaryHintState,
    promoted_flat: Option<FlatRecord<'a>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrimaryHintState {
    Derived,
    Explicit(PrimaryHints),
    PreservedOpaque(PrimaryHints),
    KnownNonImageDefault,
    Missing,
}

#[derive(Debug, Eq, PartialEq)]
enum DocumentState<'a> {
    Flat(FlatRecord<'a>),
    OpaqueFlat(PrimaryHints),
    Chunk(ChunkSet<'a>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Compatibility {
    Current,
    FutureReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrailingState {
    None,
    Preserved,
    Discarded,
}

impl DocumentState<'_> {
    const fn layout(&self) -> Layout {
        match self {
            Self::Flat(_) | Self::OpaqueFlat(_) => Layout::Flat,
            Self::Chunk(_) => Layout::Chunk,
        }
    }
}

struct OpenedDocument<'a> {
    logical_len: usize,
    file: FileMeta,
    state: DocumentState<'a>,
    compatibility: Compatibility,
    trailing: TrailingState,
    dirty: bool,
    next_id: u32,
}

#[derive(Debug, Eq, PartialEq)]
enum PayloadStorage<'a> {
    SourceRange(SourceRange),
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
    PromotedFlat,
}

impl PayloadStorage<'_> {
    fn resolve_contiguous<'document>(
        &'document self,
        origin: &'document Origin<'_>,
    ) -> Option<&'document [u8]> {
        match self {
            Self::SourceRange(range) => origin.resolve(*range),
            Self::Borrowed(bytes) => Some(bytes),
            Self::Owned(bytes) => Some(bytes.as_slice()),
            Self::PromotedFlat => None,
        }
    }
}

/// Capabilities established for rewriting one raw payload.
///
/// Opaque opened source nodes default to preserve-only. Typed preflight and
/// explicit policy grant only the individual properties they establish.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct RewriteCapability(u8);

impl RewriteCapability {
    #[cfg(test)]
    const PRESERVE_ONLY: Self = Self(0);
    const RELOCATABLE: u8 = 1 << 0;
    const CRITICAL_UNDERSTOOD: u8 = 1 << 1;
    const PRESERVE_RESERVED_BITS: u8 = 1 << 2;

    const fn new(
        relocatable: bool,
        critical_understood: bool,
        preserve_reserved_bits: bool,
    ) -> Self {
        let mut bits = 0;
        if relocatable {
            bits |= Self::RELOCATABLE;
        }
        if critical_understood {
            bits |= Self::CRITICAL_UNDERSTOOD;
        }
        if preserve_reserved_bits {
            bits |= Self::PRESERVE_RESERVED_BITS;
        }
        Self(bits)
    }

    const fn is_relocatable(self) -> bool {
        self.0 & Self::RELOCATABLE != 0
    }

    const fn critical_understood(self) -> bool {
        self.0 & Self::CRITICAL_UNDERSTOOD != 0
    }

    const fn preserves_reserved_bits(self) -> bool {
        self.0 & Self::PRESERVE_RESERVED_BITS != 0
    }
}

/// Source-backed editable MIRX document.
///
/// Opening from a slice borrows the exact input. Opening from a `Vec` moves the
/// allocation into the document. Neither path copies source bytes.
pub struct Document<'a> {
    origin: Origin<'a>,
    logical_len: usize,
    file: FileMeta,
    state: DocumentState<'a>,
    compatibility: Compatibility,
    trailing: TrailingState,
    dirty: bool,
    next_id: u32,
}

impl<'a> Document<'a> {
    /// Opens a document while borrowing its exact source bytes.
    pub fn open(source: &'a [u8]) -> Result<Self, DocumentError> {
        Self::open_with(source, &OpenOptions::new())
    }

    /// Opens a borrowed document with explicit resource and rewrite policies.
    ///
    /// The options are read only during this call. Future container semantics,
    /// trailing bytes, and raw capabilities follow
    /// [`OpenOptions::with_compatibility`],
    /// [`OpenOptions::with_trailing_bytes`], and
    /// [`OpenOptions::with_raw_type_policies`] respectively.
    pub fn open_with(source: &'a [u8], options: &OpenOptions<'_>) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Borrowed(source), options)
    }

    /// Opens a document by moving its exact source allocation without copying.
    pub fn from_vec(source: Vec<u8>) -> Result<Self, DocumentError> {
        Self::from_vec_with(source, &OpenOptions::new())
    }

    /// Opens an owned document with explicit resource and rewrite policies.
    ///
    /// The options are read only during this call. Future container semantics,
    /// trailing bytes, and raw capabilities follow
    /// [`OpenOptions::with_compatibility`],
    /// [`OpenOptions::with_trailing_bytes`], and
    /// [`OpenOptions::with_raw_type_policies`] respectively.
    pub fn from_vec_with(
        source: Vec<u8>,
        options: &OpenOptions<'_>,
    ) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Owned(source), options)
    }

    /// Creates a MIRX 1.0 FLAT document from validated image metadata and planes.
    ///
    /// Borrowed planes remain borrowed and owned planes move into the document
    /// without changing their allocations.
    pub fn new_flat(image: ImageAsset<'a>) -> Result<Self, EditError> {
        let record = prepare_flat_record(image)?;
        Ok(Self {
            origin: Origin::New,
            logical_len: 0,
            file: FileMeta::CURRENT,
            state: DocumentState::Flat(record),
            compatibility: Compatibility::Current,
            trailing: TrailingState::None,
            dirty: true,
            next_id: 0,
        })
    }

    /// Creates an empty MIRX 1.0 CHUNK document.
    pub const fn new_chunk() -> Self {
        Self {
            origin: Origin::New,
            logical_len: 0,
            file: FileMeta::CURRENT,
            state: DocumentState::Chunk(ChunkSet {
                chunks: Vec::new(),
                primary: None,
                primary_hints: PrimaryHintState::Missing,
                promoted_flat: None,
            }),
            compatibility: Compatibility::Current,
            trailing: TrailingState::None,
            dirty: true,
            next_id: 0,
        }
    }

    /// Converts a current FLAT document to an editable CHUNK document.
    ///
    /// The image planes remain in their existing source-backed, borrowed, or
    /// owned storage. A successful conversion returns the stable identity of
    /// the promoted IMAGE node. A document that already uses CHUNK layout is
    /// left unchanged and returns `None`.
    pub fn ensure_chunk_layout(&mut self) -> Result<Option<ChunkId>, EditError> {
        self.ensure_chunk_layout_with(reserve_promoted_nodes)
    }

    fn ensure_chunk_layout_with<R>(&mut self, reserve: R) -> Result<Option<ChunkId>, EditError>
    where
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        self.ensure_mutable()?;
        match &self.state {
            DocumentState::Chunk(_) => return Ok(None),
            DocumentState::Flat(_) => {}
            DocumentState::OpaqueFlat(_) => {
                unreachable!("writable FLAT documents must have validated image semantics")
            }
        }

        let ids = raw::plan_chunk_ids(self.next_id, 1)?;
        let id = ids.id(0).expect("one planned chunk ID must exist");
        let mut nodes = Vec::new();
        reserve(&mut nodes, 1)?;
        nodes.push(promoted_flat_node(id));

        let previous = core::mem::replace(
            &mut self.state,
            DocumentState::Chunk(ChunkSet {
                chunks: Vec::new(),
                primary: None,
                primary_hints: PrimaryHintState::Missing,
                promoted_flat: None,
            }),
        );
        let DocumentState::Flat(record) = previous else {
            unreachable!("FLAT layout was checked before promotion")
        };
        self.state = DocumentState::Chunk(promoted_chunk_set(record, nodes, id));
        self.next_id = ids.following_counter();
        self.dirty = true;
        Ok(Some(id))
    }

    #[cfg(test)]
    fn ensure_chunk_layout_with_reserve<R>(
        &mut self,
        reserve: R,
    ) -> Result<Option<ChunkId>, EditError>
    where
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        self.ensure_chunk_layout_with(reserve)
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

    /// Replaces the sole image of a current FLAT document without changing layout.
    ///
    /// An image with identical metadata and plane bytes is a no-op that retains
    /// the original source and plane storage.
    pub fn replace_flat_image(&mut self, image: ImageAsset<'a>) -> Result<(), EditError> {
        self.ensure_mutable()?;
        if !matches!(&self.state, DocumentState::Flat(_)) {
            return Err(EditError::FlatLayoutRequired);
        }

        let candidate = prepare_flat_record(image)?;
        let unchanged = {
            let DocumentState::Flat(current) = &self.state else {
                unreachable!("FLAT layout checked before preparing replacement");
            };
            flat_records_equal(&self.origin, current, &candidate)
        };
        if unchanged {
            return Ok(());
        }

        self.state = DocumentState::Flat(candidate);
        self.dirty = true;
        Ok(())
    }

    /// Explicitly discards preserved bytes after the logical MIRX boundary.
    ///
    /// The source allocation is retained. This removes the trailing-byte edit
    /// blocker and marks the document for a later rewrite. Future container
    /// semantics must be normalized while opening before trailing bytes can be
    /// discarded.
    pub fn discard_trailing_bytes(&mut self) -> Result<(), EditError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(EditError::FutureSemanticsReadOnly);
        }
        if matches!(self.trailing, TrailingState::Preserved) {
            self.trailing = TrailingState::Discarded;
            self.dirty = true;
        }
        Ok(())
    }

    /// Returns the validated image planes of a current-semantics FLAT document.
    ///
    /// Future-semantics FLAT sources remain opaque and return `None`.
    pub fn flat_image(&self) -> Option<ImageView<'_>> {
        let DocumentState::Flat(record) = &self.state else {
            return None;
        };
        let (main, extra) = record.resolve_planes(&self.origin)?;
        Some(ImageView::from_validated_planes(record.image, main, extra))
    }

    fn from_origin(origin: Origin<'a>, options: &OpenOptions<'_>) -> Result<Self, DocumentError> {
        let opened = {
            let source = origin
                .source()
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
            inspect_source(source, options)?
        };
        let document = Self {
            origin,
            logical_len: opened.logical_len,
            file: opened.file,
            state: opened.state,
            compatibility: opened.compatibility,
            trailing: opened.trailing,
            dirty: opened.dirty,
            next_id: opened.next_id,
        };

        if let DocumentState::Chunk(chunks) = &document.state {
            debug_assert_eq!(
                usize::try_from(document.next_id).ok(),
                Some(chunks.chunks.len())
            );
        }

        if !document.origin_contains_logical_source() {
            return Err(DocumentError::Read(ReadError::SizeOverflow));
        }
        Ok(document)
    }

    pub(super) const fn ensure_mutable(&self) -> Result<(), EditError> {
        if matches!(self.compatibility, Compatibility::FutureReadOnly) {
            return Err(EditError::FutureSemanticsReadOnly);
        }
        if matches!(self.trailing, TrailingState::Preserved) {
            return Err(EditError::PreservedTrailingBytesReadOnly);
        }
        Ok(())
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

fn promoted_flat_node<'a>(id: ChunkId) -> ChunkNode<'a> {
    ChunkNode {
        id,
        chunk_type: ChunkType::IMAGE,
        flags: ChunkFlags::NONE,
        payload: PayloadStorage::PromotedFlat,
        capability: RewriteCapability::new(true, true, false),
    }
}

fn promoted_chunk_set<'a>(
    record: FlatRecord<'a>,
    chunks: Vec<ChunkNode<'a>>,
    primary: ChunkId,
) -> ChunkSet<'a> {
    debug_assert_eq!(
        chunks
            .iter()
            .filter(|node| matches!(node.payload, PayloadStorage::PromotedFlat))
            .count(),
        1
    );
    ChunkSet {
        chunks,
        primary: Some(primary),
        primary_hints: PrimaryHintState::Derived,
        promoted_flat: Some(record),
    }
}

fn reserve_promoted_nodes(
    chunks: &mut Vec<ChunkNode<'_>>,
    additional: usize,
) -> Result<(), EditError> {
    chunks
        .try_reserve_exact(additional)
        .map_err(|_| EditError::AllocationFailed)
}

fn prepare_flat_record<'a>(image: ImageAsset<'a>) -> Result<FlatRecord<'a>, EditError> {
    let sizes = validate_image_planes(image.meta(), image.main(), image.extra())
        .map_err(EditError::InvalidPayload)?;
    let ImageAssetParts { meta, main, extra } = image.into_parts();
    let extra = if sizes.extra == 0 {
        None
    } else {
        Some(PlaneStorage::from_cow(
            extra.expect("validated nonempty extra plane must be present"),
        ))
    };
    Ok(FlatRecord::from_planes(
        meta,
        PlaneStorage::from_cow(main),
        extra,
    ))
}

fn flat_records_equal(
    origin: &Origin<'_>,
    current: &FlatRecord<'_>,
    candidate: &FlatRecord<'_>,
) -> bool {
    if current.image != candidate.image {
        return false;
    }
    let Some((current_main, current_extra)) = current.resolve_planes(origin) else {
        return false;
    };
    let Some((candidate_main, candidate_extra)) = candidate.resolve_planes(origin) else {
        return false;
    };
    if current_main != candidate_main {
        return false;
    }
    match (current_extra, candidate_extra) {
        (None, None) => true,
        (Some(current), Some(candidate)) => current == candidate,
        _ => false,
    }
}

fn inspect_source<'document>(
    source: &[u8],
    options: &OpenOptions<'_>,
) -> Result<OpenedDocument<'document>, DocumentError> {
    let read_options = ReadOptions::new()
        .with_max_chunks(options.max_chunks())
        .with_payload_limits(options.payload_limits())
        .with_trailing_bytes(options.trailing_bytes_policy());
    let reader = match options.compatibility_policy() {
        CompatibilityPolicy::Preserve => Reader::open_structural_with(source, &read_options)?,
        CompatibilityPolicy::NormalizeToCurrent => {
            Reader::open_structural_as_current_with(source, &read_options)?
        }
    };
    let header = reader.file_header();
    let normalize_future = reader.has_future_semantics()
        && matches!(
            options.compatibility_policy(),
            CompatibilityPolicy::NormalizeToCurrent
        );
    let compatibility = if reader.has_future_semantics() && !normalize_future {
        Compatibility::FutureReadOnly
    } else {
        Compatibility::Current
    };
    let trailing = if reader.has_trailing_bytes() {
        TrailingState::Preserved
    } else {
        TrailingState::None
    };
    let (state, next_id, state_dirty) = match (reader.layout(), reader.flat_image()) {
        (Layout::Flat, None) => (DocumentState::OpaqueFlat(reader.primary_hints()), 0, false),
        (Layout::Flat, Some(image)) => (
            DocumentState::Flat(inspect_flat_image(image, reader.logical_len())?),
            0,
            false,
        ),
        (Layout::Chunk, _) => {
            let (chunks, next_id, dirty) = inspect_chunks(
                &reader,
                options,
                !reader.has_future_semantics() || normalize_future,
                !reader.has_future_semantics(),
            )?;
            (DocumentState::Chunk(chunks), next_id, dirty)
        }
    };
    Ok(OpenedDocument {
        logical_len: reader.logical_len(),
        file: if normalize_future {
            FileMeta::CURRENT
        } else {
            FileMeta {
                version_major: header.version_major,
                version_minor: header.version_minor,
                file_flags: header.flags,
            }
        },
        state,
        compatibility,
        trailing,
        dirty: normalize_future || state_dirty,
        next_id,
    })
}

fn inspect_chunks<'document>(
    reader: &Reader<'_>,
    options: &OpenOptions<'_>,
    allow_reserved_normalize: bool,
    preserve_current_opaque_hints: bool,
) -> Result<(ChunkSet<'document>, u32, bool), DocumentError> {
    let entries = reader.chunks();
    let count = entries.len();
    let primary_index = reader.primary()?.map(|chunk| chunk.index());
    let wire_primary_hints = reader.primary_hints();
    let mut chunks = Vec::new();
    reserve_chunk_nodes(&mut chunks, count)?;

    let mut next_id = 0;
    let mut dirty = false;
    let mut primary_hints = PrimaryHintState::Missing;
    for chunk in entries {
        let preflight = preflight_chunk(chunk, &options.payload_limits());
        let known_contract = matches!(preflight, Ok(PreflightStatus::Validated));
        let policy = raw_type_policy(options, chunk.chunk_type());
        let (descriptor, normalized) = grant_open_descriptor(
            chunk.chunk_type(),
            chunk.flags(),
            known_contract,
            policy,
            allow_reserved_normalize,
        );
        if chunk.flags().is_critical() && !descriptor.capability.critical_understood() {
            require_understood_critical(chunk, preflight)?;
        }

        let id =
            take_next_chunk_id(&mut next_id).ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
        let payload_start = usize::try_from(chunk.payload_offset())
            .map_err(|_| DocumentError::Read(ReadError::SizeOverflow))?;
        let payload =
            SourceRange::checked(payload_start, chunk.payload().len(), reader.logical_len())
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
        chunks.push(ChunkNode {
            id,
            chunk_type: descriptor.chunk_type,
            flags: descriptor.flags,
            payload: PayloadStorage::SourceRange(payload),
            capability: descriptor.capability,
        });
        if primary_index == Some(chunk.index()) {
            primary_hints = if allow_reserved_normalize {
                open_primary_hint_state(
                    chunk.chunk_type(),
                    known_contract,
                    preserve_current_opaque_hints,
                    wire_primary_hints,
                )
            } else {
                PrimaryHintState::PreservedOpaque(wire_primary_hints)
            };
        }
        dirty |= normalized;
    }

    let primary = primary_index.and_then(|index| chunks.get(index).map(|node| node.id));
    Ok((
        ChunkSet {
            chunks,
            primary,
            primary_hints,
            promoted_flat: None,
        },
        next_id,
        dirty,
    ))
}

fn raw_type_policy(options: &OpenOptions<'_>, chunk_type: ChunkType) -> Option<RawChunkPolicy> {
    options
        .raw_type_policies()
        .iter()
        .rev()
        .find(|entry| entry.chunk_type == chunk_type)
        .map(|entry| entry.policy)
}

fn reserve_chunk_nodes(
    chunks: &mut Vec<ChunkNode<'_>>,
    additional: usize,
) -> Result<(), DocumentError> {
    chunks
        .try_reserve_exact(additional)
        .map_err(|_| DocumentError::AllocationFailed)
}

fn take_next_chunk_id(next_id: &mut u32) -> Option<ChunkId> {
    let following = next_id.checked_add(1)?;
    let id = ChunkId::from_session_counter(*next_id);
    *next_id = following;
    Some(id)
}

fn inspect_flat_image<'document>(
    image: ImageView<'_>,
    logical_len: usize,
) -> Result<FlatRecord<'document>, DocumentError> {
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

    Ok(FlatRecord::from_planes(
        ImageMeta {
            width: image.width(),
            height: image.height(),
            stride: image.stride(),
            format: image.format(),
        },
        PlaneStorage::SourceRange(main),
        extra.map(PlaneStorage::SourceRange),
    ))
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::{
        CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ColorFormat, FlatImageInput, ImageChunkInput,
        crc32, encode_chunk_image, encode_chunks, encode_flat, header::chunk_type,
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

    fn chunk_set<'document, 'source>(
        document: &'document Document<'source>,
    ) -> &'document ChunkSet<'source> {
        match &document.state {
            DocumentState::Chunk(chunks) => chunks,
            _ => panic!("expected CHUNK document"),
        }
    }

    fn source_range(node: &ChunkNode<'_>) -> SourceRange {
        match &node.payload {
            PayloadStorage::SourceRange(range) => *range,
            _ => panic!("expected source-backed payload"),
        }
    }

    fn set_primary(source: &mut [u8], chunk_type: u16) {
        source[20..22].copy_from_slice(&chunk_type.to_le_bytes());
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn table_payload_range(source: &[u8], index: usize) -> SourceRange {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        let start = table_payload_offset(source, index);
        let len = u32::from_le_bytes(source[entry + 8..entry + 12].try_into().unwrap()) as usize;
        SourceRange::checked(start, len, source.len()).unwrap()
    }

    fn table_payload_offset(source: &[u8], index: usize) -> usize {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap()) as usize
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
        assert!(matches!(borrowed.state, DocumentState::Flat(_)));
        assert!(matches!(owned.state, DocumentState::Flat(_)));
        assert_eq!(borrowed.file_meta(), owned.file_meta());
        assert_eq!(borrowed.logical_len, owned.logical_len);
        assert_eq!(borrowed.next_id, 0);
        assert_eq!(owned.next_id, 0);
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
        assert_eq!(
            document.state,
            DocumentState::Chunk(ChunkSet {
                chunks: Vec::new(),
                primary: None,
                primary_hints: PrimaryHintState::Missing,
                promoted_flat: None,
            })
        );
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
        assert_eq!(document.next_id, 0);
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
            assert_eq!(
                document.state,
                DocumentState::OpaqueFlat(PrimaryHints::new(ColorFormat::A8.to_u8(), 2, 1, 2,))
            );
            assert!(!document.is_dirty());
            assert_eq!(document.file_meta().version_minor(), minor);
            assert_eq!(document.file_meta().file_flags(), flags);
            assert!(document.file_meta().has_future_semantics());
            assert_eq!(document.next_id, 0);
            assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
            assert_eq!(document.flat_image(), None);
        }
    }

    #[test]
    fn future_chunk_sources_keep_their_source_state_and_metadata() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut source = encode_chunks(&[(0xbeef, 0xa500, b"future")]);
            source[5] = minor;
            source[7] = flags;
            let checksum = crc32(&source[..40]);
            source[40..44].copy_from_slice(&checksum.to_le_bytes());

            let document = Document::open(&source).unwrap();
            assert_eq!(document.layout(), Layout::Chunk);
            let chunks = chunk_set(&document);
            assert_eq!(chunks.chunks.len(), 1);
            assert_eq!(chunks.chunks[0].id, ChunkId::from_session_counter(0));
            assert_eq!(chunks.chunks[0].chunk_type.raw(), 0xbeef);
            assert_eq!(chunks.chunks[0].flags.bits(), 0xa500);
            assert_eq!(chunks.primary, Some(ChunkId::from_session_counter(0)));
            assert!(!document.is_dirty());
            assert_eq!(document.next_id, 1);
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
        let document = Document::open(&source).unwrap();
        assert_eq!(chunk_set(&document).chunks.len(), 256);
        assert_eq!(document.next_id, 256);

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
    fn opened_empty_chunk_materializes_an_empty_node_set() {
        let source = encode_chunks(&[]);
        let document = Document::open(&source).unwrap();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(chunk_set(&document).chunks, []);
        assert_eq!(chunk_set(&document).primary, None);
        assert_eq!(document.next_id, 0);
        assert!(!document.is_dirty());
        assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
        assert_eq!(document.flat_image(), None);
    }

    #[test]
    fn borrowed_and_owned_chunks_keep_table_order_ids_and_source_ranges() {
        let source = encode_chunks(&[
            (0xbeef, 0xa500, b"alpha"),
            (chunk_type::FONT, 0, b"beta"),
            (chunk_type::FONT, 0, b"gamma"),
        ]);
        let expected_ranges = [
            table_payload_range(&source, 0),
            table_payload_range(&source, 1),
            table_payload_range(&source, 2),
        ];
        let borrowed = Document::open(&source).unwrap();

        let owned_source = source.clone();
        let owned_payload_pointer = owned_source[table_payload_offset(&owned_source, 1)..].as_ptr();
        let owned = Document::from_vec(owned_source).unwrap();
        let borrowed_chunks = chunk_set(&borrowed);
        let owned_chunks = chunk_set(&owned);

        assert_eq!(borrowed_chunks.chunks, owned_chunks.chunks);
        assert_eq!(borrowed_chunks.chunks.len(), 3);
        assert_eq!(
            borrowed_chunks.primary,
            Some(ChunkId::from_session_counter(0))
        );
        assert_eq!(owned_chunks.primary, borrowed_chunks.primary);
        for (index, node) in borrowed_chunks.chunks.iter().enumerate() {
            assert_eq!(node.id, ChunkId::from_session_counter(index as u32));
            assert_eq!(source_range(node), expected_ranges[index]);
            assert_eq!(node.capability, RewriteCapability::PRESERVE_ONLY);
        }
        assert_eq!(borrowed.next_id, 3);
        assert_eq!(owned.next_id, 3);
        assert_eq!(
            borrowed
                .origin
                .resolve(source_range(&borrowed_chunks.chunks[0]))
                .unwrap()
                .as_ptr(),
            source[table_payload_offset(&source, 0)..].as_ptr()
        );
        assert_eq!(
            owned
                .origin
                .resolve(source_range(&owned_chunks.chunks[1]))
                .unwrap()
                .as_ptr(),
            owned_payload_pointer
        );
    }

    #[test]
    fn owned_chunk_ranges_survive_document_moves() {
        fn move_document(document: Document<'_>) -> Document<'_> {
            document
        }

        let source = encode_chunks(&[(0xbeef, 0, b"payload")]);
        let payload_pointer = source[table_payload_offset(&source, 0)..].as_ptr();
        let document = Document::from_vec(source).unwrap();
        let document = move_document(document);
        let node = &chunk_set(&document).chunks[0];

        assert_eq!(
            document
                .origin
                .resolve(source_range(node))
                .unwrap()
                .as_ptr(),
            payload_pointer
        );
        assert_eq!(
            document.origin.resolve(source_range(node)).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn duplicate_primary_type_selects_the_first_matching_chunk_id() {
        let mut source = encode_chunks(&[
            (chunk_type::META, 0, b"meta"),
            (chunk_type::FONT, 0, b"first"),
            (chunk_type::FONT, 0, b"second"),
        ]);
        set_primary(&mut source, chunk_type::FONT);

        let document = Document::open(&source).unwrap();
        let chunks = chunk_set(&document);
        assert_eq!(chunks.primary, Some(ChunkId::from_session_counter(1)));
        assert_eq!(chunks.chunks[1].chunk_type, ChunkType::FONT);
        assert_eq!(chunks.chunks[2].chunk_type, ChunkType::FONT);
    }

    #[test]
    fn zero_and_stale_primary_types_map_to_none_without_dropping_chunks() {
        for primary_type in [0, chunk_type::VECTOR] {
            let mut source = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
            set_primary(&mut source, primary_type);

            let document = Document::open(&source).unwrap();
            let chunks = chunk_set(&document);
            assert_eq!(chunks.primary, None);
            assert_eq!(chunks.chunks.len(), 1);
            assert_eq!(chunks.chunks[0].chunk_type, ChunkType::META);
        }
    }

    #[test]
    fn empty_and_overlapping_payload_ranges_are_retained_exactly() {
        let empty_source =
            encode_chunks(&[(chunk_type::META, 0, b"body"), (chunk_type::FONT, 0, b"")]);
        let empty_document = Document::open(&empty_source).unwrap();
        let empty_node = &chunk_set(&empty_document).chunks[1];
        assert_eq!(
            source_range(empty_node),
            table_payload_range(&empty_source, 1)
        );
        assert!(
            empty_document
                .origin
                .resolve(source_range(empty_node))
                .unwrap()
                .is_empty()
        );

        let mut overlap_source =
            encode_chunks(&[(chunk_type::META, 0, b"abcd"), (chunk_type::FONT, 0, b"xy")]);
        let first = table_payload_range(&overlap_source, 0);
        let first_start = table_payload_offset(&overlap_source, 0);
        let second_entry = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN;
        overlap_source[second_entry + 4..second_entry + 8]
            .copy_from_slice(&((first_start + 1) as u32).to_le_bytes());
        overlap_source[second_entry + 8..second_entry + 12].copy_from_slice(&2u32.to_le_bytes());

        let overlap_document = Document::open(&overlap_source).unwrap();
        let nodes = &chunk_set(&overlap_document).chunks;
        assert_eq!(source_range(&nodes[0]), first);
        assert_eq!(
            source_range(&nodes[1]),
            table_payload_range(&overlap_source, 1)
        );
        assert_eq!(
            overlap_document
                .origin
                .resolve(source_range(&nodes[0]))
                .unwrap(),
            b"abcd"
        );
        assert_eq!(
            overlap_document
                .origin
                .resolve(source_range(&nodes[1]))
                .unwrap(),
            b"bc"
        );
    }

    #[test]
    fn chunk_node_reservation_failure_is_explicit_and_failure_atomic() {
        let mut chunks = Vec::new();
        assert_eq!(
            reserve_chunk_nodes(&mut chunks, usize::MAX),
            Err(DocumentError::AllocationFailed)
        );
        assert!(chunks.is_empty());
    }

    #[test]
    fn checked_chunk_id_counter_stops_without_wrapping() {
        let mut next_id = u32::MAX - 1;
        assert_eq!(
            take_next_chunk_id(&mut next_id),
            Some(ChunkId::from_session_counter(u32::MAX - 1))
        );
        assert_eq!(next_id, u32::MAX);
        assert_eq!(take_next_chunk_id(&mut next_id), None);
        assert_eq!(next_id, u32::MAX);
    }

    #[test]
    fn chunk_node_size_stays_within_the_embedded_budget() {
        let size = core::mem::size_of::<ChunkNode>();
        match core::mem::size_of::<usize>() {
            4 => assert!(size <= 32),
            8 => assert!(size <= 48),
            _ => panic!("unsupported pointer width"),
        }
    }

    #[test]
    fn critical_payloads_are_validated_before_chunk_nodes_are_available() {
        let malformed =
            encode_chunks(&[(chunk_type::IMAGE, ChunkFlags::CRITICAL.bits(), b"short")]);
        assert!(matches!(
            take_error(Document::open(&malformed)),
            DocumentError::Read(ReadError::CriticalPayload(_))
        ));

        let pixels = [1, 2, 3, 4];
        let mut valid = encode_chunk_image(&ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::A8,
            stride: 2,
            main: &pixels,
            extra: None,
        });
        valid[CHUNK_FILE_HEADER_LEN + 2..CHUNK_FILE_HEADER_LEN + 4]
            .copy_from_slice(&ChunkFlags::CRITICAL.bits().to_le_bytes());
        let document = Document::open(&valid).unwrap();
        let chunks = chunk_set(&document);
        assert_eq!(chunks.chunks.len(), 1);
        assert_eq!(chunks.chunks[0].chunk_type, ChunkType::IMAGE);
        assert!(chunks.chunks[0].flags.is_critical());
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
