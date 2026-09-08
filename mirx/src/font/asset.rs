use alloc::vec::Vec;
use core::ops::Range;

pub(super) mod source;
use source::Source;

use super::{
    ADVANCE_RECORD_LEN, CMAP_INDEX_RECORD_LEN, CmapEntry, FACE_RECORD_LEN, FontError, FontFace,
    FontRepresentation, FontRepresentations, GLYPH_ID_RECORD_LEN, GLYPH_SURFACE_RECORD_LEN,
    GlyphId, GlyphMap, GlyphPacking, GlyphSurfaceRecord, RASTER_METRICS_RECORD_LEN,
    REPRESENTATION_RECORD_LEN, RasterMetrics, RawGlyphs, RepresentationRecord, ShapingData,
};
use crate::{
    ByteAlignment, PayloadLimits,
    image::{
        ATLAS_REGION_LEN, AtlasMap, ColorDescription, EncodedImageAsset, EncodedStoragePlan,
        PLANE_RECORD_LEN, PlaneMemoryLayout, SurfaceDescriptor,
    },
    media::{
        DataIntegrity, INTEGRITY_RECORD_LEN, IntegrityRange, MEDIA_CRC_LEN, MEDIA_HEADER_LEN,
        MEDIA_SECTION_LEN, MEDIA_VERSION, MediaFlags, MediaSectionKind, UnitIndex,
        output::PayloadOutput,
    },
    types::Fixed,
    wire::write_u16_le,
};

/// One representation's size semantics and shared storage references.
#[derive(Clone, Copy, Debug)]
pub struct RepresentationAsset {
    metadata: FontRepresentation,
    surface: u16,
    atlas_map: Option<u32>,
}

impl RepresentationAsset {
    pub const fn new(metadata: FontRepresentation, surface: u16) -> Self {
        Self {
            metadata,
            surface,
            atlas_map: None,
        }
    }

    /// References a shared Atlas2D map supplied through `FontAsset::with_atlas_maps`.
    /// GlyphMajor representations omit this reference.
    pub const fn with_atlas_map(mut self, map: u32) -> Self {
        self.atlas_map = Some(map);
        self
    }
    pub const fn metadata(self) -> FontRepresentation {
        self.metadata
    }
    pub const fn surface_index(self) -> u16 {
        self.surface
    }
    pub const fn atlas_map_index(self) -> Option<u32> {
        self.atlas_map
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontAdvanceSource<'a> {
    Advances(&'a [Fixed]),
    Shaping(&'a [u8]),
}

/// One shared scalar sample allocation, independent of representation metrics.
#[derive(Clone, Copy, Debug)]
pub enum GlyphSurfaceAsset<'a> {
    Raw {
        glyphs: RawGlyphs<'a, 'a>,
        integrity: DataIntegrity<'a>,
    },
    Encoded {
        map: GlyphMap<'a>,
        image: EncodedImageAsset<'a>,
    },
}

impl<'a> GlyphSurfaceAsset<'a> {
    pub const fn raw(glyphs: RawGlyphs<'a, 'a>) -> Self {
        Self::Raw {
            glyphs,
            integrity: DataIntegrity::Whole,
        }
    }

    pub const fn with_integrity(self, integrity: DataIntegrity<'a>) -> Self {
        match self {
            Self::Raw { glyphs, .. } => Self::Raw { glyphs, integrity },
            Self::Encoded { map, image } => Self::Encoded {
                map,
                image: image.with_integrity(integrity),
            },
        }
    }

    pub const fn map(self) -> GlyphMap<'a> {
        match self {
            Self::Raw { glyphs, .. } => glyphs.map(),
            Self::Encoded { map, .. } => map,
        }
    }
    fn descriptor(self) -> SurfaceDescriptor {
        match self {
            Self::Raw { glyphs, .. } => SurfaceDescriptor::new(
                self.map().width(),
                self.map().height(),
                glyphs.sample_layout(),
                ColorDescription::NONE,
            )
            .expect("scalar glyph surface"),
            Self::Encoded { image, .. } => image.surface(),
        }
    }
    pub const fn integrity(self) -> DataIntegrity<'a> {
        match self {
            Self::Raw { integrity, .. } => integrity,
            Self::Encoded { image, .. } => image.integrity(),
        }
    }
    pub(in crate::font) fn plan(self) -> Result<Storage<'a>, FontError> {
        match self {
            Self::Raw { glyphs, .. } => {
                let map = glyphs.map();
                let (width, height) = map.cell_extent().unwrap_or((map.width(), map.height()));
                let plane = glyphs
                    .sample_layout()
                    .plane_geometry(width, height, 0)
                    .expect("scalar geometry");
                let tight = PlaneMemoryLayout::tight(plane).expect("validated tight span");
                Ok(Storage::Raw {
                    memory: (tight != glyphs.memory_layout()).then_some(glyphs.memory_layout()),
                    data: glyphs.as_bytes(),
                })
            }
            Self::Encoded { map, image } => {
                let surface = image.surface();
                if !surface.sample_layout().is_alpha()
                    || surface.color() != ColorDescription::NONE
                    || surface.flags().bits() != 0
                    || image.color_table().is_some()
                    || (surface.width(), surface.height()) != (map.width(), map.height())
                {
                    return Err(FontError::SurfaceMismatch);
                }
                EncodedStoragePlan::new(image)
                    .map(Storage::Encoded)
                    .map_err(FontError::Image)
            }
        }
    }
}

/// Borrowed native authoring input for one face with shared glyphs and surfaces.
/// Directory positions and byte offsets are assigned by the writer.
#[derive(Clone, Copy, Debug)]
pub struct FontAsset<'a> {
    face: FontFace,
    cmap: &'a [CmapEntry],
    glyph_ids: Option<&'a [GlyphId]>,
    advance_source: FontAdvanceSource<'a>,
    representations: &'a [RepresentationAsset],
    raster_metrics: &'a [RasterMetrics],
    surfaces: &'a [GlyphSurfaceAsset<'a>],
    atlas_maps: &'a [AtlasMap<'a>],
}

impl<'a> FontAsset<'a> {
    pub const fn new(
        face: FontFace,
        cmap: &'a [CmapEntry],
        advance_source: FontAdvanceSource<'a>,
    ) -> Self {
        Self {
            face,
            cmap,
            glyph_ids: None,
            advance_source,
            representations: &[],
            raster_metrics: &[],
            surfaces: &[],
            atlas_maps: &[],
        }
    }
    pub const fn with_rasters(
        mut self,
        representations: &'a [RepresentationAsset],
        raster_metrics: &'a [RasterMetrics],
        surfaces: &'a [GlyphSurfaceAsset<'a>],
    ) -> Self {
        self.representations = representations;
        self.raster_metrics = raster_metrics;
        self.surfaces = surfaces;
        self
    }
    pub const fn with_glyph_ids(mut self, glyph_ids: &'a [GlyphId]) -> Self {
        self.glyph_ids = Some(glyph_ids);
        self
    }
    pub const fn with_atlas_maps(mut self, maps: &'a [AtlasMap<'a>]) -> Self {
        self.atlas_maps = maps;
        self
    }
    pub const fn face(self) -> FontFace {
        self.face
    }
    pub const fn cmap(self) -> &'a [CmapEntry] {
        self.cmap
    }
    pub const fn glyph_ids(self) -> Option<&'a [GlyphId]> {
        self.glyph_ids
    }
    pub const fn advance_source(self) -> FontAdvanceSource<'a> {
        self.advance_source
    }
    pub const fn representations(self) -> &'a [RepresentationAsset] {
        self.representations
    }
    pub const fn raster_metrics(self) -> &'a [RasterMetrics] {
        self.raster_metrics
    }
    pub const fn surfaces(self) -> &'a [GlyphSurfaceAsset<'a>] {
        self.surfaces
    }
    pub const fn atlas_maps(self) -> &'a [AtlasMap<'a>] {
        self.atlas_maps
    }

    /// Exact canonical size after structural validation; DATA is not decoded.
    pub fn encoded_len(self) -> Result<usize, FontError> {
        Ok(Plan::new(self)?.len)
    }

    /// Required payload placement for every aligned DATA body, independent of its pointer.
    pub fn input_alignment(self) -> Result<ByteAlignment, FontError> {
        Plan::new(self)?;
        self.surfaces
            .iter()
            .try_fold(ByteAlignment::ONE, |alignment, surface| {
                Ok(alignment.max(surface.plan()?.alignment()))
            })
    }

    /// Writes only after validation and capacity checks; the output suffix is untouched.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FontError> {
        let plan = Plan::new(self)?;
        if output.len() < plan.len {
            return Err(FontError::BufferTooSmall {
                needed: plan.len,
                available: output.len(),
            });
        }
        plan.emit(PayloadOutput::buffer(&mut output[..plan.len]));
        Ok(plan.len)
    }

    /// Allocates one complete payload; no extra backing-address alignment is promised.
    pub fn encode(self) -> Result<Vec<u8>, FontError> {
        let plan = Plan::new(self)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(plan.len)
            .map_err(|_| FontError::AllocationFailed)?;
        bytes.resize(plan.len, 0);
        plan.emit(PayloadOutput::buffer(&mut bytes));
        Ok(bytes)
    }

    pub fn matches_payload(self, bytes: &[u8]) -> Result<bool, FontError> {
        let plan = Plan::new(self)?;
        Ok(bytes.len() == plan.len && plan.emit(PayloadOutput::comparison(bytes)))
    }

    /// Checks scalar syntax with resource-wide counters before owning output is allocated.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), FontError> {
        Source::preflight(self, limits)
    }
}

#[derive(Clone, Copy)]
pub(in crate::font) enum Storage<'a> {
    Raw {
        memory: Option<PlaneMemoryLayout>,
        data: &'a [u8],
    },
    Encoded(EncodedStoragePlan<'a>),
}
impl Storage<'_> {
    pub(in crate::font) fn sections(self) -> [Option<(MediaSectionKind, usize)>; 3] {
        match self {
            Self::Raw { memory, .. } => [
                memory.map(|_| (MediaSectionKind::PLANES, PLANE_RECORD_LEN)),
                None,
                None,
            ],
            Self::Encoded(plan) => plan.sections(),
        }
    }
    pub(in crate::font) fn alignment(self) -> ByteAlignment {
        match self {
            Self::Raw { memory, .. } => {
                memory.map_or(ByteAlignment::ONE, |p| p.required_alignment())
            }
            Self::Encoded(plan) => plan.alignment(),
        }
    }
    pub(in crate::font) fn data_len(self) -> usize {
        match self {
            Self::Raw { data, .. } => data.len(),
            Self::Encoded(plan) => plan.data_len(),
        }
    }
    fn data_crc(self, range: Range<u32>) -> u32 {
        match self {
            Self::Raw { data, .. } => crate::crc32(&data[range.start as usize..range.end as usize]),
            Self::Encoded(plan) => plan.data_crc(range),
        }
    }
    fn emit_data(self, output: &mut PayloadOutput<'_>) {
        match self {
            Self::Raw { data, .. } => output.write(data),
            Self::Encoded(plan) => plan.emit_data(output),
        }
    }
    fn emit(self, output: &mut PayloadOutput<'_>) {
        match self {
            Self::Raw {
                memory: Some(memory),
                ..
            } => {
                output.write(&memory.encode_record());
            }
            Self::Raw { memory: None, .. } => {}
            Self::Encoded(plan) => plan.emit_metadata(output),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Plan<S: Source> {
    asset: S,
    sections: u16,
    metadata_end: usize,
    pub(super) len: usize,
    integrity_len: usize,
    atlas_records: usize,
    indexed: bool,
}

impl<S: Source> Plan<S> {
    fn atlas_record_count(asset: S) -> Result<usize, FontError> {
        asset.atlas_maps().try_fold(0usize, |count, map| {
            count.checked_add(map.len()).ok_or(FontError::SizeOverflow)
        })
    }

    fn atlas_map_range(asset: S, index: usize) -> Result<(u32, u32), FontError> {
        let offset = asset
            .atlas_maps()
            .take(index)
            .try_fold(0usize, |offset, map| {
                offset.checked_add(map.len()).ok_or(FontError::SizeOverflow)
            })?;
        let count = asset.atlas_map(index).ok_or(FontError::SizeOverflow)?.len();
        Ok((
            u32::try_from(offset).map_err(|_| FontError::SizeOverflow)?,
            u32::try_from(count).map_err(|_| FontError::SizeOverflow)?,
        ))
    }

    fn table_size(asset: S) -> Result<u64, FontError> {
        let glyphs = usize::from(asset.face().raster_count());
        let lengths = [
            glyphs,
            asset.cmap().len(),
            asset.glyph_ids().map_or(0, <[GlyphId]>::len),
            asset.representation_count(),
            asset.raster_metrics().len(),
            asset.surface_count(),
            asset.atlas_map_count(),
        ];
        for length in lengths {
            u32::try_from(length).map_err(|_| FontError::SizeOverflow)?;
        }
        let [
            _glyphs,
            cmap,
            glyph_ids,
            representations,
            metrics,
            surfaces,
            _,
        ] = lengths.map(|n| n as u64);
        let map_bytes = (Self::atlas_record_count(asset)? as u64)
            .checked_mul(ATLAS_REGION_LEN as u64)
            .ok_or(FontError::SizeOverflow)?;
        let source_bytes = match asset.advance_source() {
            FontAdvanceSource::Advances(values) => values.len() as u64 * ADVANCE_RECORD_LEN as u64,
            FontAdvanceSource::Shaping(bytes) => bytes.len() as u64,
        };
        let size = (FACE_RECORD_LEN as u64
            + cmap * CMAP_INDEX_RECORD_LEN as u64
            + glyph_ids * GLYPH_ID_RECORD_LEN as u64
            + representations * REPRESENTATION_RECORD_LEN as u64
            + metrics * RASTER_METRICS_RECORD_LEN as u64
            + surfaces * GLYPH_SURFACE_RECORD_LEN as u64)
            .checked_add(source_bytes)
            .and_then(|n| n.checked_add(map_bytes))
            .ok_or(FontError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| FontError::SizeOverflow)?;
        Ok(size)
    }

    pub(super) fn new(asset: S) -> Result<Self, FontError> {
        let plan = Self::metadata(asset)?;
        for surface in asset.surfaces() {
            if let Storage::Encoded(storage) = surface.plan()? {
                storage.validate().map_err(FontError::Image)?;
            }
        }
        Ok(plan)
    }

    fn metadata(asset: S) -> Result<Self, FontError> {
        let mut size = Self::table_size(asset)?;
        let glyph_count = usize::from(asset.face().raster_count());
        let atlas_records = Self::atlas_record_count(asset)?;
        if asset.surface_count() > asset.representation_count()
            || asset.atlas_map_count() > asset.representation_count()
        {
            return Err(FontError::UnreferencedStorage);
        }
        for (index, pair) in asset.cmap().windows(2).enumerate() {
            if pair[0].scalar() >= pair[1].scalar() {
                return Err(FontError::Cmap(super::CmapIndexError::NotSorted {
                    index: index + 1,
                    previous: pair[0].scalar(),
                    current: pair[1].scalar(),
                }));
            }
        }
        if let Some(ids) = asset.glyph_ids() {
            if ids.len() != glyph_count {
                return Err(FontError::GlyphIdCount {
                    expected: glyph_count,
                    actual: ids.len(),
                });
            }
            for (index, pair) in ids.windows(2).enumerate() {
                if pair[0] >= pair[1] {
                    return Err(FontError::GlyphIds(super::GlyphIdsError::NotSorted {
                        index: index + 1,
                        previous: pair[0],
                        current: pair[1],
                    }));
                }
            }
            if ids
                .iter()
                .enumerate()
                .all(|(ordinal, id)| id.get() as usize == ordinal)
            {
                return Err(FontError::NonCanonicalGlyphIds);
            }
        }
        for glyph_id in core::iter::once(asset.face().default_glyph())
            .chain(asset.cmap().iter().map(|entry| entry.glyph_id()))
        {
            if Self::raster_ordinal(asset, glyph_id).is_none() {
                return Err(FontError::MissingRasterGlyph(glyph_id));
            }
        }
        match asset.advance_source() {
            FontAdvanceSource::Advances(values) if values.len() != glyph_count => {
                return Err(FontError::AdvanceCount {
                    expected: glyph_count,
                    actual: values.len(),
                });
            }
            FontAdvanceSource::Shaping(bytes) => {
                ShapingData::open(bytes)
                    .and_then(|shaping| shaping.validate_cmap_entries(asset.cmap()))
                    .map_err(FontError::Shaping)?;
            }
            FontAdvanceSource::Advances(_) => {}
        }
        let expected_metrics = asset
            .representation_count()
            .checked_mul(glyph_count)
            .ok_or(FontError::SizeOverflow)?;
        if asset.raster_metrics().len() != expected_metrics {
            return Err(FontError::RasterMetricCount {
                expected: expected_metrics,
                actual: asset.raster_metrics().len(),
            });
        }
        FontRepresentations::validate_by(asset.representation_count(), |i| {
            asset
                .representation(i)
                .expect("representation ordinal")
                .metadata
        })
        .map_err(FontError::Selection)?;
        for (index, representation) in asset.representations().enumerate() {
            let surface = asset.surface(usize::from(representation.surface)).ok_or(
                FontError::SurfaceOutOfBounds {
                    representation: index,
                },
            )?;
            if surface.map().len() != glyph_count {
                return Err(FontError::Cardinality);
            }
            RepresentationRecord::new(representation.metadata, representation.surface)
                .validate_for(surface.descriptor())
                .map_err(FontError::Record)?;
            match (surface.map().packing(), representation.atlas_map) {
                (GlyphPacking::GlyphMajor, None) => {}
                (GlyphPacking::Atlas2D, Some(id)) => {
                    let map =
                        asset
                            .atlas_map(id as usize)
                            .ok_or(FontError::AtlasMapOutOfBounds {
                                representation: index,
                            })?;
                    if map.len() != glyph_count
                        || map.iter().any(|region| {
                            region.right() > surface.map().width()
                                || region.bottom() > surface.map().height()
                        })
                    {
                        return Err(FontError::AtlasMapMismatch {
                            representation: index,
                        });
                    }
                }
                _ => {
                    return Err(FontError::AtlasMapMismatch {
                        representation: index,
                    });
                }
            }
        }
        for index in 0..asset.atlas_map_count() {
            if !asset
                .representations()
                .any(|r| r.atlas_map.map(|id| id as usize) == Some(index))
            {
                return Err(FontError::UnreferencedStorage);
            }
        }
        let mut sections = 6
            + usize::from(asset.glyph_ids().is_some())
            + usize::from(asset.atlas_map_count() != 0)
            + asset.surface_count();
        let indexed = asset
            .surfaces()
            .any(|s| s.integrity().partitions().is_some());
        let mut integrity_len = 0u64;
        for (index, surface) in asset.surfaces().enumerate() {
            if !asset
                .representations()
                .any(|r| usize::from(r.surface) == index)
            {
                return Err(FontError::UnreferencedStorage);
            }
            let storage = surface.plan()?;
            surface
                .integrity()
                .section_len(storage.data_len())
                .map_err(FontError::Integrity)?;
            for (_, len) in storage.sections().into_iter().flatten() {
                sections += 1;
                size += len as u64;
            }
            if indexed {
                let count = surface
                    .integrity()
                    .partitions()
                    .map_or(usize::from(storage.data_len() != 0), <[u32]>::len);
                integrity_len += count as u64 * INTEGRITY_RECORD_LEN as u64;
            }
        }
        sections += usize::from(indexed);
        let sections = u16::try_from(sections).map_err(|_| FontError::SizeOverflow)?;
        size += integrity_len
            + MEDIA_HEADER_LEN as u64
            + u64::from(sections) * MEDIA_SECTION_LEN as u64;
        let metadata_end = u32::try_from(size).map_err(|_| FontError::SizeOverflow)? as usize;
        let mut end = metadata_end;
        for surface in asset.surfaces() {
            let storage = surface.plan()?;
            end = Self::aligned(end, storage.alignment())?;
            end = end
                .checked_add(storage.data_len())
                .ok_or(FontError::SizeOverflow)?;
        }
        let len = end
            .checked_add(if indexed { 0 } else { MEDIA_CRC_LEN })
            .ok_or(FontError::SizeOverflow)?;
        u32::try_from(len).map_err(|_| FontError::SizeOverflow)?;
        Ok(Self {
            asset,
            sections,
            metadata_end,
            len,
            integrity_len: integrity_len as usize,
            atlas_records,
            indexed,
        })
    }

    fn raster_ordinal(asset: S, glyph_id: GlyphId) -> Option<usize> {
        match asset.glyph_ids() {
            Some(ids) => ids.binary_search(&glyph_id).ok(),
            None => {
                let ordinal = usize::from(glyph_id.get());
                (ordinal < usize::from(asset.face().raster_count())).then_some(ordinal)
            }
        }
    }

    fn aligned(offset: usize, alignment: ByteAlignment) -> Result<usize, FontError> {
        UnitIndex::aligned(
            u32::try_from(offset).map_err(|_| FontError::SizeOverflow)?,
            alignment,
        )
        .map(|n| n as usize)
        .map_err(|_| FontError::SizeOverflow)
    }

    fn visit_metadata_sections(self, mut visitor: impl FnMut(MediaSectionKind, usize)) {
        visitor(MediaSectionKind::FACE, FACE_RECORD_LEN);
        visitor(
            MediaSectionKind::CMAP_INDEX,
            self.asset.cmap().len() * CMAP_INDEX_RECORD_LEN,
        );
        if let Some(ids) = self.asset.glyph_ids() {
            visitor(MediaSectionKind::GLYPH_IDS, ids.len() * GLYPH_ID_RECORD_LEN);
        }
        visitor(
            MediaSectionKind::REPRESENTATIONS,
            self.asset.representation_count() * REPRESENTATION_RECORD_LEN,
        );
        match self.asset.advance_source() {
            FontAdvanceSource::Advances(values) => visitor(
                MediaSectionKind::ADVANCES,
                values.len() * ADVANCE_RECORD_LEN,
            ),
            FontAdvanceSource::Shaping(bytes) => visitor(MediaSectionKind::SHAPING, bytes.len()),
        }
        visitor(
            MediaSectionKind::RASTER_METRICS,
            self.asset.raster_metrics().len() * RASTER_METRICS_RECORD_LEN,
        );
        visitor(
            MediaSectionKind::SURFACE_GROUPS,
            self.asset.surface_count() * GLYPH_SURFACE_RECORD_LEN,
        );
        if self.asset.atlas_map_count() != 0 {
            visitor(
                MediaSectionKind::ATLAS_MAPS,
                self.atlas_records * ATLAS_REGION_LEN,
            );
        }
    }

    fn metadata_section_count(self) -> u16 {
        6 + u16::from(self.asset.glyph_ids().is_some())
            + u16::from(self.asset.atlas_map_count() != 0)
    }

    fn visit_data(&self, mut visitor: impl FnMut(GlyphSurfaceAsset<'_>, Storage<'_>, usize)) {
        let mut offset = self.metadata_end;
        for surface in self.asset.surfaces() {
            let storage = surface.plan().expect("validated storage");
            offset = Self::aligned(offset, storage.alignment()).expect("validated span");
            visitor(surface, storage, offset);
            offset += storage.data_len();
        }
    }

    pub(super) fn emit(self, mut output: PayloadOutput<'_>) -> bool {
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        header[1] = if self.indexed {
            MediaFlags::INDEXED_INTEGRITY.bits()
        } else {
            0
        };
        write_u16_le(&mut header, 2, self.sections);
        output.header(&header);
        let mut offset = MEDIA_HEADER_LEN + usize::from(self.sections) * MEDIA_SECTION_LEN;
        self.visit_metadata_sections(|kind, len| {
            output.section(kind, offset, len);
            offset += len;
        });
        for surface in self.asset.surfaces() {
            for (kind, len) in surface
                .plan()
                .expect("validated storage")
                .sections()
                .into_iter()
                .flatten()
            {
                output.section(kind, offset, len);
                offset += len;
            }
        }
        if self.indexed {
            output.section(MediaSectionKind::INTEGRITY, offset, self.integrity_len);
        }
        self.visit_data(|_, storage, offset| {
            output.section(MediaSectionKind::DATA, offset, storage.data_len())
        });
        output.write(&self.asset.face().encode_record());
        for &entry in self.asset.cmap() {
            output.write(&entry.encode_record());
        }
        if let Some(ids) = self.asset.glyph_ids() {
            for id in ids {
                output.write(&id.get().to_le_bytes());
            }
        }
        for r in self.asset.representations() {
            let record = RepresentationRecord::new(r.metadata, r.surface);
            let record = match r.atlas_map {
                Some(index) => {
                    let (offset, count) = Self::atlas_map_range(self.asset, index as usize)
                        .expect("validated atlas map");
                    record.with_atlas_map_range(offset, count)
                }
                None => record,
            };
            output.write(&record.encode_record().expect("validated representation"));
        }
        match self.asset.advance_source() {
            FontAdvanceSource::Advances(values) => {
                for value in values {
                    output.write(&value.to_le_bytes());
                }
            }
            FontAdvanceSource::Shaping(bytes) => output.write(bytes),
        }
        for metric in self.asset.raster_metrics() {
            output.write(&metric.encode_record());
        }
        let data_base = self.sections - self.asset.surface_count() as u16;
        let mut section = self.metadata_section_count();
        for (index, surface) in self.asset.surfaces().enumerate() {
            let map = surface.map();
            let (width, height) = map.cell_extent().unwrap_or((map.width(), map.height()));
            let mut record = GlyphSurfaceRecord::new(
                surface.descriptor().sample_layout(),
                map.packing(),
                width,
                height,
                data_base + index as u16,
            )
            .expect("validated surface");
            let mut index_section = None;
            let mut group_section = None;
            for (kind, _) in surface
                .plan()
                .expect("validated storage")
                .sections()
                .into_iter()
                .flatten()
            {
                match kind {
                    MediaSectionKind::PLANES => {
                        record = record.with_planes(section).expect("RAW storage")
                    }
                    MediaSectionKind::CODINGS => {
                        record = record.with_codings(section).expect("encoded storage")
                    }
                    MediaSectionKind::UNIT_GROUPS => group_section = Some(section),
                    MediaSectionKind::UNIT_INDEX => index_section = Some(section),
                    _ => unreachable!(),
                }
                section += 1;
            }
            if let Some(group) = group_section {
                record = record
                    .with_groups(group, index_section)
                    .expect("encoded groups");
            }
            output.write(&record.encode_record());
        }
        for map in self.asset.atlas_maps() {
            for region in map {
                output.write(&region.x().to_le_bytes());
                output.write(&region.y().to_le_bytes());
                output.write(&region.width().to_le_bytes());
                output.write(&region.height().to_le_bytes());
            }
        }
        for surface in self.asset.surfaces() {
            surface.plan().expect("validated storage").emit(&mut output);
        }
        if self.indexed {
            self.visit_data(|surface, storage, offset| {
                let mut start = 0;
                let end = storage.data_len() as u32;
                let whole = [end];
                let ends =
                    surface
                        .integrity()
                        .partitions()
                        .unwrap_or(if end == 0 { &[] } else { &whole });
                for &end in ends {
                    let crc = storage.data_crc(start..end);
                    let range =
                        IntegrityRange::new(offset as u32 + start..offset as u32 + end, crc)
                            .expect("validated integrity range");
                    output.write(&range.encode_record());
                    start = end;
                }
            });
        }
        debug_assert_eq!(output.position(), self.metadata_end);
        self.visit_data(|_, storage, offset| {
            output.begin_metadata();
            output.pad_to(offset);
            output.begin_data(if self.indexed {
                DataIntegrity::Indexed(&[])
            } else {
                DataIntegrity::Whole
            });
            storage.emit_data(&mut output);
        });
        debug_assert_eq!(
            output.position() + if self.indexed { 0 } else { MEDIA_CRC_LEN },
            self.len
        );
        output.finish()
    }
}

#[cfg(test)]
mod tests;
